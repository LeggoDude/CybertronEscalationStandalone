//! The start of a match: the map, the countdown and the players' list before the
//! match begins, as Escalation shows them.
//!
//! As a match starts the player hasn't come into it yet: there is no Jazz of his on
//! the map, and the game shows the map through its level camera (the CameraActor
//! its TnWorldInfo names as LevelCamera, TnSpectateTargetChooser.GetLevelCamera:
//! main.rs puts the view there). The Choose Character screen comes up over that
//! view (choose.rs); once he has chosen, the multiplayer UI controller's
//! GameStartUI (WaitingOnGameStart.OpenGameStartUI) shows the movie
//! UI_GFxPreGameCountdown_p.PreGameCountdown_GFX over it, with the players' list
//! (UI_GFxPlayerList_p.PlayerList_GFX in its 'InGame' view) loaded into it: the
//! starting scoreboard. The match begins when its countdown runs out: the movie
//! closes, Jazz spawns at his start and the match starts as the game starts it
//! (the announcer, the music: menu.rs's announce_match), the HUD with it.
//!
//! The countdown is COUNTDOWN_SECS, and it starts once every player in the match
//! has chosen his character: until then the movie shows "WAITING FOR PLAYERS..."
//! where the countdown goes (its waiting_mc, while IsCountingDown is false). The
//! game runs its countdown on the server (TnGameReplicationInfo's
//! CurrentCountdown); here the host runs it (net.rs: each PC says when its player
//! has chosen, the host starts the countdown once all have and sends it with the
//! lobby), so it is the same on every PC and the match begins for everyone
//! together. Playing without the network (no online match), it starts as the
//! player chooses. From 10 seconds down, each second ticks (its
//! LowCountdownTickSound, LowCountdownTickThreshold 10). Without the Choose
//! Character screen (no assets/choose), the player counts as having chosen as the
//! match starts. Until the match begins the other PCs don't show a player's Jazz
//! (main.rs's Player::waiting, sent in his snapshot).
//!
//! The movie, drawn through flash.rs in its numbers (stage pixels):
//!
//! * the view darkened through a blue gradient (sprite 4, shape 3, in the multiply
//!   blend mode), brought in to 60% over a second;
//! * the bands of light it opens with (hints_mc at (-103, 590), sprite 31 at
//!   (-103, 172): shapes 5, 7, 13 and 26 widening frame by frame at 30 frames a
//!   second, in the add blend mode at 25%), the game mode's icon by them (Escalation's
//!   mc_gameModeIconSV at 280%, turning in from -60 degrees as it fades in);
//! * the countdown (sprite 23 at (982, 97)): "MATCH STARTS IN" and the seconds, the
//!   number popping from twice its size with a white glow below 10, the label
//!   sliding to stay left of it; "WAITING FOR PLAYERS..." in its place until it
//!   starts, fading out as the countdown rises into view;
//! * the players' list at (320, 162), from a second after the movie opens (its
//!   first UpdatePlayerListData): the team (the Decepticons on their maps, the
//!   Autobots elsewhere) with its score, the columns (Level, Name, Score, Kills,
//!   Deaths, Revives, Money) and a row for each player, sorted as the movie sorts
//!   them. The list has the input, so this player's row is focused: the highlight
//!   (its glow_mc) fades in under it and its text goes dark. Nothing has been scored
//!   yet, so every number is 0. The game's own starting scoreboard has the
//!   single-team in-game view's Ability first (ListView::Ability); the demo shows
//!   the scoreboard in a match's Level there (the symbol of the class each player
//!   has chosen and his level in it: scoreboard.rs's rows), as it does everywhere
//!   else.
//!
//! The movie's game mode name (gameMode_txt) is left out: the game leaves
//! Escalation's empty (GameModeFriendlyName), as the reference shows.

use crate::flash::{
    Align, Blend, Canvas, Cx, DrawCmd, Ease, FlashFont, FlashFrame, FlashView, Glow, M3, STAGE_H,
    STAGE_W, TextSpec, Tween, key, text_width,
};
use crate::menu::{AppState, MenuText, Pause};
use crate::sound::Sfx;
use bevy::prelude::*;

/// The countdown, from the character's choice to the match (seconds).
pub const COUNTDOWN_SECS: u32 = 15;
/// From this many seconds left down to one, each new second ticks.
const TICK_FROM: u32 = 10;
/// The tick (TnGameReplicationInfo's LowCountdownTickSound, MP_COUNTDOWN_TICK).
const SND_TICK: &str = "BL_LVL_MP.MP_COUNTDOWN_BEEP";

const IMG_DIR: &str = "pregame/img";
const IMAGES: [&str; 5] = ["shade", "bar_top", "bar_wide", "hint_bar", "select_glow"];
/// The classes' symbols, in SPECIALTIES' order (the scoreboard's level cells).
const SPECIALTY_IMAGES: [&str; 4] = [
    "specialty_scout",
    "specialty_leader",
    "specialty_soldier",
    "specialty_scientist",
];
/// The lobby's mode icon and connection bars (tools/build_menu.py).
const MODE_ICON: &str = "menu/img/lobby_mode_icon.png";
const PING_IMAGES: [&str; 3] = [
    "menu/img/lobby_ping_1.png",
    "menu/img/lobby_ping_2.png",
    "menu/img/lobby_ping_3.png",
];

// ---------------------------------------------------------------------------
// The movie's numbers
// ---------------------------------------------------------------------------

/// The movies' frame rate.
const FPS: f32 = 30.0;
/// Sprite 4: the view darkened to this (its _alpha 60) over a second.
const SHADE_ALPHA: f32 = 0.6;
const SHADE_SECS: f32 = 1.0;
/// The bands' colour (#50b5d5), at 25% in the add blend mode.
const BAND: [f32; 3] = [80.0, 181.0, 213.0];
const BAND_ALPHA: f32 = 0.25;
/// hints_mc: the notched band (shape 5, 400 x 14, scale-9 from x 1 to 376) and the
/// box under it (shape 7, 400 x 36, 18 pixels down), each widening over ten
/// frames, the box four frames after the band.
const HINTS_AT: (f32, f32) = (-103.0, 590.0);
const HINT_BAR: (f32, f32) = (400.0, 14.0);
const HINT_BAR_BORDERS: (f32, f32) = (1.0, 24.0);
const HINT_BOX: (f32, f32, f32) = (18.0, 400.0, 36.0);
const HINT_BOX_DELAY: usize = 4;
const HINT_SCALE: [f32; 11] = [
    0.05, 0.2305, 0.392, 0.5345, 0.658, 0.7625, 0.848, 0.9145, 0.962, 0.9905, 1.0,
];
/// Sprite 31: the band under the countdown (shape 13, 414 x 25 above its origin,
/// scale-9 from x 1 to 376) widening over ten frames, and the long band over it
/// (shape 26, 300 x 25, 29 pixels up, scale-9 from x 1 to 277) from frame 4 to
/// 21, to 3.8 times its width.
const GROUP_AT: (f32, f32) = (-103.0, 172.0);
const BAR_TOP: (f32, f32) = (414.0, 25.0);
const BAR_TOP_BORDERS: (f32, f32) = (1.0, 38.0);
const BAR_TOP_SCALE: [f32; 11] = [
    0.0821, 0.2565, 0.4126, 0.5502, 0.6696, 0.7705, 0.8531, 0.9174, 0.9633, 0.9908, 1.0,
];
const BAR_WIDE: (f32, f32) = (300.0, 25.0);
const BAR_WIDE_Y: f32 = -29.0;
const BAR_WIDE_BORDERS: (f32, f32) = (1.0, 23.0);
const BAR_WIDE_FROM: usize = 4;
const BAR_WIDE_SCALE: [f32; 18] = [
    0.0667, 0.4929, 0.8934, 1.268, 1.6168, 1.9398, 2.2369, 2.5082, 2.7536, 2.9733, 3.1671, 3.335,
    3.4771, 3.5933, 3.6837, 3.7483, 3.7871, 3.8,
];
/// The game mode's icon (sprite 12 at (281, 115) in sprite 31, through
/// (0.309, 0.711, 0.840, 0.25) in the add blend mode): attached at 280%, at -60
/// degrees and unseen, it turns to 0 as it fades in (0.5 s). (The lobby's image
/// covers the icon's 32.65 pixels either way of its centre.)
const ICON_AT: (f32, f32) = (281.0, 115.0);
const ICON_SCALE: f32 = 2.8;
const ICON_HALF: f32 = 32.65;
const ICON_TINT: [f32; 4] = [0.308_593_75, 0.710_937_5, 0.839_843_75, 0.25];
const ICON_TURN: f32 = -60.0;
const ICON_SECS: f32 = 0.5;
/// The countdown, sprite 23 at (1085, -75) in sprite 31. Its text_mc rises from
/// 10 pixels down and fades in as the countdown starts (0.5 s) while waiting_mc
/// fades out; in
/// text_mc, time_mc at (-2, 15.9) (time_txt at (-15, -13.9), right-aligned to 3)
/// and label_mc at (-22, 15.9) (label_txt at (-145, -13.9), right-aligned to 3),
/// which slides to the left of the number each time it changes (0.2 s).
const COUNT_AT: (f32, f32) = (1085.0, -75.0);
const COUNT_RISE: f32 = 10.0;
const COUNT_SHOW_SECS: f32 = 0.5;
const TIME_AT: (f32, f32) = (-2.0, 15.9);
const TIME_TEXT_Y: f32 = -13.9;
const LABEL_AT: (f32, f32) = (-22.0, 15.9);
const LABEL_SECS: f32 = 0.2;
/// The text fields' right edges (their bounds' 18 and 148 from -15 and -145) less
/// Flash's 2 pixel gutter: where autoSize 'right' ends their text.
const TEXT_RIGHT: f32 = 1.0;
/// Both 14 pixel $NormalFont; the label #50b5d5, the number white.
const COUNT_SIZE: f32 = 14.0;
const COUNT_LABEL: [u8; 3] = [80, 181, 213];
/// Below 10 seconds each new number pops: from twice its size, a white glow under
/// it fading (its shadowAlpha 0.8 to 0, blur 8), over half a second.
const POP_BELOW: u32 = 10;
const POP_SECS: f32 = 0.5;
const POP_GLOW: Glow = Glow {
    color: [255, 255, 255],
    strength: 0.8,
    blur: 8.0,
};
/// waiting_mc at (-83, 22.4): "WAITING FOR PLAYERS..." centred (label_txt at
/// (-73, -19.7), 150 wide), until the countdown starts, then fading out (0.5 s).
const WAITING_AT: (f32, f32) = (-83.0, 22.4);
const WAITING_TEXT_Y: f32 = -19.7;

/// playerListAnchor_mc: the players' list, loaded into it, shows from its first
/// update, a second after the movie opens.
const LIST_AT: (f32, f32) = (320.0, 162.0);
const LIST_FIRST_SECS: f32 = 1.0;
/// The team's header (mc_teamHeader): its name (a 333 wide cell) and its score
/// (at 343, 130% and 8 pixels up), 28 pixel $NormalFont with the cell's text at
/// (2, 2); the columns' header 34 pixels down; the players' rows from 54 down,
/// 20 apart.
const TEAM_SIZE: f32 = 28.0;
const TEAM_TEXT: (f32, f32) = (2.0, 2.0);
const TEAM_SCORE_X: f32 = 343.0;
const TEAM_SCORE_Y: f32 = -8.0;
const TEAM_SCORE_SCALE: f32 = 1.3;
const COLUMNS_Y: f32 = 34.0;
const ROWS_Y: f32 = 54.0;
const ROW_H: f32 = 20.0;
/// The columns (ColumnHeaderDisplayModel for a single-team game's 'InGame' view:
/// _Ability0, PlayerName, Score, Kills, Deaths, _Revives, _SurvivalMoney): each
/// cell where the one before it ends (with 25 pixels before the name, 10 between
/// the rest), its 12 pixel #999999 label moved 10 pixels in (`cell_txt._x = 10`
/// places the field's box, so its text starts 12 in, as the game shows it), with a
/// faint white glow. The scoreboard in a match (scoreboard.rs) has the lobby's
/// view's first column in the ability's place, PlayerLevel (105 wide, the name 10
/// after it), and the rest after them as in the 'InGame' view.
const COLUMN_X: [f32; 7] = [0.0, 75.0, 335.0, 410.0, 485.0, 560.0, 635.0];
const LEVEL_COLUMN_X: [f32; 7] = [0.0, 115.0, 375.0, 450.0, 525.0, 600.0, 675.0];
const COLUMN_TEXT: (f32, f32) = (12.0, 2.0);
const COLUMN_SIZE: f32 = 12.0;
const COLUMN_COLOR: [u8; 3] = [153, 153, 153];
const COLUMN_GLOW: Glow = Glow {
    color: [255, 255, 255],
    strength: 0.148,
    blur: 4.0,
};
/// A row's cells (PlayerEntryDisplayModel): the name after the ability, connecting
/// and speaking icons (at 35, 50 and 70; the speaking one 15.05 wide), then the
/// five numbers 75 apart; the connection's bars (mc_ping, 32 x 32) 40 pixels
/// after the last number's cell starts, 5 up. Each cell's 16 pixel $NormalFont
/// text at (2, -1).
const NAME_X: f32 = 85.05;
const NUMBER_X: [f32; 5] = [345.05, 420.05, 495.05, 570.05, 645.05];
const PING_AT: (f32, f32) = (685.05, -5.0);
/// The level view's row (the lobby's PlayerEntryDisplayModel for its first cells:
/// mc_playerSpecialtyLevelAnchor at 35 and 11 down, the connecting icon 55 after it
/// and the speaking one 20 after that): the name at 125.05, the numbers from 385.05,
/// the connection's bars as far after the last as in the 'InGame' view, and the
/// highlight as wide as its list (PlayerListWidth: 775).
const LEVEL_AT: (f32, f32) = (35.0, 11.0);
const LEVEL_NAME_X: f32 = 125.05;
const LEVEL_NUMBER_X: [f32; 5] = [385.05, 460.05, 535.05, 610.05, 685.05];
const LEVEL_PING_AT: (f32, f32) = (725.05, -5.0);
const LEVEL_GLOW_W: f32 = 775.0;
/// The level cell (mc_playerSpecialtyLevelAnchor's updateCurrentCharacterString):
/// the class's symbol (the shared icons' mc_specialtyIcon<class>, its white shape
/// round the anchor: SPECIALTY_BOUNDS, in SPECIALTIES' order) at 55%, its height
/// growing from nothing, and the level (mc_playerSpecialtyLevelText: its level_txt,
/// 18 pixel $NormalFont, at (2, -13) in it) sliding from x -15 to 28 as it shrinks
/// from twice its size, both over 0.4 s, in the row's colour (getColorCodeFromLabel:
/// 'blue' #5AD1FE for the team's, 'hoverstate' #0A2828 for the focused row's).
const SPECIALTY_BOUNDS: [[f32; 4]; 4] = [
    [-31.7, -13.75, 31.75, 13.75],
    [-40.0, -16.3, 40.0, 15.05],
    [-44.1, -16.6, 44.1, 16.65],
    [-39.55, -16.3, 38.85, 15.65],
];
const SPECIALTY_SCALE: f32 = 0.55;
const SPECIALTY_SECS: f32 = 0.4;
const LEVEL_TEXT_X: (f32, f32) = (-15.0, 28.0);
const LEVEL_TEXT_AT: (f32, f32) = (2.0, -13.0);
const LEVEL_TEXT_SIZE: f32 = 18.0;
const LEVEL_BLUE: [u8; 3] = [90, 209, 254];
const COLOR_SECS: f32 = 0.2;
const PING_SIZE: f32 = 32.0;
const CELL_TEXT: (f32, f32) = (2.0, -1.0);
const CELL_SIZE: f32 = 16.0;
/// The row's highlight (glow_mc, the 256 x 32 image at (-10.65, 10.3) from its
/// middle row, scale-9 from x 10 to 250): as wide as the list (PlayerListWidth,
/// 735), it fades in (0.6 s) on the focused row.
const GLOW_AT: (f32, f32) = (-10.65, 10.3);
const GLOW_SIZE: (f32, f32) = (735.0, 32.0);
const GLOW_BORDERS: (f32, f32) = (10.0, 6.0);
const GLOW_TEX: (f32, f32) = (256.0, 32.0);
const GLOW_SECS: f32 = 0.6;
/// The rows' colours (checkTeamColorMode, the default colouring: the player's own
/// team is 'blue'): the team's header #a7e6fe with a #0980d7 glow (blur 10, its
/// rows' 6, strength 1.6); the focused row ('hoverstate') #0a2828 with none.
const TEAM_COLOR: [u8; 3] = [167, 230, 254];
const TEAM_GLOW: Glow = Glow {
    color: [9, 128, 215],
    strength: 1.6,
    blur: 10.0,
};
const ROW_GLOW: Glow = Glow {
    color: [9, 128, 215],
    strength: 1.6,
    blur: 6.0,
};
const FOCUSED_COLOR: [u8; 3] = [10, 40, 40];

// ---------------------------------------------------------------------------
// State
// ---------------------------------------------------------------------------

/// Where this PC's player is in the start of the match.
#[derive(Resource, Default)]
pub struct PreMatch {
    /// He isn't in the match yet: the Choose Character screen is up as it starts,
    /// the other players are still choosing, or the countdown is running. (The
    /// game's systems leave his Jazz out.)
    pub waiting: bool,
    /// When he chose his character (game time): the starting scoreboard opens.
    chosen_at: Option<f32>,
    /// When the countdown started here (game time: the movie's text comes in from
    /// then), once every player has chosen.
    counting_since: Option<f32>,
    /// The seconds shown, and when they last changed.
    shown: u32,
    changed_at: f32,
    /// The match begins in this frame: main.rs starts it (start_match) and Jazz
    /// takes up his character.
    pub begins: bool,
}

impl PreMatch {
    /// When the starting scoreboard opened (game time), while it is up.
    pub fn opened(&self) -> Option<f32> {
        self.chosen_at.filter(|_| self.waiting)
    }

    /// When the countdown started (game time), while it runs.
    pub fn counting_since(&self) -> Option<f32> {
        self.counting_since.filter(|_| self.waiting)
    }

    /// The class of the character he chose went to another player at the same
    /// moment (choose.rs's follow_classes): he chooses again, and the countdown
    /// (which waits for every player's choice) waits for him.
    pub fn choose_again(&mut self) {
        self.chosen_at = None;
        self.counting_since = None;
    }
}

/// The match's first frame for this PC: the countdown has just run out.
pub fn match_begins(pre: Res<PreMatch>) -> bool {
    pre.begins
}

/// The images (none drawn without them).
#[derive(Resource)]
pub(crate) struct PreGameImages {
    images: Vec<Handle<Image>>,
    icon: Handle<Image>,
    pings: [Handle<Image>; 3],
    /// The classes' symbols (SPECIALTY_IMAGES), where tools/build_pregame.py has
    /// made them.
    specialties: Option<[Handle<Image>; 4]>,
}

impl PreGameImages {
    pub(crate) fn get(&self, name: &str) -> &Handle<Image> {
        let i = IMAGES.iter().position(|n| *n == name).unwrap_or(0);
        &self.images[i]
    }
}

pub struct PreGamePlugin;

impl Plugin for PreGamePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<PreMatch>()
            .add_systems(Startup, load_images)
            .add_systems(OnExit(AppState::Game), leave)
            .add_systems(
                Update,
                draw_pregame
                    .run_if(in_state(AppState::Game))
                    .after(countdown)
                    .before(crate::flash::FlashSystems),
            );
    }
}

fn load_images(mut commands: Commands, asset_server: Res<AssetServer>) {
    let dir = crate::assets_dir();
    let missing: Vec<&str> = IMAGES
        .iter()
        .copied()
        .filter(|n| !dir.join(format!("{IMG_DIR}/{n}.png")).exists())
        .collect();
    if !missing.is_empty() {
        info!(
            "No starting scoreboard art ({missing:?}: tools/build_pregame.py makes \
             assets/pregame): the countdown shows without it"
        );
        return;
    }
    let specialties = SPECIALTY_IMAGES
        .iter()
        .all(|n| dir.join(format!("{IMG_DIR}/{n}.png")).exists())
        .then(|| SPECIALTY_IMAGES.map(|n| asset_server.load(format!("{IMG_DIR}/{n}.png"))));
    commands.insert_resource(PreGameImages {
        images: IMAGES
            .iter()
            .map(|n| asset_server.load(format!("{IMG_DIR}/{n}.png")))
            .collect(),
        icon: asset_server.load(MODE_ICON),
        pings: PING_IMAGES.map(|p| asset_server.load(p)),
        specialties,
    });
}

/// The match starts (OnEnter of the game, before main.rs sets it up): its player
/// isn't in it until the countdown has run.
pub fn enter(mut pre: ResMut<PreMatch>) {
    *pre = PreMatch {
        waiting: true,
        ..default()
    };
}

/// Leaving the game: nothing of the start is left.
fn leave(mut pre: ResMut<PreMatch>) {
    *pre = PreMatch::default();
}

/// The start of the match for this PC: the player has chosen once the Choose
/// Character screen the match starts with has gone; the countdown runs once every
/// player has (online, the host's count, net.rs; alone, from his choice), ticking
/// each second from TICK_FROM down; and at zero the match begins: Jazz takes up his
/// character (arm_chosen) and the match starts as the game starts it.
#[allow(clippy::too_many_arguments)]
pub fn countdown(
    time: Res<Time>,
    mut pre: ResMut<PreMatch>,
    pause: Res<Pause>,
    mut net: ResMut<crate::net::Net>,
    mut sfx: ResMut<Sfx>,
    mut chosen: crate::split::BySeat<crate::choose::Chosen>,
    mut huds: crate::split::BySeat<crate::hud::HudState>,
    current_map: Res<crate::CurrentMap>,
) {
    pre.begins = false;
    if !pre.waiting {
        return;
    }
    let now = time.elapsed_secs();
    // (Choose Character opened again from the pause menu doesn't take it back.)
    if pre.chosen_at.is_none() && !pause.choosing {
        pre.chosen_at = Some(now);
    }
    net.set_chosen(pre.chosen_at.is_some());
    // (Split screen's guest chooses after the first player, the screen up till he
    // has: choose.rs.)
    net.set_guest_chosen(pre.chosen_at.is_some());
    let Some(chosen_at) = pre.chosen_at else {
        return;
    };
    let full = COUNTDOWN_SECS as f32;
    let left = if net.in_match() {
        net.match_countdown()
    } else {
        Some((full - (now - chosen_at)).max(0.0))
    };
    // (Waiting for the other players.)
    let Some(left) = left else {
        return;
    };
    let shown = left.ceil() as u32;
    if pre.counting_since.is_none() {
        pre.counting_since = Some(now);
        pre.shown = shown;
        pre.changed_at = now;
    } else if shown != pre.shown {
        pre.shown = shown;
        pre.changed_at = now;
        if (1..=TICK_FROM).contains(&shown) {
            sfx.play(SND_TICK);
        }
    }
    if left <= 0.0 {
        pre.waiting = false;
        pre.chosen_at = None;
        pre.counting_since = None;
        pre.begins = true;
        for chosen in chosen.each() {
            chosen.arm_now = true;
        }
        crate::menu::announce_match(&mut sfx, huds.each(), current_map.0);
    }
}

// ---------------------------------------------------------------------------
// Drawing
// ---------------------------------------------------------------------------

/// What the drawing keeps from frame to frame: the label's slide.
struct Board {
    opened: Option<f32>,
    shown: Option<u32>,
    label_x: Tween,
}

impl Default for Board {
    fn default() -> Self {
        Board {
            opened: None,
            shown: None,
            label_x: Tween::at(LABEL_AT.0),
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn draw_pregame(
    time: Res<Time>,
    pre: Res<PreMatch>,
    pause: Res<Pause>,
    view: Res<FlashView>,
    text: Option<Res<MenuText>>,
    images: Option<Res<PreGameImages>>,
    net: Res<crate::net::Net>,
    current_map: Res<crate::CurrentMap>,
    mut frame: ResMut<FlashFrame>,
    mut board: Local<Board>,
    mut scoreboard: ResMut<crate::scoreboard::Scoreboard>,
) {
    let now = time.elapsed_secs();
    let Some(opened) = pre.opened() else {
        *board = Board::default();
        return;
    };
    let Some(text) = text else {
        return;
    };
    if board.opened != Some(opened) {
        *board = Board {
            opened: Some(opened),
            ..default()
        };
    }
    // (The pause menu, or Choose Character from it, takes its place while up.)
    if pause.open || pause.choosing {
        return;
    }
    let t = now - opened;
    let mut cmds: Vec<DrawCmd> = Vec::new();
    let mut c = Canvas { cmds: &mut cmds };
    if let Some(img) = images.as_deref() {
        draw_shade(&mut c, img, &view, t);
        draw_bands(&mut c, img, t);
    }
    draw_countdown(&mut c, &text, &pre, &mut board, now);
    if t >= LIST_FIRST_SECS {
        let decepticons = current_map
            .0
            .and_then(|m| crate::MAPS.get(m))
            .is_some_and(|m| m.decepticons);
        let rows = crate::scoreboard::rows(&net, &mut scoreboard, now);
        draw_list(
            &mut c,
            images.as_deref(),
            &text,
            &List {
                view: ListView::Level,
                rows: &rows,
                team_score: 0,
                decepticons,
                at: M3::translate(LIST_AT.0, LIST_AT.1, 0.0),
                t: t - LIST_FIRST_SECS,
            },
        );
    }
    frame.cmds.extend(cmds);
}

/// The frame of the movie's timeline `t` seconds in.
fn frame_at(t: f32) -> usize {
    (t.max(0.0) * FPS) as usize
}

/// A scale table's value at `frame` (from `first`), or None before it.
fn scale_at(table: &[f32], first: usize, frame: usize) -> Option<f32> {
    let i = frame.checked_sub(first)?;
    table.get(i.min(table.len() - 1)).copied()
}

fn band_cx() -> Cx {
    Cx::new(
        [
            BAND[0] / 255.0,
            BAND[1] / 255.0,
            BAND[2] / 255.0,
            BAND_ALPHA,
        ],
        [0.0; 4],
    )
}

/// A band (a white image, scale-9 across) from `(x, y)` (its top-left), `w` x `h`
/// at scale `s`, its ends `borders` wide kept.
#[allow(clippy::too_many_arguments)]
fn band(
    c: &mut Canvas,
    k: u64,
    image: &Handle<Image>,
    (x, y): (f32, f32),
    (w, h): (f32, f32),
    borders: (f32, f32),
    s: f32,
) {
    c.blended(
        k,
        Some(image),
        Some((
            [borders.0, 0.0, borders.1, 0.0],
            [borders.0 / w, 0.0, borders.1 / w, 0.0],
        )),
        [x, y, x + w * s, y + h],
        &M3::IDENTITY,
        band_cx(),
        Blend::Add,
    );
}

/// Sprite 4: the view through the gradient, multiplied, over the whole window.
pub(crate) fn draw_shade(c: &mut Canvas, img: &PreGameImages, view: &FlashView, t: f32) {
    let a = SHADE_ALPHA * Ease::RegularOut.apply(t / SHADE_SECS);
    shade(c, img, view, a);
}

/// The gradient over the whole window at alpha `a` (the end of the game's
/// background_mc, scoreboard.rs, has the same).
pub(crate) fn shade(c: &mut Canvas, img: &PreGameImages, view: &FlashView, a: f32) {
    let lo = view.to_stage(Vec2::ZERO);
    let hi = view.to_stage(view.window);
    let (x0, y0) = (lo.x.min(0.0) - 1.0, lo.y.min(0.0) - 1.0);
    let (x1, y1) = (hi.x.max(STAGE_W) + 1.0, hi.y.max(STAGE_H) + 1.0);
    c.blended(
        key("pg_shade", 0),
        Some(img.get("shade")),
        None,
        [x0, y0, x1, y1],
        &M3::IDENTITY,
        Cx::alpha(a),
        Blend::Multiply,
    );
}

/// hints_mc's bands, sprite 31's and the mode icon (the end of the game's
/// footer_mc and header_mc are the same: scoreboard.rs).
pub(crate) fn draw_bands(c: &mut Canvas, img: &PreGameImages, t: f32) {
    let f = frame_at(t);
    if let Some(s) = scale_at(&HINT_SCALE, 0, f) {
        band(
            c,
            key("pg_hint", 0),
            img.get("hint_bar"),
            HINTS_AT,
            HINT_BAR,
            HINT_BAR_BORDERS,
            s,
        );
    }
    if let Some(s) = scale_at(&HINT_SCALE, HINT_BOX_DELAY, f) {
        let (dy, w, h) = HINT_BOX;
        let (x, y) = (HINTS_AT.0, HINTS_AT.1 + dy);
        c.blended(
            key("pg_hint", 1),
            None,
            None,
            [x, y, x + w * s, y + h],
            &M3::IDENTITY,
            Cx::fill(BAND, BAND_ALPHA),
            Blend::Add,
        );
    }
    // The mode icon, under sprite 31's bands.
    let k = Ease::RegularOut.apply(t / ICON_SECS);
    let m = M3::translate(GROUP_AT.0 + ICON_AT.0, GROUP_AT.1 + ICON_AT.1, 0.0)
        .then(&M3::rot_z(ICON_TURN * (1.0 - k)))
        .then(&M3::scale(ICON_SCALE, ICON_SCALE));
    let mut tint = ICON_TINT;
    tint[3] *= k;
    c.blended(
        key("pg_icon", 0),
        Some(&img.icon),
        None,
        [-ICON_HALF, -ICON_HALF, ICON_HALF, ICON_HALF],
        &m,
        Cx::new(tint, [0.0; 4]),
        Blend::Add,
    );
    if let Some(s) = scale_at(&BAR_TOP_SCALE, 0, f) {
        band(
            c,
            key("pg_bar", 0),
            img.get("bar_top"),
            (GROUP_AT.0, GROUP_AT.1 - BAR_TOP.1),
            BAR_TOP,
            BAR_TOP_BORDERS,
            s,
        );
    }
    if let Some(s) = scale_at(&BAR_WIDE_SCALE, BAR_WIDE_FROM, f) {
        band(
            c,
            key("pg_bar", 1),
            img.get("bar_wide"),
            (GROUP_AT.0, GROUP_AT.1 + BAR_WIDE_Y - BAR_WIDE.1),
            BAR_WIDE,
            BAR_WIDE_BORDERS,
            s,
        );
    }
}

pub(crate) fn line(
    s: impl Into<String>,
    size: f32,
    color: [u8; 3],
    width: f32,
    glow: Option<Glow>,
) -> TextSpec {
    TextSpec {
        text: s.into(),
        font: FlashFont::Normal,
        size,
        color,
        align: Align::Left,
        width,
        shadow: None,
        glow,
        zoom: 1.0,
    }
}

/// Sprite 23: the waiting label until the countdown starts (showCountdown with
/// IsCountingDown false), then the countdown's label and number rising into view
/// as it fades out.
fn draw_countdown(c: &mut Canvas, text: &MenuText, pre: &PreMatch, board: &mut Board, now: f32) {
    let origin = (GROUP_AT.0 + COUNT_AT.0, GROUP_AT.1 + COUNT_AT.1);
    let show = pre.counting_since().map_or(0.0, |since| {
        Ease::RegularOut.apply((now - since) / COUNT_SHOW_SECS)
    });
    // waiting_mc.
    let waiting = text
        .get("MessagePrompts.WaitingForPlayers", "Waiting for players...")
        .to_uppercase();
    let ww = text_width(text, FlashFont::Normal, COUNT_SIZE, &waiting);
    c.text(
        key("pg_waiting", 0),
        line(waiting, COUNT_SIZE, COUNT_LABEL, ww + 4.0, None),
        origin.0 + WAITING_AT.0 - ww * 0.5,
        origin.1 + WAITING_AT.1 + WAITING_TEXT_Y,
        &M3::IDENTITY,
        Cx::alpha(1.0 - show),
    );
    // text_mc, rising into place.
    if pre.counting_since().is_none() {
        return;
    }
    let y = origin.1 + COUNT_RISE * (1.0 - show);
    let number = format!(" {}", pre.shown);
    let tw = text_width(text, FlashFont::Normal, COUNT_SIZE, &number);
    if board.shown != Some(pre.shown) {
        // updateCountdown: the label slides to the number's left (its field's
        // width, the text and Flash's two 2 pixel gutters).
        board.shown = Some(pre.shown);
        board
            .label_x
            .to(now, -(tw + 4.0), LABEL_SECS, Ease::RegularOut);
    }
    let pop = if pre.shown < POP_BELOW {
        Ease::RegularOut.apply((now - pre.changed_at) / POP_SECS)
    } else {
        1.0
    };
    let scale = 2.0 - pop;
    let glow = (pop < 1.0).then_some(Glow {
        strength: POP_GLOW.strength * (1.0 - pop),
        ..POP_GLOW
    });
    let m = M3::translate(origin.0 + TIME_AT.0, y + TIME_AT.1, 0.0).then(&M3::scale(scale, scale));
    c.text(
        key("pg_time", 0),
        TextSpec {
            zoom: scale,
            ..line(number, COUNT_SIZE, [255; 3], tw + 4.0, glow)
        },
        TEXT_RIGHT - tw,
        TIME_TEXT_Y,
        &m,
        Cx::alpha(show),
    );
    let label = text
        .get("Lobby.MultiplayerCountdownLabel", "Match starts in")
        .to_uppercase();
    let lw = text_width(text, FlashFont::Normal, COUNT_SIZE, &label);
    c.text(
        key("pg_label", 0),
        line(label, COUNT_SIZE, COUNT_LABEL, lw + 4.0, None),
        origin.0 + board.label_x.get(now) + TEXT_RIGHT - lw,
        y + LABEL_AT.1 + TIME_TEXT_Y,
        &M3::IDENTITY,
        Cx::alpha(show),
    );
}

/// The players' list's views: the game's starting scoreboard's (the single-team
/// 'InGame' view, the ability first: kept for its layout, the demo shows Level in
/// its place) and the scoreboard in a match's (scoreboard.rs: the class's symbol
/// and level first, the lobby's PlayerLevel cell), which every list here shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ListView {
    #[allow(dead_code)]
    Ability,
    Level,
}

impl ListView {
    fn columns(self) -> [f32; 7] {
        match self {
            ListView::Ability => COLUMN_X,
            ListView::Level => LEVEL_COLUMN_X,
        }
    }

    fn name_x(self) -> f32 {
        match self {
            ListView::Ability => NAME_X,
            ListView::Level => LEVEL_NAME_X,
        }
    }

    fn numbers(self) -> [f32; 5] {
        match self {
            ListView::Ability => NUMBER_X,
            ListView::Level => LEVEL_NUMBER_X,
        }
    }

    fn ping(self) -> (f32, f32) {
        match self {
            ListView::Ability => PING_AT,
            ListView::Level => LEVEL_PING_AT,
        }
    }

    fn glow_w(self) -> f32 {
        match self {
            ListView::Ability => GLOW_SIZE.0,
            ListView::Level => LEVEL_GLOW_W,
        }
    }

    /// The first column's label (its text's key and the game's English).
    fn first_column(self) -> (&'static str, &'static str) {
        match self {
            ListView::Ability => (
                "TransGame.TnPlayerReplicationInfoSurvival._Ability0.FriendlyName",
                "Ability",
            ),
            ListView::Level => (
                "TransGame.TnPlayerReplicationInfo.PlayerLevel.FriendlyName",
                "Level",
            ),
        }
    }

    /// The draws' keys, apart for each view.
    fn key(self, name: &str, i: usize) -> u64 {
        match self {
            ListView::Ability => key(name, i),
            ListView::Level => key(name, i + 1_000_000),
        }
    }
}

/// A player's row: his name, whether he is this PC's, his connection's round trip,
/// his character's class (its index in SPECIALTIES) with its level, from when it
/// came into the list (seconds ago), and his numbers (score, kills, deaths, revives,
/// money).
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct Row {
    pub name: String,
    pub me: bool,
    pub ping_ms: u16,
    pub class: Option<(usize, u32)>,
    pub class_shown_for: f32,
    pub numbers: [u32; 5],
}

/// Sorts the rows as the movie does (UpdatePlayerListData's sortOn: score and kills
/// down, deaths up, then the aliases, whatever their case).
pub(crate) fn sort_rows(rows: &mut [Row]) {
    rows.sort_by(|a, b| {
        b.numbers[0]
            .cmp(&a.numbers[0])
            .then(b.numbers[1].cmp(&a.numbers[1]))
            .then(a.numbers[2].cmp(&b.numbers[2]))
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
    });
}

/// A players' list to draw: its view, its rows (sorted), its team's score and
/// side, where the movie that loads it places it, and how long it has shown.
pub(crate) struct List<'a> {
    pub view: ListView,
    pub rows: &'a [Row],
    pub team_score: u32,
    pub decepticons: bool,
    pub at: M3,
    pub t: f32,
}

/// A colour tween from the art's own white to `rgb` (hms Tween on a colour: the
/// transform's multipliers going to 0 as its offsets go to the colour), `k` of the
/// way.
fn whitened(rgb: [u8; 3], k: f32) -> Cx {
    let m = 1.0 - k;
    Cx::new(
        [m, m, m, 1.0],
        [rgb[0] as f32 * k, rgb[1] as f32 * k, rgb[2] as f32 * k, 0.0],
    )
}

/// The players' list (PlayerList_GFX).
pub(crate) fn draw_list(c: &mut Canvas, img: Option<&PreGameImages>, text: &MenuText, list: &List) {
    let view = list.view;
    let at = list.at;
    // The team's header: its name and its score.
    let team = if list.decepticons {
        text.get("TransContent.TnTeamInfoDecepticons.TeamName", "Decepticons")
    } else {
        text.get("TransContent.TnTeamInfoAutobots.TeamName", "Autobots")
    }
    .to_uppercase();
    let width = text_width(text, FlashFont::Normal, TEAM_SIZE, &team) + 4.0;
    c.text(
        view.key("pg_team", 0),
        line(team, TEAM_SIZE, TEAM_COLOR, width, Some(TEAM_GLOW)),
        TEAM_TEXT.0,
        TEAM_TEXT.1,
        &at,
        Cx::IDENTITY,
    );
    let score = at
        .then(&M3::translate(TEAM_SCORE_X, TEAM_SCORE_Y, 0.0))
        .then(&M3::scale(TEAM_SCORE_SCALE, TEAM_SCORE_SCALE));
    let team_score = list.team_score.to_string();
    let width = text_width(text, FlashFont::Normal, TEAM_SIZE, &team_score) + 4.0;
    c.text(
        view.key("pg_team", 1),
        TextSpec {
            zoom: TEAM_SCORE_SCALE,
            ..line(team_score, TEAM_SIZE, TEAM_COLOR, width, Some(TEAM_GLOW))
        },
        TEAM_TEXT.0,
        TEAM_TEXT.1,
        &score,
        Cx::IDENTITY,
    );
    // The columns.
    let columns = [
        view.first_column(),
        (
            "TransGame.TnPlayerReplicationInfo.PlayerName.FriendlyName",
            "Name",
        ),
        (
            "TransGame.TnPlayerReplicationInfo.Score.FriendlyName",
            "Score",
        ),
        (
            "TransGame.TnPlayerReplicationInfo.Kills.FriendlyName",
            "Kills",
        ),
        (
            "TransGame.TnPlayerReplicationInfo.Deaths.FriendlyName",
            "Deaths",
        ),
        (
            "TransGame.TnPlayerReplicationInfoSurvival._Revives.FriendlyName",
            "Revives",
        ),
        (
            "TransGame.TnPlayerReplicationInfoSurvival._SurvivalMoney.FriendlyName",
            "Money",
        ),
    ];
    for (i, ((k, fallback), x)) in columns.iter().zip(view.columns()).enumerate() {
        let label = text.get(k, fallback).to_uppercase();
        let w = text_width(text, FlashFont::Normal, COLUMN_SIZE, &label) + 4.0;
        c.text(
            view.key("pg_column", i),
            line(label, COLUMN_SIZE, COLUMN_COLOR, w, Some(COLUMN_GLOW)),
            x + COLUMN_TEXT.0,
            COLUMNS_Y + COLUMN_TEXT.1,
            &at,
            Cx::IDENTITY,
        );
    }
    // The players.
    let glow_in = Ease::RegularOut.apply(list.t / GLOW_SECS);
    let numbers_x = view.numbers();
    for (i, row) in list.rows.iter().enumerate() {
        let y = ROWS_Y + i as f32 * ROW_H;
        if row.me
            && let Some(img) = img
        {
            let (gx, gy) = (GLOW_AT.0, y + GLOW_AT.1 - GLOW_SIZE.1 * 0.5);
            c.blended(
                view.key("pg_row_glow", i),
                Some(img.get("select_glow")),
                Some((
                    [GLOW_BORDERS.0, 0.0, GLOW_BORDERS.1, 0.0],
                    [
                        GLOW_BORDERS.0 / GLOW_TEX.0,
                        0.0,
                        GLOW_BORDERS.1 / GLOW_TEX.0,
                        0.0,
                    ],
                )),
                [gx, gy, gx + view.glow_w(), gy + GLOW_SIZE.1],
                &at,
                Cx::alpha(glow_in),
                Blend::Normal,
            );
        }
        let (color, glow) = if row.me {
            (FOCUSED_COLOR, None)
        } else {
            (TEAM_COLOR, Some(ROW_GLOW))
        };
        let cell = |s: &str| {
            let w = text_width(text, FlashFont::Normal, CELL_SIZE, s) + 4.0;
            line(s, CELL_SIZE, color, w, glow)
        };
        if view == ListView::Level
            && let Some((class, level)) = row.class
        {
            draw_level(
                c,
                img,
                text,
                list,
                i,
                y,
                row.me,
                class,
                level,
                row.class_shown_for,
            );
        }
        c.text(
            view.key("pg_row_name", i),
            cell(&row.name),
            view.name_x() + CELL_TEXT.0,
            y + CELL_TEXT.1,
            &at,
            Cx::IDENTITY,
        );
        for (j, (x, n)) in numbers_x.iter().zip(row.numbers).enumerate() {
            c.text(
                view.key("pg_row_number", i * numbers_x.len() + j),
                cell(&n.to_string()),
                x + CELL_TEXT.0,
                y + CELL_TEXT.1,
                &at,
                Cx::IDENTITY,
            );
        }
        if let Some(img) = img {
            // (As the lobby shows them: three bars under 80 ms, two under 160.)
            let bars = match row.ping_ms {
                0..80 => 2,
                80..160 => 1,
                _ => 0,
            };
            let (px, py) = (view.ping().0, y + view.ping().1);
            c.image(
                view.key("pg_row_ping", i),
                &img.pings[bars],
                [px, py, px + PING_SIZE, py + PING_SIZE],
                &at,
                Cx::IDENTITY,
            );
        }
    }
}

/// A row's level cell (mc_playerSpecialtyLevelAnchor): the class's symbol and its
/// level, `shown_for` seconds after they came in.
#[allow(clippy::too_many_arguments)]
fn draw_level(
    c: &mut Canvas,
    img: Option<&PreGameImages>,
    text: &MenuText,
    list: &List,
    i: usize,
    y: f32,
    focused: bool,
    class: usize,
    level: u32,
    shown_for: f32,
) {
    let view = list.view;
    let k = Ease::RegularOut.apply(shown_for / SPECIALTY_SECS);
    // The colour: from the art's white to the row's (0.2 s).
    let rgb = if focused { FOCUSED_COLOR } else { LEVEL_BLUE };
    let cx = whitened(rgb, Ease::RegularOut.apply(shown_for / COLOR_SECS));
    let (ax, ay) = (LEVEL_AT.0, y + LEVEL_AT.1);
    if let Some(icons) = img.and_then(|img| img.specialties.as_ref())
        && let (Some(icon), Some(b)) = (icons.get(class), SPECIALTY_BOUNDS.get(class))
    {
        let m = list
            .at
            .then(&M3::translate(ax, ay, 0.0))
            .then(&M3::scale(SPECIALTY_SCALE, SPECIALTY_SCALE * k));
        c.image(view.key("pg_row_class", i), icon, *b, &m, cx);
    }
    // mc_playerSpecialtyLevelText: sliding in from the left as it shrinks.
    let s = 2.0 - k;
    let x = LEVEL_TEXT_X.0 + (LEVEL_TEXT_X.1 - LEVEL_TEXT_X.0) * k;
    let m = list
        .at
        .then(&M3::translate(ax + x, ay, 0.0))
        .then(&M3::scale(s, s));
    let level = level.to_string();
    let w = text_width(text, FlashFont::Normal, LEVEL_TEXT_SIZE, &level) + 4.0;
    c.text(
        view.key("pg_row_level", i),
        TextSpec {
            zoom: s,
            ..line(level, LEVEL_TEXT_SIZE, [255; 3], w, None)
        },
        LEVEL_TEXT_AT.0,
        LEVEL_TEXT_AT.1,
        &m,
        cx,
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_view_lines_its_cells_up_under_its_columns() {
        for view in [ListView::Ability, ListView::Level] {
            let columns = view.columns();
            // The name and the numbers 10 pixels into their columns (the cells'
            // own spacing), the bars 40 after the last number, the highlight as
            // wide as the list.
            assert!(
                (view.name_x() - columns[1] - 10.05).abs() < 1e-3,
                "{view:?}"
            );
            for (x, column) in view.numbers().iter().zip(&columns[2..]) {
                assert!((x - column - 10.05).abs() < 1e-3, "{view:?}");
            }
            assert!((view.ping().0 - view.numbers()[4] - 40.0).abs() < 1e-3);
            assert!((view.glow_w() - (view.ping().0 + 50.0 - 0.05)).abs() < 1e-3);
        }
        // The level cell takes 40 pixels more than the ability's.
        assert_eq!(LEVEL_COLUMN_X[1] - COLUMN_X[1], 40.0);
    }

    #[test]
    fn rows_sort_as_the_movie_sorts_them() {
        let row = |name: &str, numbers: [u32; 5]| Row {
            name: name.to_string(),
            numbers,
            ..default()
        };
        let mut rows = vec![
            row("bee", [100, 3, 2, 0, 0]),
            row("Ace", [100, 3, 2, 0, 0]),
            row("cog", [100, 3, 1, 0, 0]),
            row("dux", [100, 4, 5, 0, 0]),
            row("eon", [250, 0, 9, 0, 0]),
        ];
        sort_rows(&mut rows);
        let names: Vec<&str> = rows.iter().map(|r| r.name.as_str()).collect();
        assert_eq!(names, ["eon", "dux", "cog", "Ace", "bee"]);
    }
}
