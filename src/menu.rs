//! The main menu shown before the map, rebuilt from Fall of Cybertron's front end:
//! the Scaleform movie UI_GFxFrontEnd_p.FrontEnd_GFX (its menu_Main clip) over the
//! Bink movie BG_RustSea, with the settings menu from
//! UI_GFxSettings_p.SettingsMenu_GFX. tools/build_menu.py makes assets/menu from the
//! game's files: the background movie, the images, the fonts and the text.
//!
//! The items are CUSTOMIZE, ESCALATION, SETTINGS, EXTRAS and EXIT GAME: the game's
//! list with CONTINUE and CAMPAIGN left out and MULTIPLAYER renamed CUSTOMIZE (the
//! game's own "Customize" text from its lobby).
//! ESCALATION opens the Escalation lobby (UI_GFxLobbies_p.MPLobby_GFX, with its
//! player list and the shared prompts): its party lobby hosts a game or joins one
//! by the host's IP address (net.rs), and a game's lobby lists the players, sets
//! the difficulty and, for the host, starts the match. SETTINGS opens the settings
//! menus (Display, which does nothing here; Graphics with Fullscreen Mode and a
//! Framerate Cap; Audio with a master volume and the game's FX, Dialog and Music
//! volumes; Controls, the game's, with its Keyboard/Mouse Layout screen, where keys
//! can also be changed: controls.rs), EXTRAS opens the Extras menu
//! (UI_GFxExtras_p.ExtrasMenu_GFX) with its Credits,
//! which play the game's ending credits movie, EXIT GAME quits, and CUSTOMIZE does
//! nothing.
//!
//! Positions are the movie's, in pixels of its 1120 x 720 stage, which is scaled to
//! the window's height and centred as Scaleform shows it. The animations follow the
//! movie's scripts (menu_Main's onEnterMainMenu / onExitMainMenuBegin and
//! MainMenuButton's stateHandler).

use crate::assets_dir;
use crate::controls::{self, Action, Controls, Input, Mode};
use crate::customize::CustomizeLink;
use crate::graphics::{GraphicsQuality, RENDER_SCALES, RenderScale, StartQuality};
use crate::net::{COUNTDOWN_SECS, DEFAULT_PORT, Failure, NAME_MAX, Net, NetStatus, PASSWORD_MAX};
use crate::padlayout::{self, Row as PadRow};
use crate::sound::{Loudness, Music, Sfx, SoundBank, SoundClass, Volumes};
use bevy::asset::RenderAssetUsages;
use bevy::audio::{AudioSinkPlayback, Volume};
use bevy::core_pipeline::tonemapping::Tonemapping;
use bevy::ecs::system::SystemParam;
use bevy::input::ButtonState as KeyState;
use bevy::input::keyboard::KeyboardInput;
use bevy::input::mouse::AccumulatedMouseScroll;
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use bevy::render::view::Hdr;
use bevy::window::{
    CursorGrabMode, CursorOptions, Monitor, MonitorSelection, PresentMode, PrimaryMonitor,
    PrimaryWindow, VideoMode, VideoModeSelection, WindowMode,
};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::mpsc::{Receiver, SyncSender, TryRecvError, sync_channel};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// Whether the menu or the game is running. The game's systems run in `Game`; its
/// world is built at startup, behind the menu.
#[derive(States, Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum AppState {
    #[default]
    Menu,
    Game,
}

/// The Audio settings' volumes (sound.rs's `Volumes`) as kept in settings.txt next
/// to the assets folder: each 0-1, and 1 (the game's 100) until it is set.
fn saved_volumes() -> Volumes {
    let saved = |name: &str| {
        saved_setting(name)
            .and_then(|v| v.parse::<f32>().ok())
            .map_or(1.0, |v| v.clamp(0.0, 1.0))
    };
    Volumes {
        master: saved("master_volume"),
        effects: saved("fx_volume"),
        dialog: saved("dialog_volume"),
        music: saved("music_volume"),
    }
}

/// Fullscreen Mode from the Graphics settings: the window covers the screen (in
/// the Resolution's video mode, or borderless at the desktop's) or is a normal
/// window. Kept in settings.txt too.
#[derive(Resource, Clone, Copy, PartialEq)]
pub struct Fullscreen(pub bool);

/// The Graphics menu's Resolution (GraphicResolution): the monitor's video mode
/// the game takes in fullscreen (the game's Fullscreen Mode is exclusive), and the
/// window's size otherwise; None, the desktop's (fullscreen borderless, as the
/// demo always was, and the window as it is). Kept in settings.txt as
/// `resolution <width>x<height>`.
#[derive(Resource, Clone, Copy, PartialEq, Eq, Debug)]
pub struct Resolution(pub Option<UVec2>);

impl Resolution {
    fn setting(self) -> (String, String) {
        let value = self
            .0
            .map_or("desktop".to_string(), |s| format!("{}x{}", s.x, s.y));
        ("resolution".to_string(), value)
    }

    /// Its text (the game's resolution label: PCSettings.GetResolution).
    fn text(self) -> String {
        self.0
            .map_or("Desktop".to_string(), |s| format!("{}x{}", s.x, s.y))
    }
}

impl Default for Resolution {
    fn default() -> Self {
        let saved = saved_setting("resolution").and_then(|v| {
            let (w, h) = v.split_once('x')?;
            Some(UVec2::new(w.parse().ok()?, h.parse().ok()?))
        });
        Resolution(saved.filter(|s| s.x > 0 && s.y > 0))
    }
}

/// VSync (the game's VSyncMode): each frame waits for the display (no tearing,
/// the display's pace), or shows as soon as it is done (Mailbox: no tearing where
/// the driver can; the Framerate Cap sets the pace). Off until it is set, as the
/// demo always was. Kept in settings.txt as `vsync`.
#[derive(Resource, Clone, Copy, PartialEq, Eq, Debug)]
pub struct VSync(pub bool);

impl Default for VSync {
    fn default() -> Self {
        VSync(saved_setting("vsync").is_some_and(|v| v == "1"))
    }
}

/// The monitor's resolutions for the Resolution slider: its video modes' sizes,
/// smallest first (none under MIN_RESOLUTION: the game's Error_ResolutionTooLow).
#[derive(Resource, Default, Clone, PartialEq, Debug)]
pub struct Resolutions(pub Vec<UVec2>);

const MIN_RESOLUTION: UVec2 = UVec2::new(800, 600);

/// Fullscreen Mode, Resolution and VSync as changed in the Graphics menu, waiting
/// for Accept (the game's GraphicsMenuStore: its gfxChange rows take effect
/// together with updateGfxProfiles; Back leaves them as they were).
#[derive(Resource, Default, Clone, Copy, PartialEq, Eq, Debug)]
pub struct PendingDisplay(Option<DisplayChoice>);

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
struct DisplayChoice {
    fullscreen: bool,
    resolution: Option<UVec2>,
    vsync: bool,
}

impl Default for Fullscreen {
    fn default() -> Self {
        Fullscreen(saved_setting("fullscreen").is_some_and(|v| v == "1"))
    }
}

/// The Framerate Cap from the Graphics settings: an index into `FRAMERATE_CAPS`
/// (30, 60, 90 or 120 frames a second, or none). Kept in settings.txt too (as the
/// number, 0 for none); 60 until it is set.
#[derive(Resource, Clone, Copy, PartialEq)]
pub struct FramerateCap(pub usize);

/// The Framerate Cap's choices, in the stepper's order (None: no cap).
const FRAMERATE_CAPS: [Option<u32>; 5] = [Some(30), Some(60), Some(90), Some(120), None];

impl FramerateCap {
    /// Frames a second, or None for no cap.
    pub fn fps(self) -> Option<u32> {
        FRAMERATE_CAPS.get(self.0).copied().flatten()
    }
}

impl Default for FramerateCap {
    fn default() -> Self {
        let saved = saved_setting("framerate_cap").and_then(|v| v.parse::<u32>().ok());
        let fps = match saved {
            Some(0) => None,
            Some(n) => Some(n),
            None => Some(60),
        };
        let index = FRAMERATE_CAPS.iter().position(|&c| c == fps).unwrap_or(1);
        FramerateCap(index)
    }
}

/// The Display settings (the settings movie's DisplayMenuProvider), kept in
/// settings.txt: Brightness (the profile's GammaSetting, 0 to 100, 50 until it is
/// set), the Safe Frame (ScreenSafeFrameX and Y: the share of the screen's width
/// and height the HUD keeps clear at its edges, half on each side, 0 to 0.18; 0 on
/// PC until it is set, HudData's DefaultScreenSafeFrame) and Team Coloring Mode
/// (MpColoringMode: Ally vs Enemy, the game's default, or Autobot vs Decepticon,
/// `faction_colors`); and the demo's own Split Screen (split.rs: the two players'
/// views one above the other, as by default, or side by side).
#[derive(Resource, Clone, Copy, PartialEq, Debug)]
pub struct DisplaySettings {
    pub brightness: f32,
    pub safe_frame: Vec2,
    pub faction_colors: bool,
    pub split_side_by_side: bool,
}

/// The Safe Frame screen's largest frame (MaxSafeFrame) and its step
/// (AdjustIncrement).
pub const MAX_SAFE_FRAME: f32 = 0.18;
const SAFE_FRAME_STEP: f32 = 0.005;

impl Default for DisplaySettings {
    fn default() -> Self {
        let number = |name: &str| saved_setting(name).and_then(|v| v.parse::<f32>().ok());
        let frame = |name: &str| number(name).map_or(0.0, |v| v.clamp(0.0, MAX_SAFE_FRAME));
        DisplaySettings {
            brightness: number("brightness").map_or(50.0, |v| v.clamp(0.0, 100.0)),
            safe_frame: Vec2::new(frame("safe_frame_x"), frame("safe_frame_y")),
            faction_colors: saved_setting("team_coloring").is_some_and(|v| v == "1"),
            split_side_by_side: saved_setting("split_screen").is_some_and(|v| v == SIDE_BY_SIDE),
        }
    }
}

/// Split Screen's values in settings.txt.
const SIDE_BY_SIDE: &str = "side_by_side";
const TOP_AND_BOTTOM: &str = "top_and_bottom";

impl DisplaySettings {
    /// The display gamma Brightness sets: 2.2, 0.95 lower or higher at the
    /// slider's ends (HmProfileSettings.GetGammaSetting: GAMMA_CENTER and
    /// GAMMA_HALFRANGE).
    pub fn gamma(&self) -> f32 {
        2.2 + 0.95 * (2.0 * (self.brightness / 100.0).clamp(0.0, 1.0) - 1.0)
    }

    /// Its lines in settings.txt.
    fn settings(&self) -> Vec<(String, String)> {
        vec![
            ("brightness".to_string(), format!("{:.0}", self.brightness)),
            (
                "safe_frame_x".to_string(),
                format!("{:.3}", self.safe_frame.x),
            ),
            (
                "safe_frame_y".to_string(),
                format!("{:.3}", self.safe_frame.y),
            ),
            (
                "team_coloring".to_string(),
                u8::from(self.faction_colors).to_string(),
            ),
            (
                "split_screen".to_string(),
                if self.split_side_by_side {
                    SIDE_BY_SIDE
                } else {
                    TOP_AND_BOTTOM
                }
                .to_string(),
            ),
        ]
    }
}

/// The settings, for the menus' drawing.
#[derive(SystemParam)]
pub(crate) struct SettingsView<'w> {
    volumes: Res<'w, Volumes>,
    fullscreen: Res<'w, Fullscreen>,
    cap: Res<'w, FramerateCap>,
    controls: Res<'w, Controls>,
    display: Res<'w, DisplaySettings>,
    quality: Res<'w, GraphicsQuality>,
    screen: SettingsScreen<'w>,
}

/// The Graphics menu's display settings (applied and waiting for Accept), the
/// monitor's resolutions and the Render Scale, for the menus' drawing.
#[derive(SystemParam)]
pub(crate) struct SettingsScreen<'w> {
    resolution: Res<'w, Resolution>,
    vsync: Res<'w, VSync>,
    pending: Res<'w, PendingDisplay>,
    resolutions: Res<'w, Resolutions>,
    render_scale: Res<'w, RenderScale>,
}

impl SettingsScreen<'_> {
    fn choice(&self, fullscreen: &Fullscreen) -> DisplayChoice {
        self.pending.0.unwrap_or(DisplayChoice {
            fullscreen: fullscreen.0,
            resolution: self.resolution.0,
            vsync: self.vsync.0,
        })
    }
}

/// The settings the menus change, for their input.
#[derive(SystemParam)]
pub(crate) struct Settings<'w> {
    volumes: ResMut<'w, Volumes>,
    fullscreen: ResMut<'w, Fullscreen>,
    cap: ResMut<'w, FramerateCap>,
    controls: ResMut<'w, Controls>,
    display: ResMut<'w, DisplaySettings>,
    quality: ResMut<'w, GraphicsQuality>,
    /// The Graphics level the demo started at (its textures', till it starts again).
    start_quality: Res<'w, StartQuality>,
    resolution: ResMut<'w, Resolution>,
    vsync: ResMut<'w, VSync>,
    pending: ResMut<'w, PendingDisplay>,
    resolutions: Res<'w, Resolutions>,
    render_scale: ResMut<'w, RenderScale>,
}

impl Settings<'_> {
    /// The steppers' values, with the Keyboard/Mouse Layout screen showing `view`'s
    /// keys.
    fn choices(&self, view: Mode) -> Choices {
        choices(
            (self.display_choice(), &self.cap, &self.quality),
            &self.controls,
            &self.display,
            view,
        )
    }

    /// Fullscreen Mode, Resolution and VSync as the Graphics menu shows them:
    /// those waiting for Accept, or else those in effect.
    fn display_choice(&self) -> DisplayChoice {
        self.pending.0.unwrap_or(DisplayChoice {
            fullscreen: self.fullscreen.0,
            resolution: self.resolution.0,
            vsync: self.vsync.0,
        })
    }

    /// A change to them, kept for Accept.
    fn change_display(&mut self, change: impl FnOnce(&mut DisplayChoice)) {
        let mut choice = self.display_choice();
        change(&mut choice);
        self.pending.0 = Some(choice);
    }

    /// Whether changes wait for Accept.
    fn display_pending(&self) -> bool {
        self.pending.0.is_some_and(|p| {
            p != DisplayChoice {
                fullscreen: self.fullscreen.0,
                resolution: self.resolution.0,
                vsync: self.vsync.0,
            }
        })
    }

    /// The sliders' values.
    fn sliders(&self) -> SliderSource<'_> {
        SliderSource {
            volumes: &self.volumes,
            controls: &self.controls,
            resolution: self.display_choice().resolution,
            resolutions: &self.resolutions.0,
            render_scale: self.render_scale.0,
        }
    }
}

fn choices(
    (display_choice, cap, quality): (DisplayChoice, &FramerateCap, &GraphicsQuality),
    controls: &Controls,
    display: &DisplaySettings,
    view: Mode,
) -> Choices {
    Choices {
        fullscreen: display_choice.fullscreen,
        vsync: display_choice.vsync,
        cap: cap.0,
        quality: quality.0,
        faction_colors: display.faction_colors,
        split_side: display.split_side_by_side,
        vibration: controls.vibration,
        hold_boost: controls.hold_boost,
        stick_delay: controls.stick_delay,
        invert: controls.invert,
        controller: controls.controller,
        layout: controls.layout,
        layouts: controls.layouts(),
        view,
        pad: controls.pad_schemes,
    }
}

/// A value saved in settings.txt (lines of "<name> <value>").
pub(crate) fn saved_setting(name: &str) -> Option<String> {
    let text = std::fs::read_to_string(settings_path()).ok()?;
    text.lines().find_map(|line| {
        let mut parts = line.split_whitespace();
        (parts.next() == Some(name))
            .then(|| parts.next().map(str::to_string))
            .flatten()
    })
}

/// Every value saved in settings.txt, by name.
pub(crate) fn read_settings() -> HashMap<String, String> {
    let text = std::fs::read_to_string(settings_path()).unwrap_or_default();
    text.lines()
        .filter_map(|line| {
            let mut parts = line.split_whitespace();
            Some((parts.next()?.to_string(), parts.next()?.to_string()))
        })
        .collect()
}

/// Writes `values` into settings.txt in place of the lines with their names (and
/// of any line whose name starts with `replace`), keeping the other lines.
fn write_settings(values: &[(String, String)], replace: Option<&str>) {
    let text = std::fs::read_to_string(settings_path()).unwrap_or_default();
    let mut lines: Vec<String> = text
        .lines()
        .filter(|line| {
            let name = line.split_whitespace().next().unwrap_or("");
            !name.is_empty()
                && !values.iter().any(|(n, _)| n == name)
                && !replace.is_some_and(|p| name.starts_with(p))
        })
        .map(str::to_string)
        .collect();
    lines.extend(values.iter().map(|(name, value)| format!("{name} {value}")));
    let _ = std::fs::write(settings_path(), lines.join("\n") + "\n");
}

/// Saves the settings to settings.txt, and Join Game's address (`address`; the one
/// saved before stays if it's None, and goes if it's empty).
fn save_settings(
    volumes: Volumes,
    fullscreen: Fullscreen,
    cap: FramerateCap,
    address: Option<&str>,
) {
    let volume = |name: &str, v: f32| (name.to_string(), format!("{v:.2}"));
    let mut values = vec![
        volume("master_volume", volumes.master),
        volume("fx_volume", volumes.effects),
        volume("dialog_volume", volumes.dialog),
        volume("music_volume", volumes.music),
        ("fullscreen".to_string(), u8::from(fullscreen.0).to_string()),
        (
            "framerate_cap".to_string(),
            cap.fps().unwrap_or(0).to_string(),
        ),
    ];
    let mut replace = None;
    match address {
        Some("") => replace = Some("join_address"),
        Some(address) => values.push(("join_address".to_string(), address.to_string())),
        None => {}
    }
    write_settings(&values, replace);
}

/// Display Name's name, as saved in settings.txt (the rest of its line: it may
/// have spaces).
fn saved_display_name() -> Option<String> {
    let text = std::fs::read_to_string(settings_path()).ok()?;
    text.lines().find_map(|line| {
        let rest = line.strip_prefix("display_name ")?.trim();
        (!rest.is_empty()).then(|| rest.to_string())
    })
}

/// Saves Display Name's name to settings.txt.
fn save_display_name(name: &str) {
    write_settings(&[("display_name".to_string(), name.to_string())], None);
}

/// The player's name as he set it last time (Display Name), in place of the
/// default, Bumblebee.
fn load_display_name(mut net: ResMut<Net>) {
    if let Some(name) = saved_display_name() {
        net.set_name(&name);
    }
}

fn settings_path() -> PathBuf {
    let assets = assets_dir();
    assets
        .parent()
        .map(|p| p.join("settings.txt"))
        .unwrap_or_else(|| PathBuf::from("settings.txt"))
}

pub struct MenuPlugin;

/// The menu's own systems (screens drawn over it, like Customize, run before them).
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) struct MenuSystems;

impl Plugin for MenuPlugin {
    fn build(&self, app: &mut App) {
        app.init_state::<AppState>()
            .insert_resource(saved_volumes())
            .init_resource::<Fullscreen>()
            .init_resource::<Resolution>()
            .init_resource::<VSync>()
            .init_resource::<Resolutions>()
            .init_resource::<PendingDisplay>()
            .init_resource::<FramerateCap>()
            .init_resource::<DisplaySettings>()
            .insert_resource(Controls::load())
            .add_systems(Startup, (setup_menu, load_display_name))
            .add_systems(
                Update,
                (
                    prime_mode,
                    (
                        menu_input,
                        menu_credits,
                        menu_movie,
                        menu_music,
                        crate::sound::play_sounds,
                    )
                        .chain()
                        .run_if(in_state(AppState::Menu)),
                    make_display_art,
                    layout_menu,
                    crate::sound::follow_volumes,
                    list_resolutions,
                    apply_display,
                    save_changed_settings,
                )
                    .chain()
                    .in_set(MenuSystems),
            )
            // The cap holds each frame back until its time is up.
            .add_systems(Last, limit_framerate)
            .init_resource::<Pause>()
            .add_systems(OnEnter(AppState::Game), enter_game)
            .add_systems(OnEnter(AppState::Menu), reenter_menu);
    }
}

// ---------------------------------------------------------------------------
// The movie's layout
// ---------------------------------------------------------------------------

const STAGE_W: f32 = 1120.0;
const STAGE_H: f32 = 720.0;

/// logo_mc: placed at (672, 166.75) at 84.34%; its image (the INT logo,
/// 1024 x 256) is drawn 920 x 230 around that point.
const LOGO_POS: Vec2 = Vec2::new(672.0, 166.75);
const LOGO_SCALE: f32 = 0.8434;
const LOGO_SIZE: Vec2 = Vec2::new(920.0, 230.0);
/// The logo's own timeline fades it in over its first 38 frames (at 30 fps).
const LOGO_FADE_SECS: f32 = 38.0 / 30.0;

/// shell_mc, which holds the menu: it starts at y 480 and slides down 80 pixels
/// in 0.3 s when the menu is attached.
const SHELL_X: f32 = 560.0;
const SHELL_Y: f32 = 560.0;
const SHELL_DROP: f32 = 80.0;

/// The buttons (MainMenuButton) sit at x 43 in the shell, at 105% x 109%. The
/// game's list is CONTINUE, CAMPAIGN, MULTIPLAYER, ESCALATION, SETTINGS, EXTRAS
/// (then EXIT GAME, hidden on consoles). Without the first two, the five left take
/// the rows from CAMPAIGN's down, so the list ends on the row where EXTRAS does.
const BUTTON_X: f32 = 43.0;
const BUTTON_SCALE: Vec2 = Vec2::new(1.05, 1.09);
const SLOTS: [f32; 5] = [-240.0, -197.0, -153.0, -111.0, -68.0];
/// The label (28 px $NormalFont, upper case, right-aligned): where its text ends
/// and its baseline, from the button's origin. Its textField (at 2, 2, its box
/// [-2, 398] x [-2, 37.4]) is kept at its own size as the button is scaled (CLIK's
/// Constraints, scaled: the field counter-scaled, its margins kept), so the text
/// ends 2 pixels inside the box's right (398) and its first line's baseline is
/// the box's top plus the 2-pixel gutter and the font's ascent (2 + 0.99 x 28):
/// centred in the brackets, which take the button's scale.
const LABEL_SIZE: f32 = 28.0;
const LABEL_RIGHT: f32 = 398.0;
const LABEL_BASELINE: f32 = 2.0 + ASCENT * LABEL_SIZE;
/// Label colours: "up" and "over" (the button's textField per state).
const LABEL_UP: Color = Color::srgb(1.0, 208.0 / 255.0, 91.0 / 255.0);
const LABEL_OVER: Color = Color::srgb(187.0 / 255.0, 1.0, 1.0);
/// The focused button slides 15 pixels left.
const OVER_SHIFT: f32 = -15.0;
/// The brackets around the focused button (the clip "brackets", a 229 x 47 image
/// with a scale-9 grid whose corners are 24 and 25 pixels wide): at
/// x = 363 - textWidth, width textWidth + 36, y -4, in the button's space.
const BRACKET_IMAGE: Vec2 = Vec2::new(229.0, 47.0);
const BRACKET_LEFT: f32 = 24.0;
const BRACKET_RIGHT: f32 = 25.0;

/// summary_txt: the focused item's description, 20 px $NormalFont, white,
/// right-aligned to x 440 of the shell, with a dark red glow.
const SUMMARY_SIZE: f32 = 20.0;
const SUMMARY_RIGHT: f32 = 439.95;
const SUMMARY_BASELINE: f32 = 21.3;
const SUMMARY_GLOW: Color = Color::srgba(0.2, 0.0, 0.0, 0.4);

/// The demo's version at the main menu's bottom left (in the window's corner, a
/// wide window's past the menu's stage): small, faint, as the main menu fades in.
const VERSION_SIZE: f32 = 14.0;
const VERSION_MARGIN: Vec2 = Vec2::new(16.0, 14.0);
const VERSION_COLOR: Color = Color::srgba(1.0, 1.0, 1.0, 0.6);

/// What the main menu says the demo's version is: the installer's when
/// tools/make_installer.py builds it (FOC_ESC_VERSION: the date and time its
/// Setup was made, as Setup is named, so a player can tell which he has), or the
/// package's for a build of one's own (`boot demo cargo.bat`).
fn version_label() -> String {
    match option_env!("FOC_ESC_VERSION").map(str::trim) {
        Some(v) if !v.is_empty() => format!("Version {v}"),
        _ => format!("Version {} (dev build)", env!("CARGO_PKG_VERSION")),
    }
}

/// The button hint bar under the menu: the gamepad button and its action
/// ($TitleFont, upper case).
const HINT_CENTER: Vec2 = Vec2::new(559.0, 619.5);
const HINT_ICON: f32 = 21.0;
const HINT_SIZE: f32 = 14.0;
const HINT_COLOR: Color = Color::srgb(1.0, 208.0 / 255.0, 91.0 / 255.0);

/// $NormalFont and $TitleFont ascents (Register Demi Bold / Medium), in ems.
const ASCENT: f32 = 0.990;

/// The settings menus (mc_menu): anchored at (243, 360), their rows 27 pixels
/// apart and centred on the anchor (row i of n at i * 27 - n * 27 / 2). The whole
/// menu, with its title and selection bar, drifts by -15% of the focused row's
/// offset (0.3 s, Regular.easeOut).
const SET_ANCHOR: Vec2 = Vec2::new(243.0, 360.0);
const SET_ROW_H: f32 = 27.0;
const SET_DRIFT: f32 = -0.15;
/// The selection bar's art (SharedAssets mc_selectionBarArt, the settings and
/// pause menus') has a scale-9 grid (DefineScalingGrid: x 10 to 254, y -1 to 1 of
/// its 256 x 32 bitmap from y -16): however far the bar is stretched, its notched
/// left end and its right end keep their size, clear of the focused row's text.
const SELECTION_BAR_GRID: BorderRect = BorderRect {
    min_inset: Vec2::new(10.0, 15.0),
    max_inset: Vec2::new(2.0, 15.0),
};
/// Its background art (mc_menuBackgroundArt: 256 x 256 at x -36 + 100 -> 0,
/// scale 430% -> 469% by 281%, alpha 0 -> 10% over 13 frames).
const SET_ART_SECS: f32 = 13.0 / 30.0;
/// The menus' background art (SharedAssets' mc_menuBackgroundArt, the settings and
/// pause menus'; its image MenuBackground, 256 x 256, has an arrow-shaped notch in
/// the middle of its left edge) has a scale-9 grid too (DefineScalingGrid: x 22 to
/// 255, y -0.5 to 0.5 about its middle): however far it is stretched, its left 22
/// pixels and right one keep their width and its top and bottom 127.5 rows their
/// height, and only the one row through the notch's tip stretches up and down. So
/// the panel's left edge runs straight down from the tip of the notch, its two
/// slopes pushed out to the ends of that band, 233 pixels above and below the
/// middle (drawn at the stage's scale: hold_slice_scale).
const MENU_ART_GRID: BorderRect = BorderRect {
    min_inset: Vec2::new(22.0, 127.5),
    max_inset: Vec2::new(1.0, 127.5),
};
/// The title ($NormalFont 32 px, white, upper case).
const SET_TITLE_SIZE: f32 = 32.0;
/// The slider (MenuSlider): label at 2, 18 px $NormalFont #66ffe4; the track and
/// bar 200 x 32 at x 298, the arrows 26 x 26 at x 310 (mirrored) and 486.
const SET_LABEL_SIZE: f32 = 18.0;
/// The focused row is drawn dark on the bright selection bar: its parts' colour
/// transform in the "focused" / "over" states (the label slides 5 pixels left).
const SET_FOCUS_TINT: [f32; 3] = [0.0586, 0.281, 0.281];
const SET_LABEL_COLOR: Color = Color::srgb(0.4 * 0.0586, 0.281, 228.0 / 255.0 * 0.281);
/// A row's label ($NormalFont 18 px, #66ffe4) when not focused; it darkens and
/// slides 5 pixels left as it takes the focus, and back as it loses it (the
/// MenuItemStepper / MenuSlider "focused" and "default" states, 9 frames each).
const SET_LABEL_UP: Color = Color::srgb(0.4, 1.0, 228.0 / 255.0);
const SET_FOCUS_SECS: f32 = 0.3;
/// Where the menus' texts start, from their rows' tops (and the title from its
/// row's): the fields' placement points, as measured against the game's Controls
/// menu (the labels' text 2 pixels in and 2.5 down; the values' 2.5 in from their
/// renderers and 4.3 down; the title's 3 pixels left and 36.5 up).
const SET_LABEL_DX: f32 = 2.0;
const SET_LABEL_DY: f32 = 2.5;
const SET_VALUE_DX: f32 = 2.5;
const SET_VALUE_DY: f32 = 4.3;
const SET_TITLE_DX: f32 = -3.0;
const SET_TITLE_DY: f32 = -36.5;
/// A stepper's values (MenuItemStepper): renderers in a container 300 pixels in,
/// "$NormalFont" 15 px #66ffe4, the selected one full ("on") and the others at
/// 25% without red ("off"), over 8 frames. Focused, all are shown side by side;
/// otherwise only the selected one, at the start (0.2 s tweens).
const SET_VALUE_SIZE: f32 = 15.0;
/// A slider's text (Resolution's, Render Scale's): this far past its arrow.
const SLIDER_LABEL_GAP: f32 = 14.0;
const SET_VALUE_X: f32 = 300.0;
const STEP_GAP: f32 = 5.3;
const STEP_SECS: f32 = 0.2;
const STEP_STATE_SECS: f32 = 8.0 / 30.0;
/// The PC version's Back button under the settings (stage pixels and colours, as
/// the game draws it with mouse and keyboard): it closes the open menu.
const BACK_CENTER: Vec2 = Vec2::new(560.0, 669.0);
const BACK_SIZE: Vec2 = Vec2::new(132.0, 36.0);
const BACK_FILL: Color = Color::srgb(9.0 / 255.0, 34.0 / 255.0, 41.0 / 255.0);
const BACK_TEXT: Color = Color::srgb(166.0 / 255.0, 246.0 / 255.0, 1.0);
const BACK_TEXT_OVER: Color = Color::WHITE;
const BACK_TEXT_SIZE: f32 = 14.0;
/// The Display screens' Accept button (their button hints' A: Accept, shown once
/// something has changed), beside Back.
const ACCEPT_CENTER: Vec2 = Vec2::new(420.0, 669.0);

/// The Brightness screen (Brightness_GFX, over CalibrationImage_GFX): the
/// calibration picture (512 x 256, centred at (564, 355)), fading in over 18 frames
/// (its sprite 7), the glowing panel behind it (sprite 20: a 64 x 512 image at 8
/// times its width, its scale-9 grid keeping 22 pixels at either side, from (304,
/// 168)), the slider (slider_mc at (330, 500): the TransSlider at 3.03 times its
/// width, 460 pixels; its arrows outside it), and the directions under it (text_txt:
/// one line, 16 px white, its field sized to it about x 560, from y 562; Safe
/// Frame's the same).
const CALIBRATION: Rect = Rect {
    min: Vec2::new(308.0, 227.0),
    max: Vec2::new(820.0, 483.0),
};
const BRIGHTNESS_PANEL: Rect = Rect {
    min: Vec2::new(304.0, 168.0),
    max: Vec2::new(816.0, 680.0),
};
const BRIGHTNESS_PANEL_EDGE: f32 = 22.0;
const BRIGHTNESS_TRACK: Rect = Rect {
    min: Vec2::new(330.0, 500.0),
    max: Vec2::new(790.0, 532.0),
};
const SCREEN_FADE_SECS: f32 = 18.0 / 30.0;
const DIRECTIONS_TOP: f32 = 562.0;
const DIRECTIONS_SIZE: f32 = 16.0;
/// The TransSlider's parts: its track's and bar's scale-9 ends (14 and 12 pixels),
/// the bar never shorter than its ends, and its arrows (32 pixels).
const TRANS_TRACK_END: f32 = 14.0;
const TRANS_BAR_END: f32 = 12.0;
const TRANS_ARROW: f32 = 32.0;
/// The Safe Frame screen (SafeFrame_GFX): the safe area (safeArea_mc: the glow at
/// 30%, its scale-9 grid keeping 112 pixels at its edges), its arrows (64 pixels,
/// their tips 10 in from its edges; their 'increase' and 'decrease' ease back over
/// 8 frames), and the band behind the directions (shape 13: 900 x 60 around (560,
/// 586), at half).
const SAFE_AREA_EDGE: f32 = 112.0;
const SAFE_ARROW: f32 = 64.0;
const SAFE_ARROW_INSET: f32 = 10.0;
const SAFE_ARROW_SECS: f32 = 8.0 / 30.0;
const SAFE_BAND: Rect = Rect {
    min: Vec2::new(110.0, 556.0),
    max: Vec2::new(1010.0, 616.0),
};

/// The Display menu's screens, each opened over the settings (which hide
/// meanwhile): Brightness and Safe Frame.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum ScreenKind {
    Brightness,
    SafeFrame,
}

/// A Display screen open: which, and when it opened; the brightness as it opened
/// (Back puts it back: closeBrightnessMenu; the slider sets it as it moves); the
/// safe frame as adjusted (Accept keeps it: applySafeFrame; Back leaves the one set
/// before); whether anything has changed (Accept shows); and the safe frame's
/// arrows' last steps (across, then up and down: whether the frame grew, and when).
#[derive(Clone, Copy, PartialEq, Debug)]
struct DisplayScreen {
    kind: ScreenKind,
    at: f32,
    brightness: f32,
    frame: Vec2,
    changed: bool,
    steps: [Option<(bool, f32)>; 2],
}
const SLIDER_X: f32 = 298.0;
const SLIDER_W: f32 = 200.0;
const SLIDER_H: f32 = 32.0;
const ARROW: f32 = 26.0;
/// The slider moves in steps of 5 (of 100).
const VOLUME_STEP: f32 = 0.05;
/// The slider's bar (the Slider class, its offsetLeft 10): 10 pixels plus 190
/// times the value's share of its range, from the track's left, and hidden at the
/// minimum. Its ends, and the track's, keep their 6 pixels as they stretch.
const SLIDER_BAR_MIN: f32 = 10.0;
const SLIDER_END: f32 = 6.0;
/// The stepper values' glow (MenuItemStepperRendererLabel's GlowFilter: #003333,
/// blur 5, strength 2): drawn as copies of the text around it, for the values on
/// the bright bar. (Each copy darkens like 22% in the movie's sRGB blending: 42%
/// in the UI's linear light.)
const STEP_GLOW: Color = Color::srgb(0.0, 0.2, 0.2);
const STEP_GLOW_ALPHA: f32 = 0.42;
const STEP_GLOW_REACH: f32 = 1.5;
/// A row that can't be used (MenuButton's "disabled" state: grey, 80% opaque).
const SET_LABEL_DISABLED: Color = Color::srgba(100.0 / 255.0, 100.0 / 255.0, 100.0 / 255.0, 0.8);
/// A slider not focused (its parts' "up" state) is in its own colours with its
/// arrows faded out 6 pixels in; focused ("over"), dark with its arrows (10
/// frames each way).
const SLIDER_ARROW_SLIDE: f32 = 6.0;
/// The Keyboard/Mouse Layout screen (mc_keyboardMouse): its menu's items are 220
/// pixels higher (mc_menu's initMenu for KeyboardMouseLayoutMenuProvider), and its
/// table (mc_keyboardMouseLayout) takes the third item's place, moved out to x -50
/// on the stage (LayoutOffsetX: 50 left of the menus' anchor's parent). Its rows
/// (mc_keyboardMouseLayoutRow) are 20 pixels apart: the action ($HeaderFont 18 px,
/// #00ffd3) right-aligned in a field 600 wide at x 2, its key ($HeaderFont 18 px,
/// #faa63d) in one at x 622.
const KEYBOARD_LAYOUT_RAISE: f32 = 220.0;
const TABLE_ROW_H: f32 = 20.0;
const TABLE_X: f32 = -50.0;
const TABLE_ACTION_RIGHT: f32 = TABLE_X + 2.0 + 598.0 - 2.0;
const TABLE_KEY_X: f32 = TABLE_X + 622.0;
const TABLE_TEXT_SIZE: f32 = 18.0;
const TABLE_ACTION: Color = Color::srgb(0.0, 1.0, 211.0 / 255.0);
const TABLE_KEY: Color = Color::srgb(250.0 / 255.0, 166.0 / 255.0, 61.0 / 255.0);
/// (The demo's own: the game's table only shows a layout.) A table row takes the
/// focus to change its key: the selection bar is shorter under it, and the mouse
/// picks it between these x.
const TABLE_BAR_SCALE: f32 = 0.68;
const TABLE_HIT: (f32, f32) = (200.0, 920.0);
/// The Controller Layout screen (ControllerLayoutMenuProvider): its four rows and,
/// as its fifth item, the controller map (mc_controllerMap, attaching
/// mc_controllerMapXbox360 at its origin, or with no controller
/// mc_controllerMapNone), 220 pixels higher like the Keyboard/Mouse Layout
/// screen's. Its labels (hms.controls.ControllerLabel) show what each control does
/// in the focused row's mode (updateControllerMap: GetKeyDescription, padlayout.rs);
/// one whose text changes comes in white (WhiteColor) from nearer (its _z -1000),
/// to its own colours where it belongs over 0.5 s (Regular.easeOut), and one with
/// nothing left to say fades out without its red over 0.2 s (FadeBlueColor). The
/// nearness is seen in the stage's perspective (Flash's: 55 degrees across the
/// stage's 1120 pixels, about its middle); the game's player takes the _z in
/// twips, as a recording of it shows (a label comes in 5% larger, not 14 times):
/// 50 pixels. Its text has a shadow (a DropShadowFilter: black, 0 away, blurred 5
/// pixels, strength 1.5), unseen on the dark screen but for the white it turns as
/// the label comes in: drawn as a ring of copies 2 pixels around it.
const PAD_MAP_ITEM: usize = 4;
const PAD_LABEL_IN_SECS: f32 = 0.5;
const PAD_LABEL_OUT_SECS: f32 = 0.2;
const PAD_LABEL_Z: f32 = -50.0;
const PAD_SHADOW_ALPHA: f32 = 0.35;
const STAGE_FOV: f32 = 55.0;
const STAGE_CENTER: Vec2 = Vec2::new(560.0, 360.0);
/// A label's text sits 2 pixels into its field (Flash's gutter); a line of it is
/// about this many ems high.
const FIELD_GUTTER: f32 = 2.0;
/// A line of text's height in ems, as the UI lays it out (Bevy's default line
/// height): a label's text zooms about its middle.
const LINE_EMS: f32 = 1.2;

/// The Controller Layout screen's controller map, as tools/build_menu.py writes it
/// (padmap.txt: SettingsMenu_GFX's mc_controllerMapXbox360 and
/// mc_controllerMapNone), in the movie's pixels from the map's origin: the
/// controller's picture, each label (its control's key in the game's bindings, its
/// art's rect and image, its text field: aligned right or left, from x0 to x1 with
/// its top at y0, its size and colour), and the line said with no controller.
#[derive(Default, Debug, PartialEq)]
struct PadMap {
    picture: Option<(String, Rect)>,
    labels: Vec<PadMapLabel>,
    none: Option<PadMapText>,
}

#[derive(Debug, PartialEq)]
struct PadMapLabel {
    key: String,
    image: String,
    art: Rect,
    right: bool,
    text: PadMapText,
}

#[derive(Debug, PartialEq, Clone, Copy)]
struct PadMapText {
    x0: f32,
    x1: f32,
    y0: f32,
    size: f32,
    color: Color,
}

impl PadMap {
    fn load(path: &std::path::Path) -> PadMap {
        match std::fs::read_to_string(path) {
            Ok(text) => PadMap::parse(&text),
            Err(_) => {
                warn!("No {path:?}: the Controller Layout screen shows no controller");
                PadMap::default()
            }
        }
    }

    fn parse(text: &str) -> PadMap {
        let mut map = PadMap::default();
        let field = |w: &[&str]| -> Option<PadMapText> {
            let n = |i: usize| w.get(i).and_then(|v| v.parse::<f32>().ok());
            let hex = u32::from_str_radix(w.get(4)?, 16).ok()?;
            Some(PadMapText {
                x0: n(0)?,
                x1: n(1)?,
                y0: n(2)?,
                size: n(3)?,
                color: Color::srgb_u8((hex >> 16) as u8, (hex >> 8) as u8, hex as u8),
            })
        };
        let rect = |w: &[&str]| -> Option<Rect> {
            let n: Vec<f32> = w.iter().filter_map(|v| v.parse().ok()).collect();
            (n.len() == 4).then(|| Rect::new(n[0], n[1], n[0] + n[2], n[1] + n[3]))
        };
        for line in text.lines() {
            let w: Vec<&str> = line.split_whitespace().collect();
            match w.as_slice() {
                ["picture", image, rest @ ..] if rest.len() == 4 => {
                    map.picture = rect(rest).map(|r| (image.to_string(), r));
                }
                ["label", key, image, rest @ ..] if rest.len() == 10 => {
                    let (Some(art), Some(text)) = (rect(&rest[..4]), field(&rest[5..])) else {
                        continue;
                    };
                    map.labels.push(PadMapLabel {
                        key: key.to_string(),
                        image: image.to_string(),
                        art,
                        right: rest[4] == "right",
                        text,
                    });
                }
                ["none", rest @ ..] => map.none = field(rest),
                _ => {}
            }
        }
        map
    }
}

/// The Controller Layout screen's labels as they are shown: what they are shown
/// for (the focused row, the schemes chosen and whether a controller is there:
/// none since the screen opened), and each label's text and since when, or the
/// text it is fading out of and since when.
#[derive(Default)]
struct PadLabels {
    shown_for: Option<(PadRow, [usize; 3], bool)>,
    labels: Vec<PadLabel>,
    /// Since when the line said with no controller has shown.
    none_at: f32,
}

#[derive(Default, Clone)]
struct PadLabel {
    text: String,
    at: f32,
    leaving: Option<(String, f32)>,
}

/// What a table row shows while it waits for its key, pulsing (and for Move, which
/// of its keys).
const PRESS_A_KEY: &str = "Press a key";
const PRESS_PULSE_HZ: f32 = 1.5;

/// The overlay the front end lays over the background behind a sub-menu
/// (overlayBG_mc, multiplied in): a vertical teal gradient, as (position 0-1,
/// colour) stops.
const OVERLAY: [(f32, [f32; 3]); 5] = [
    (0.0, [40.0, 153.0, 181.0]),
    (33.0 / 255.0, [20.0, 98.0, 116.0]),
    (126.0 / 255.0, [1.0, 74.0, 92.0]),
    (221.0 / 255.0, [20.0, 98.0, 116.0]),
    (1.0, [40.0, 153.0, 181.0]),
];

/// The background movie runs at 29.97 frames a second.
const MOVIE_FPS: f32 = 30000.0 / 1001.0;
/// A menu choice is acted on half a second after it is made (exitMenuDuration).
const LEAVE_SECS: f32 = 0.5;
/// Fades to and from black around starting the game.
const FADE_SECS: f32 = 1.0;

// Sounds (BL_HUD_INTERFACE, as the movie plays them).
const SND_SCROLL_UP: &str = "BL_HUD_INTERFACE.CUST_SUBMENU_01_SCROLL_UP";
const SND_SCROLL_DOWN: &str = "BL_HUD_INTERFACE.CUST_SUBMENU_01_SCROLL_DOWN";
const SND_PRESS_IN: &str = "BL_HUD_INTERFACE.CUST_SUBMENU_01_PRESS_IN";
const SND_PRESS_OUT: &str = "BL_HUD_INTERFACE.CUST_SUBMENU_01_PRESS_OUT";
const SND_SUBMENU_EXIT: &str = "BL_HUD_INTERFACE.PAUSE_SUBMENU_EXIT";
const SND_SUBMENU_ENTER: &str = "BL_HUD_INTERFACE.PAUSE_SUBMENU_ENTER";
const SND_SLIDER_STEP: &str = "BL_HUD_INTERFACE.BUTTON_CALIBRATE";
/// A Display screen's Accept (applyBrightness, applySafeFrame).
const SND_APPLY: &str = "BL_HUD_INTERFACE.CAMPAIGN_MENU_LEVEL_SET_DIFF";
// What the volume sliders play as they move, at their new volume (the settings
// movie's setSettingsModified): a beep for FX Volume, a line of Optimus Prime's for
// Dialog Volume and a music sting for Music Volume.
const SND_SLIDER_VOLUME: &str = "BL_HUD_INTERFACE.SFX_SLIDER_VOLUME";
const SND_SLIDER_DIALOG: &str = "BL_HUD_INTERFACE.DX_SLIDER_VOLUME";
const SND_SLIDER_MUSIC: &str = "BL_HUD_INTERFACE.MX_SLIDER_VOLUME";
const MUSIC: &str = "BL_LVL_HUD_INTERFACE.MP_FRONT_END";
// The pause menu's (PauseMenu_GFX's sounds, and the game's on leaving it).
const SND_PAUSE_OPEN: &str = "BL_HUD_INTERFACE.PAUSE_MENU_OPEN_MP";
const SND_PAUSE_UP: &str = "BL_HUD_INTERFACE.PAUSE_MENU_SCROLL_UP";
const SND_PAUSE_DOWN: &str = "BL_HUD_INTERFACE.PAUSE_MENU_SCROLL_DOWN";
const SND_REENTER: &str = "BL_HUD_INTERFACE.BUTTON_REENTER_GAMEPLAY";
// The Extras menu's buttons (HmButton: rolling over one, and pressing it).
const SND_BUTTON_UP: &str = "BL_HUD_INTERFACE.BUTTON_UP";
const SND_BUTTON_ACCEPT: &str = "BL_HUD_INTERFACE.BUTTON_ACCEPT";

// ---------------------------------------------------------------------------
// The Extras menu (UI_GFxExtras_p.ExtrasMenu_GFX)
// ---------------------------------------------------------------------------

/// Its root menu (mc_rootMenu) sits at (140, 570). Its buttons (mc_panelButton,
/// 350 x 32) are 27 pixels apart from 120 above that; their labels ($HeaderFont,
/// 20 px) sit 10 pixels in, sliding to 0 and darkening from #00ffd3 to #034a3d as
/// the selection bar comes under them (0.2 s).
const EXTRAS_MENU: Vec2 = Vec2::new(140.0, 570.0);
const EXTRAS_ROW_Y: f32 = -120.0;
const EXTRAS_ROW_H: f32 = 27.0;
const EXTRAS_ROW_W: f32 = 350.0;
const EXTRAS_PANEL_H: f32 = 32.0;
/// The label's text field (28.65 pixels tall) is centred on the panel, its text 2
/// pixels inside it (Flash's gutter).
const EXTRAS_LABEL_Y: f32 = EXTRAS_PANEL_H * 0.5 - 0.5 - 28.65 * 0.5 + 2.0;
const EXTRAS_LABEL_X: f32 = 10.0;
const EXTRAS_LABEL_SIZE: f32 = 20.0;
const EXTRAS_LABEL_UP: Color = Color::srgb(0.0, 1.0, 211.0 / 255.0);
const EXTRAS_LABEL_OVER: Color = Color::srgb(3.0 / 255.0, 74.0 / 255.0, 61.0 / 255.0);
const EXTRAS_FOCUS_SECS: f32 = 0.2;
/// The title ("EXTRAS", $HeaderFont 42 px, white) slides in from 90 pixels left
/// as it fades in (title_mc, 15 frames); under it the focused button's hint
/// ($NormalFont 16 px, #ff9900). title_mc ends at (-10, -214.4); in it the title's
/// text starts at (2, -2) and the hint's at (2, 44) (their fields' gutters are
/// inside their bounds).
const EXTRAS_TITLE: Vec2 = Vec2::new(-10.0 + 2.0, -214.4 - 2.0);
const EXTRAS_TITLE_SLIDE: f32 = 90.0;
const EXTRAS_TITLE_SECS: f32 = 15.0 / 30.0;
const EXTRAS_TITLE_SIZE: f32 = 42.0;
const EXTRAS_HINT_Y: f32 = 46.0;
const EXTRAS_HINT_SIZE: f32 = 16.0;
const EXTRAS_HINT: Color = Color::srgb(1.0, 153.0 / 255.0, 0.0);
/// The selection bar (ExtrasMenu_GFX_I2F, 256 x 32, a 9-slice: 10 pixels left,
/// 6 right, 13 top and bottom) 15 pixels left of the buttons at 325.7% x 109.4%,
/// its middle 16 pixels below the focused button's top. On each new focus it
/// starts again from a quarter of its width at half opacity (0.2 s).
const EXTRAS_BAR_X: f32 = -15.0;
const EXTRAS_BAR_SCALE: Vec2 = Vec2::new(3.2575, 1.0938);
const EXTRAS_BAR_SECS: f32 = 0.2;
/// The Extras movie's own background (background_mc): the front end's teal
/// gradient multiplied in once more over its overlay, fading in over a second.
const EXTRAS_BACKGROUND_SECS: f32 = 1.0;
/// Its button hints fade in over 20 frames.
const EXTRAS_FOOTER_SECS: f32 = 20.0 / 30.0;

/// The Extras menu's buttons. The game's root menu has Movies, Audio Logs and
/// Credits; the demo has the credits.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum ExtrasItem {
    Credits,
}

const EXTRAS_ITEMS: [ExtrasItem; 1] = [ExtrasItem::Credits];

impl ExtrasItem {
    /// Its label and rollover hint (UIText.int keys and English fallbacks).
    fn text(self) -> (&'static str, &'static str, &'static str, &'static str) {
        match self {
            ExtrasItem::Credits => (
                "Extras.CreditsButton",
                "Credits",
                "Extras.CreditsRollover",
                "Watch the ending credits for TRANSFORMERS: Fall of Cybertron.",
            ),
        }
    }
}

/// The top of Extras button `i`.
fn extras_row_top(i: usize) -> f32 {
    EXTRAS_MENU.y + EXTRAS_ROW_Y + i as f32 * EXTRAS_ROW_H
}

/// The ending credits (StartExtrasMovie("TFOC_Ending_Credits")): the Bink movie
/// re-encoded as credits.h264 (29.97 frames a second, as the menu movie) and
/// credits.ogg, shown over everything, fitted to the window.
const CREDITS_FPS: f32 = MOVIE_FPS;
/// The menu music fades out under the credits and back in after them.
const CREDITS_MUSIC_OUT_SECS: f32 = 0.5;
const CREDITS_MUSIC_IN_SECS: f32 = 1.0;
/// The menu music plays on under the Customize screen at half its volume, dipping
/// as the menu fades to black into it and coming back up as the menu returns.
const CUSTOMIZE_MUSIC_LEVEL: f32 = 0.5;
/// A press this soon after the credits start doesn't skip them.
const CREDITS_SKIP_GRACE: f32 = 0.5;

// ---------------------------------------------------------------------------
// The Escalation lobby (UI_GFxLobbies_p.MPLobby_GFX)
// ---------------------------------------------------------------------------

/// clouds_bg: tiles of a 512 x 512 cloud image, 511 pixels apart from (-207, -150)
/// (three across and two down in the movie; more here, for wider windows). Each
/// fades into the image turned a quarter turn further every 58.5 frames, round
/// and round.
const CLOUDS_ORIGIN: Vec2 = Vec2::new(-207.0, -150.0);
const CLOUDS_STEP: f32 = 511.0;
const CLOUDS_SIZE: f32 = 512.0;
const CLOUD_COLUMNS: [i32; 6] = [-2, -1, 0, 1, 2, 3];
const CLOUD_ROWS: [i32; 4] = [-1, 0, 1, 2];
const CLOUDS_TURN_SECS: f32 = 58.5 / 30.0;
/// Over the clouds (under the panels) the movie darkens the stage to black at its
/// bottom: a gradient from (5, 93, 102) at no opacity at its top to black at full
/// opacity at its bottom (and black below it).
const CLOUDS_SHADE_TOP: [f32; 3] = [5.0, 93.0, 102.0];
/// The cloud image's average colour (the shade is made to darken it as the movie
/// does).
const CLOUDS_MEAN: [f32; 3] = [0.0, 88.0, 99.0];
/// The lobby's clouds and panels fade in over the main menu as it opens (and out
/// as it closes).
const LOBBY_FADE_SECS: f32 = 0.3;
/// The panels (MPLobby_GFX_I38: 1024 x 1024, drawn 863.2 x 720 from (162, -2)).
const LOBBY_PANELS: Rect = Rect {
    min: Vec2::new(162.0, -2.0),
    max: Vec2::new(162.0 + 863.2, -2.0 + 720.0),
};
/// The title ($NormalFont 36 px, white) and under it the lobby's subtitle (16 px,
/// #00ffd3): each slides 30 pixels right as it fades in (6 frames), 12 and 14
/// frames after the lobby's view opens. (Their text sits a little above the
/// fields' tops: placed as the game shows them.)
const LOBBY_TITLE: Vec2 = Vec2::new(182.0, 63.6);
const LOBBY_TITLE_SIZE: f32 = 36.0;
const LOBBY_TITLE_GLOW: Color = Color::srgba(1.0, 1.0, 1.0, 0.12);
const LOBBY_SUBTITLE: Vec2 = Vec2::new(182.0, 104.4);
const LOBBY_SUBTITLE_SIZE: f32 = 16.0;
const LOBBY_TEAL: Color = Color::srgb(0.0, 1.0, 211.0 / 255.0);
const LOBBY_TITLE_SLIDE: f32 = 30.0;
const LOBBY_TITLE_AT: [f32; 2] = [12.0 / 30.0, 14.0 / 30.0];
const LOBBY_TITLE_SECS: f32 = 6.0 / 30.0;
/// The view's contents (its map, menu, player list...) come in 11 frames after it
/// opens.
const LOBBY_CONTENT_AT: f32 = 11.0 / 30.0;
const LOBBY_CONTENT_SECS: f32 = 0.3;
/// The ticker under the title (mc_ticker, from frame 21): messages (14 px
/// $NormalFont, white, upper case) scroll in from the right of its 825 pixels at
/// 60 pixels a second, each 200 pixels behind the one before.
const TICKER: Rect = Rect {
    min: Vec2::new(183.0, 126.0),
    max: Vec2::new(183.0 + 825.0, 126.0 + 20.0),
};
const TICKER_AT: f32 = 21.0 / 30.0;
/// A message's text, from the window's top.
const TICKER_TEXT_Y: f32 = 0.4;
const TICKER_SIZE: f32 = 14.0;
const TICKER_SPEED: f32 = 60.0;
const TICKER_GAP: f32 = 200.0;
/// A game's map (mc_mapDisplay): its picture (400 x 162) under a shade at 70%, in a
/// frame (mapFrame, 463 x 227) that opens out, wide over 4 frames and then tall
/// over 4, as the view comes in; the map's name ($TitleFont 26 px, #8dffff, with a
/// dark glow) slides in from 20 pixels left.
const MAP_PICTURE: Rect = Rect {
    min: Vec2::new(162.0, 187.0),
    max: Vec2::new(562.0, 349.0),
};
const MAP_FRAME: Rect = Rect {
    min: Vec2::new(131.0, 155.0),
    max: Vec2::new(594.0, 382.0),
};
const MAP_FRAME_START: Vec2 = Vec2::new(0.102, 0.122);
const MAP_LABEL: Vec2 = Vec2::new(183.0, 303.0);
const MAP_LABEL_SIZE: f32 = 26.0;
const MAP_LABEL_COLOR: Color = Color::srgb(141.0 / 255.0, 1.0, 1.0);
/// The menus (mc_partyMainMenu at (181, 195); mc_gameMainMenu's first row at
/// (181, 372)), of the shared panel buttons (392 x 20) and selectors (360 x 20):
/// rows 25 pixels apart.
const LOBBY_MENU_X: f32 = 181.0;
const PARTY_MENU_Y: f32 = 195.0;
const GAME_MENU_Y: f32 = 372.0;
const LOBBY_ROW_H: f32 = 25.0;
const LOBBY_ROW_W: f32 = 392.0;
const LOBBY_SELECTOR_W: f32 = 360.0;
const LOBBY_PANEL_H: f32 = 20.0;
/// The rows' labels ($HeaderFont 20 px, their text fields centred on the rows, the
/// text 2 pixels inside its field: Flash's gutter): a
/// button's 10 pixels in, #00ffd3, sliding to 0 and darkening to #034a3d as it
/// takes the focus (0.2 s); a selector's (as the game shows it, in line with the
/// buttons') darkening to #0f3d4d, with its value (#ff9333) half the selector's
/// width less 10 after its label's 12. A row that can't be used shows at half
/// opacity (a selector, in #50b5d5 at a quarter).
const LOBBY_LABEL_SIZE: f32 = 20.0;
const LOBBY_LABEL_Y: f32 = LOBBY_PANEL_H * 0.5 - 0.5 - 28.65 * 0.5 + 2.0;
const LOBBY_LABEL_X: f32 = 10.0;
const LOBBY_VALUE_X: f32 = 12.0 + LOBBY_SELECTOR_W * 0.5 - 10.0;
const LOBBY_LABEL_OVER: Color = Color::srgb(3.0 / 255.0, 74.0 / 255.0, 61.0 / 255.0);
const LOBBY_SELECTOR_OVER: Color = Color::srgb(15.0 / 255.0, 61.0 / 255.0, 77.0 / 255.0);
const LOBBY_VALUE: Color = Color::srgb(1.0, 147.0 / 255.0, 51.0 / 255.0);
const LOBBY_BLUE: Color = Color::srgb(80.0 / 255.0, 181.0 / 255.0, 213.0 / 255.0);
/// A locked row's label, focused (mc_lockPanelButton's #0f3d4d), and its lock:
/// mc_lock's 24 pixels at 83%, its middle 22 pixels in from the panel's right end
/// (350 pixels across).
const LOBBY_LOCKED_OVER: Color = Color::srgb(15.0 / 255.0, 61.0 / 255.0, 77.0 / 255.0);
const LOBBY_LOCK_X: f32 = 350.0 - 22.0;
const LOBBY_LOCK_SIZE: f32 = 24.0 * 0.83;
const LOBBY_FOCUS_SECS: f32 = 0.2;
/// A selector's arrows (13.4 x 21, scaled to the row's height): the left one ending
/// 0.9 in, the right one starting 0.9 short of the selector's end. #50b5d5 at half
/// opacity; focused, white where its value can move and #50b5d5 at a quarter
/// where it can't. (lobby_arrow.png covers each with a pixel to spare all round.)
const LOBBY_ARROW_INSET: f32 = 0.9;
const LOBBY_ARROW: Vec2 = Vec2::new(13.4, 21.0);
/// The selection bar (MPLobby_GFX_I19, or I1B on a selector: 256 x 32, a 9-slice)
/// 28 pixels left of the rows at 164.1% x 106.2%, its middle 11 pixels below the
/// focused row's top; it moves in 0.2 s.
const LOBBY_BAR_X: f32 = -28.0;
const LOBBY_BAR_SIZE: Vec2 = Vec2::new(256.0 * 1.641, 32.0 * 1.062);
const LOBBY_BAR_Y: f32 = 11.0;
const LOBBY_BAR_SECS: f32 = 0.2;
/// The focused row's hint under the menu ($NormalFont 16 px, #ff9332, wrapping at
/// 380 pixels): it fades in over frames 10 to 20, sliding 10 pixels left and 4
/// down.
const LOBBY_HINT: Vec2 = Vec2::new(174.0, 534.4);
const LOBBY_HINT_SIZE: f32 = 16.0;
const LOBBY_HINT_WIDTH: f32 = 380.0;
const LOBBY_ORANGE: Color = Color::srgb(1.0, 147.0 / 255.0, 50.0 / 255.0);
/// The Back button under it (the PC version's button hint: a 132 x 36 box).
const LOBBY_BACK: Rect = Rect {
    min: Vec2::new(162.0, 597.0),
    max: Vec2::new(294.0, 633.0),
};
/// The player list (PlayerList_GFX at (587, 163)): the team's name ($NormalFont
/// 28 px, #ffc1c1, with a red glow; in the party lobby "PARTY MEMBERS" in
/// #b7fff2), the columns' names (12 px, #999999), then a 16 px row every 20
/// pixels: the name (this player's in white, the others' in the team's colour),
/// the score and the connection's bars (PlayerList_GFX_I51 / 53 / 55, 32 x 32:
/// green under 80 ms, yellow under 160, red beyond).
const LIST_HEADER: Vec2 = Vec2::new(589.0, 163.4);
const LIST_HEADER_SIZE: f32 = 28.0;
const LIST_COLUMNS_Y: f32 = 197.0;
const LIST_COLUMN_SIZE: f32 = 12.0;
const LIST_NAME_COLUMN: f32 = 639.0;
const LIST_SCORE_COLUMN: f32 = 899.0;
const LIST_ROWS_Y: f32 = 217.0;
const LIST_ROW_H: f32 = 20.0;
const LIST_ENTRY_SIZE: f32 = 16.0;
/// A row's text, from its top.
const LIST_ENTRY_Y: f32 = -2.4;
const LIST_NAME_X: f32 = 640.0;
const LIST_SCORE_X: f32 = 902.0;
const LIST_PING_X: f32 = 938.0;
const LIST_TEAM: Color = Color::srgb(1.0, 193.0 / 255.0, 193.0 / 255.0);
const LIST_TEAM_GLOW: Color = Color::srgba(0.94, 0.24, 0.24, 0.3);
/// On a Decepticon map (MapInfo::decepticons), the Decepticons' instead
/// (mc_teamHeaderCell's and mc_playerEntryCell's "decepticon" frames): the text
/// #dfdfff, and a wider, stronger glow in #6633cc (blur 10, strength 1.6, against
/// the Autobots' 8 and 1.1), drawn as a second ring of copies further out.
const LIST_DECEPTICON: Color = Color::srgb(223.0 / 255.0, 223.0 / 255.0, 1.0);
const LIST_DECEPTICON_GLOW: [(f32, Color); 2] = [
    (2.0, Color::srgba(0.4, 0.2, 0.8, 0.35)),
    (4.5, Color::srgba(0.4, 0.2, 0.8, 0.25)),
];
const LIST_PARTY: Color = Color::srgb(183.0 / 255.0, 1.0, 242.0 / 255.0);
/// A Prime's medal (PlayerList_GFX's mc_awardPrimeMode in its awards clip, at 50%
/// and 80%): its star's and ring's sizes (32 and 33 pixels), and its middle from
/// the name's start and the row's top.
const MEDAL_STAR: f32 = 32.0 * 0.4;
const MEDAL_RING: f32 = 33.0 * 0.4;
const MEDAL_X: f32 = -11.0;
const MEDAL_Y: f32 = 8.5;
const LIST_COLUMN: Color = Color::srgb(0.6, 0.6, 0.6);
/// The game mode's icon behind the list (mc_gameModeIconSV, 63.3 pixels across at
/// 480%) in faint teal: it turns in from -60 degrees as it fades in (0.5 s).
/// (lobby_mode_icon.png covers it with a pixel to spare all round.)
const MODE_ICON_CENTER: Vec2 = Vec2::new(868.0, 402.0);
const MODE_ICON_HALF: f32 = 32.65 * 4.8;
const MODE_ICON_TINT: Color = Color::srgba(0.309, 0.711, 0.840, 0.148);
/// The lobby's status (lobbyStatus_txt: 16 px, #ff9332, right-aligned to 1021.6),
/// pulsing between 60% and full opacity (14 frames up, 11 down).
const LOBBY_STATUS: Vec2 = Vec2::new(1021.6, 597.5);
const LOBBY_STATUS_SIZE: f32 = 16.0;
/// The countdown to the match (countdown_mc at (1000, 70)): "MATCH STARTS IN"
/// (14 px, #00ffd3) and the seconds (#ffffcc), right-aligned to 1001. Each second
/// its number shrinks from twice its size (0.5 s) and MULTI_MATCH_COUNTDOWN plays;
/// it rises 10 pixels as it fades in.
const COUNTDOWN_ORIGIN: Vec2 = Vec2::new(1000.0, 85.9);
const COUNTDOWN_SIZE: f32 = 14.0;
const COUNTDOWN_NUMBER: Color = Color::srgb(1.0, 1.0, 0.8);
/// Connecting to Host (mc_connectingToHost): a band of light 1280 x 40 across the
/// middle of the screen (white fading out to its ends, tinted (204, 255, 255) at
/// 30%), "CONNECTING TO HOST" ($HeaderFont 20 px, #cafff5) on it, and by it the
/// lobby's spinning connection icon.
const CONNECTING_BAND: Vec2 = Vec2::new(1280.0, 40.0);
const CONNECTING_TOP: f32 = 347.25;
const CONNECTING_SIZE: f32 = 20.0;
const CONNECTING_COLOR: Color = Color::srgb(202.0 / 255.0, 1.0, 245.0 / 255.0);

/// The prompts (UI_GFxShared_p's TextPrompt_GFX, for Join Game's address, and
/// MessagePrompt_GFX): the screen darkened, a dark box 560 pixels wide across the
/// middle between two bars (#50b5d5, 10 pixels tall) that open out from the centre
/// as the box grows to its height (0.2 s), then the icon on the left, the title,
/// the text and the buttons (the PC version's: 132 x 36 boxes 136 pixels apart,
/// right-aligned to 813) fading in (0.5 s).
const PROMPT_CENTER: Vec2 = Vec2::new(560.0, 360.0);
const PROMPT_W: f32 = 560.0;
const PROMPT_OPEN_SECS: f32 = 0.2;
const PROMPT_CONTENT_SECS: f32 = 0.5;
const PROMPT_ICON_X: f32 = 330.0;
const PROMPT_TEXT_X: f32 = 377.0;
const PROMPT_TEXT_WIDTH: f32 = 434.0;
const PROMPT_TITLE_SIZE: f32 = 32.0;
const PROMPT_BUTTONS_RIGHT: f32 = 813.15;
const PROMPT_BUTTON: Vec2 = Vec2::new(132.0, 36.0);
const PROMPT_BUTTON_STEP: f32 = 136.0;
/// A prompt's buttons as the PC version draws them (its DefaultButton's states, as
/// they measure on its screen): a box 28 pixels tall at the top of the button,
/// dark (BACK_FILL) with its label in cyan (BACK_TEXT), 16 px; the one with the
/// focus orange (its sRGB, top to bottom, its red reaching 255 a third of the way
/// down), its label dark, and under it, past a 2.4 pixel gap, a white bar 3.3
/// pixels tall across its width.
const PROMPT_BUTTON_FILL_H: f32 = 28.0;
const PROMPT_BUTTON_ON: [[f32; 3]; 2] = [[210.0, 121.0, 44.0], [360.0, 201.0, 71.0]];
const PROMPT_LABEL_ON: Color = Color::srgb(36.0 / 255.0, 52.0 / 255.0, 38.0 / 255.0);
const PROMPT_LABEL_SIZE: f32 = 16.0;
const PROMPT_BAR: (f32, f32) = (30.2, 33.5);
/// Join Game's title ($TitleFont 32 px, #8bf0ff) and text (16 px, #50b5d5), and its
/// text input (CLIK's TextInput, 400 x 21: a white box in a 1 pixel border, teal
/// while it has the focus, with 16 px text in (77, 77, 77) 6 pixels in).
const JOIN_TITLE: Color = Color::srgb(139.0 / 255.0, 240.0 / 255.0, 1.0);
const JOIN_TEXT_SIZE: f32 = 16.0;
const JOIN_INPUT_X: f32 = 375.05;
const JOIN_INPUT: Vec2 = Vec2::new(400.0, 21.0);
const JOIN_INPUT_BORDER: Color = Color::srgb(121.0 / 255.0, 181.0 / 255.0, 181.0 / 255.0);
const JOIN_INPUT_TEXT: Color = Color::srgb(77.0 / 255.0, 77.0 / 255.0, 77.0 / 255.0);
/// The longest address that can be typed.
const ADDRESS_MAX: usize = 64;
/// A message's text (18 px, #ff9332).
const MESSAGE_TEXT_SIZE: f32 = 18.0;
/// The prompts' icons: Join Game's (cyan) and a message's warning (orange). Their
/// images cover the movies' shapes with a pixel to spare all round.
const PROMPT_ICON_TINT: Color = Color::srgb(109.0 / 255.0, 210.0 / 255.0, 234.0 / 255.0);
const PROMPT_ICON_HALF: Vec2 = Vec2::new(22.95, 27.475);
const PROMPT_ALERT_TINT: Color = Color::srgb(1.0, 186.0 / 255.0, 51.0 / 255.0);
const PROMPT_ALERT_HALF: Vec2 = Vec2::new(34.375, 29.9);

// The lobby's sounds (BL_HUD_INTERFACE, as its menus play them).
const SND_BUTTON_DOWN: &str = "BL_HUD_INTERFACE.BUTTON_DOWN";
const SND_BUTTON_BACK: &str = "BL_HUD_INTERFACE.BUTTON_BACK";
const SND_BUTTON_START: &str = "BL_HUD_INTERFACE.BUTTON_START";
const SND_SELECTOR: &str = "BL_HUD_INTERFACE.BUTTON_SLIDER_INC_DECREASE";
const SND_LOCKED: &str = "BL_HUD_INTERFACE.BUTTON_LOCKED_ITEM";
const SND_COUNTDOWN: &str = "BL_HUD_INTERFACE.MULTI_MATCH_COUNTDOWN";

/// What the lobby shows (MPLobby_GFX's mc_partyLobby, mc_connectingToHost and
/// mc_gameLobby).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum LobbyView {
    /// The party lobby: Join Game, Host a Game...
    Party,
    /// Joining: waiting for the host to answer.
    Connecting,
    /// A game's lobby, hosting it or joined to it.
    Game,
}

/// The lobby menus' rows: the game's, less matchmaking, parties and the friends
/// list, with Join Game in Find Game's place and Display Name in Leaderboards';
/// the multiplayer lobby's Prime Mode (its Teletraan 1 menu's) in the party lobby;
/// and the demo's own Offline Game above Join Game.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum LobbyRow {
    /// (Not one of the game's: a game of this PC's own, solo or split screen, with
    /// no network and no password to give: Net::host_offline.)
    OfflineGame,
    JoinGame,
    HostGame,
    StartGame,
    SelectMap,
    DisplayName,
    /// The Customize screen, as the main menu opens it (the game's lobby has it
    /// too: Lobby.Customize), closing back to the lobby.
    Customize,
    Difficulty,
    Settings,
    /// Prime Mode, open (every class at the top level): its question.
    PrimeMode,
    /// Prime Mode, locked (mc_lockPanelButton's lockVal): its lock and hint.
    PrimeModeLocked,
}

/// What the party lobby offers of Prime Mode (mc_menuTeletran1's primeMode_mc):
/// open once every class is at the top level, locked till then, and gone once it
/// is on (updatePrimeModeActive).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub(crate) enum PrimeOffer {
    #[default]
    Locked,
    Open,
    Gone,
}

/// Whether a row can be used: it can; it shows its value but can't change it (the
/// host's difficulty, as those who join see it, or as the match counts down); it
/// can't (not part of this demo, or not now); or it is locked (focused, it says
/// what it needs, but does nothing: Prime Mode's lock).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum RowState {
    Enabled,
    DisplayOnly,
    Disabled,
    Locked,
}

const PARTY_ROWS: [LobbyRow; 5] = [
    LobbyRow::OfflineGame,
    LobbyRow::JoinGame,
    LobbyRow::HostGame,
    LobbyRow::DisplayName,
    LobbyRow::Settings,
];
/// With Prime Mode to offer, after Host a Game.
const PARTY_ROWS_PRIME: [LobbyRow; 6] = [
    LobbyRow::OfflineGame,
    LobbyRow::JoinGame,
    LobbyRow::HostGame,
    LobbyRow::PrimeMode,
    LobbyRow::DisplayName,
    LobbyRow::Settings,
];
const PARTY_ROWS_PRIME_LOCKED: [LobbyRow; 6] = [
    LobbyRow::OfflineGame,
    LobbyRow::JoinGame,
    LobbyRow::HostGame,
    LobbyRow::PrimeModeLocked,
    LobbyRow::DisplayName,
    LobbyRow::Settings,
];
const HOST_ROWS: [LobbyRow; 6] = [
    LobbyRow::StartGame,
    LobbyRow::SelectMap,
    LobbyRow::DisplayName,
    LobbyRow::Customize,
    LobbyRow::Difficulty,
    LobbyRow::Settings,
];
/// Those who join see the host's menu without Start Game and Select Map (the rest
/// moves up into their place).
const CLIENT_ROWS: [LobbyRow; 4] = [
    LobbyRow::DisplayName,
    LobbyRow::Customize,
    LobbyRow::Difficulty,
    LobbyRow::Settings,
];
const LOBBY_MAX_ROWS: usize = 6;

impl LobbyRow {
    /// Its label and hint (UIText.int keys and English fallbacks).
    fn text(self) -> (&'static str, &'static str, &'static str, &'static str) {
        match self {
            LobbyRow::OfflineGame => (
                "",
                "Offline Game",
                "",
                "Play alone, or split screen with a second controller, without going online.",
            ),
            // (The game finds games to join; here the host's address is typed in.)
            LobbyRow::JoinGame => (
                "OnlineFriends.JoinGame",
                "Join Game",
                "",
                "Join a friend's game with the host's IP address.",
            ),
            LobbyRow::HostGame => (
                "Lobby.HostGame",
                "Host a Game",
                "Lobby.HostDesc",
                "Set up a custom game for others to join.",
            ),
            LobbyRow::StartGame => (
                "Lobby.StartGame",
                "Start Game",
                "Lobby.StartGameDesc",
                "Start the game.",
            ),
            LobbyRow::SelectMap => (
                "Lobby.SelectMap",
                "Select Map",
                "Lobby.SelectMapDesc",
                "Select the game map.",
            ),
            // (Not one of the game's: the name others see, as typed in.)
            LobbyRow::DisplayName => ("", "Display Name", "", "Change the name other players see."),
            LobbyRow::Customize => (
                "Lobby.Customize",
                "Customize",
                "Lobby.CustomizeDesc",
                "Create customized character builds for use in multiplayer matches.",
            ),
            LobbyRow::Difficulty => (
                "Difficulty.Difficulty",
                "Difficulty Setting",
                "Lobby.ChooseDifficultyDesc",
                "Select a difficulty setting for the current game.",
            ),
            LobbyRow::Settings => (
                "Shared.Settings",
                "Settings",
                "MainMenu.SettingsDesc",
                "Adjust Graphics, Audio, and Controls.",
            ),
            LobbyRow::PrimeMode => (
                "Lobby.PrimeMode",
                "Prime Mode",
                "Lobby.PrimeModeUnlockedDesc",
                "Continue your advancement and gain Prime status.",
            ),
            LobbyRow::PrimeModeLocked => (
                "Lobby.PrimeMode",
                "Prime Mode",
                "Lobby.PrimeModeDesc",
                "Achieve max level in all classes to access Prime Mode.",
            ),
        }
    }

    /// A lateral selector (label, then < value >) rather than a button.
    fn selector(self) -> bool {
        matches!(self, LobbyRow::SelectMap | LobbyRow::Difficulty)
    }

    /// Whether it can be used now. Only the host starts the match, picks the map
    /// (among those built: main.rs's built_maps) and sets the difficulty, until the
    /// countdown starts; Customize is anyone's till then.
    fn state(self, net: &Net) -> RowState {
        let waiting = net.countdown().is_none() && !net.in_match();
        let hosting = net.is_host() && waiting;
        match self {
            LobbyRow::Customize if waiting => RowState::Enabled,
            LobbyRow::OfflineGame
            | LobbyRow::JoinGame
            | LobbyRow::HostGame
            | LobbyRow::DisplayName
            | LobbyRow::Settings
            | LobbyRow::PrimeMode => RowState::Enabled,
            LobbyRow::PrimeModeLocked => RowState::Locked,
            LobbyRow::StartGame | LobbyRow::SelectMap | LobbyRow::Difficulty if hosting => {
                RowState::Enabled
            }
            LobbyRow::SelectMap | LobbyRow::Difficulty => RowState::DisplayOnly,
            _ => RowState::Disabled,
        }
    }
}

/// The rows of the menu `view` shows.
fn lobby_rows(view: LobbyView, host: bool, prime: PrimeOffer) -> &'static [LobbyRow] {
    match view {
        LobbyView::Party => match prime {
            PrimeOffer::Locked => &PARTY_ROWS_PRIME_LOCKED,
            PrimeOffer::Open => &PARTY_ROWS_PRIME,
            PrimeOffer::Gone => &PARTY_ROWS,
        },
        LobbyView::Connecting => &[],
        LobbyView::Game if host => &HOST_ROWS,
        LobbyView::Game => &CLIENT_ROWS,
    }
}

/// The top of row `i` of the menu `view` shows.
fn lobby_row_top(view: LobbyView, i: usize) -> f32 {
    let first = if view == LobbyView::Party {
        PARTY_MENU_Y
    } else {
        GAME_MENU_Y
    };
    first + i as f32 * LOBBY_ROW_H
}

/// Select Map's values: the maps' names, as the game gives them.
fn map_name(text: &MenuText, map: usize) -> String {
    let info = &crate::MAPS[map.min(crate::MAPS.len() - 1)];
    text.get(info.name_key, info.name).to_uppercase()
}

/// Where Select Map stands among the maps that have been built, and the last place.
fn map_place(net: &Net) -> (usize, usize) {
    let maps = crate::built_maps();
    (
        maps.iter().position(|&m| m == net.map()).unwrap_or(0),
        maps.len().saturating_sub(1),
    )
}

/// Difficulty Setting's values.
fn difficulty_name(text: &MenuText, difficulty: usize) -> String {
    match difficulty {
        0 => text.get("Difficulty.Easy", "EASY"),
        2 => text.get("Difficulty.Hard", "HARD"),
        _ => text.get("Difficulty.Medium", "NORMAL"),
    }
}

/// What the ticker under the lobby's title says, in every view of the lobby.
const TICKER_MESSAGE: &str = "WELCOME TO FALL OF CYBERTRON MULTIPLAYER.";

/// The title and text a prompt shows when online play fails (the game's own
/// words, where it has them).
fn failure_message(failure: &Failure, text: &MenuText) -> (String, String) {
    let join_failed = || {
        text.get(
            "MessagePrompts.JoinGameInviteFailedTitle",
            "Join Game Failed",
        )
    };
    match failure {
        Failure::Connect => (
            join_failed(),
            text.get("Errors.FailedToConnect", "Failed to Connect."),
        ),
        Failure::Full => (
            join_failed(),
            text.get(
                "MessagePrompts.JoinGameInviteFailedMessage",
                "Could not join the game. The game is full or no longer available.",
            ),
        ),
        Failure::Version => (
            join_failed(),
            text.get(
                "Engine.Errors.ClientOutdated",
                "The match you are trying to join is running an incompatible version of the game.",
            ),
        ),
        Failure::Password { wrong } => (
            join_failed(),
            if *wrong {
                text.get(
                    "Engine.AccessControl.WrongPassword",
                    "The password you entered is incorrect.",
                )
            } else {
                text.get(
                    "Engine.AccessControl.NeedPassword",
                    "You need to enter a password to join this game.",
                )
            },
        ),
        Failure::HostLost => (
            text.get("Engine.Errors.ConnectionFailed_Title", "Connection Error"),
            text.get(
                "Errors.HostQuitError",
                "Your connection with the Host has been lost.",
            ),
        ),
        Failure::Host(reason) => (
            text.get("MessagePrompts.StrictNATTitle", "Unable to Host"),
            format!("Port {DEFAULT_PORT} could not be opened: {reason}"),
        ),
    }
}

/// `s` broken into lines at most `width` pixels wide, between words (as Flash wraps
/// a text field), joined by line breaks.
fn wrap_text(text: &MenuText, font: &str, s: &str, size: f32, width: f32) -> String {
    let mut lines = Vec::new();
    for paragraph in s.split('\n') {
        let mut line = String::new();
        for word in paragraph.split_whitespace() {
            let longer = if line.is_empty() {
                word.to_string()
            } else {
                format!("{line} {word}")
            };
            if !line.is_empty() && text.width(font, &longer, size) > width {
                lines.push(std::mem::replace(&mut line, word.to_string()));
            } else {
                line = longer;
            }
        }
        lines.push(line);
    }
    lines.join("\n")
}

/// A prompt over the menus.
#[derive(Clone, PartialEq, Debug)]
enum Prompt {
    /// Join Game: the host's address, typed in.
    Address,
    /// Display Name: the player's name, typed in.
    DisplayName,
    /// Host a Game: the game's password (Game Password), typed in; left empty,
    /// anyone can join.
    HostPassword,
    /// Joining a game with a password: the password, typed in (asked for again,
    /// `wrong`, when the one given was wrong).
    JoinPassword { wrong: bool },
    /// A message, with an Ok button.
    Message { title: String, text: String },
    /// A question over the pause menu, with Yes and No buttons (TnQuitMessageBox's,
    /// as Quit Game asks): Yes does `action`.
    Confirm {
        title: String,
        text: String,
        action: ConfirmAction,
    },
}

/// What a Confirm prompt's Yes does.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum ConfirmAction {
    /// Quit Game: leave the match for the main menu, losing the experience it earned.
    QuitGame,
    /// Return to Lobby (the host's): end the match, every player back to the lobby.
    ReturnToLobby,
    /// Prime Mode (the party lobby's, with Confirm and Cancel): turn it on.
    PrimeMode,
    /// Exit Game (the main menu's, with Continue and Cancel): quit the demo.
    ExitGame,
}

/// Prime Mode's question (MessagePrompts.PrimeModeMessage), should menu.txt lack it.
const PRIME_MODE_TEXT: &str = "Activating Prime Mode will reset all of your current XP and \
     challenge completions. A medal will also appear next to your name so that all can see \
     your new status as a Prime.";

/// Quit Game's question (in the game's question's form: TnQuitMessageBox's).
const QUIT_GAME_TEXT: &str = "You will be returned to the main menu. Do you want to proceed?";
/// Exit Game's (FrontEnd_GFX's exitGame: MessagePrompts.GenericWarningTitle and
/// ExitGameMessage), should menu.txt lack them.
const EXIT_GAME_TITLE: &str = "Warning";
const EXIT_GAME_TEXT: &str = "Do you want to exit the game?";
/// Return to Lobby's (the game has none: in its words).
const RETURN_TO_LOBBY_TITLE: &str = "Return to Lobby";
const RETURN_TO_LOBBY_TEXT: &str =
    "The match will end and all players will return to the lobby. Do you want to proceed?";
/// What the other players are told as they come back to the lobby.
const MATCH_ENDED_TEXT: &str = "The host ended the match.";

impl Prompt {
    /// A prompt with a line to type in (TextPrompt_GFX's, as Join Game's).
    fn typed(&self) -> bool {
        !matches!(self, Prompt::Message { .. } | Prompt::Confirm { .. })
    }

    /// A prompt for a password (what is typed shows as asterisks, as in a Flash
    /// password field).
    fn password(&self) -> bool {
        matches!(self, Prompt::HostPassword | Prompt::JoinPassword { .. })
    }
}

/// Where a prompt's parts go (stage pixels).
struct PromptLayout {
    /// Its box (between its bars) and its icon's centre.
    box_rect: Rect,
    icon: Vec2,
    /// Where its title's and text's first lines start (their tops), and the text
    /// broken into lines.
    title: Vec2,
    body: Vec2,
    lines: String,
    /// The line typed in (Join Game's and Display Name's).
    input: Option<Rect>,
    /// The buttons, left to right, with their labels.
    buttons: Vec<(Rect, String)>,
}

/// TextPrompt_GFX's and MessagePrompt_GFX's layout (updateMessage): the content
/// stacked from the title down and centred on the screen (Join Game's 10 pixels
/// high), the box 50 pixels taller than it, the icon by the title.
fn prompt_layout(prompt: &Prompt, text: &MenuText) -> PromptLayout {
    let buttons = |labels: Vec<String>, top: f32| -> Vec<(Rect, String)> {
        let n = labels.len();
        labels
            .into_iter()
            .enumerate()
            .map(|(i, label)| {
                let right = PROMPT_BUTTONS_RIGHT - (n - 1 - i) as f32 * PROMPT_BUTTON_STEP;
                let rect = Rect::new(right - PROMPT_BUTTON.x, top, right, top + PROMPT_BUTTON.y);
                (rect, label)
            })
            .collect()
    };
    let box_around = |height: f32| {
        Rect::from_center_half_size(
            PROMPT_CENTER,
            Vec2::new(PROMPT_W * 0.5, (height + 50.0) * 0.5),
        )
    };
    match prompt {
        Prompt::Address
        | Prompt::DisplayName
        | Prompt::HostPassword
        | Prompt::JoinPassword { .. } => {
            let join = || text.get("MessagePrompts.JoinFriendGameJoinButton", "Join");
            let ok = || text.get("ButtonHints.Ok", "Ok");
            let (description, accept) = match prompt {
                Prompt::Address => (
                    format!(
                        "Enter the host's IP address, and its port if it isn't {DEFAULT_PORT}."
                    ),
                    join(),
                ),
                Prompt::HostPassword => (
                    format!(
                        "{} Leave it blank for anyone to join.",
                        text.get(
                            "Engine.AccessControl.GamePasswordDesc",
                            "If this password is set, players will have to enter it to join this server.",
                        )
                    ),
                    ok(),
                ),
                Prompt::JoinPassword { wrong: true } => (
                    text.get(
                        "Engine.AccessControl.WrongPassword",
                        "The password you entered is incorrect.",
                    ),
                    join(),
                ),
                Prompt::JoinPassword { wrong: false } => (
                    text.get(
                        "Engine.AccessControl.NeedPassword",
                        "You need to enter a password to join this game.",
                    ),
                    join(),
                ),
                _ => ("Enter the name the other players will see.".to_string(), ok()),
            };
            let lines = wrap_text(
                text,
                DEMI_BOLD,
                &description,
                JOIN_TEXT_SIZE,
                PROMPT_TEXT_WIDTH,
            );
            // The title at 2, the text at 32, the input at 69, the buttons at 112.2,
            // the input and buttons lower by any lines of text after the first.
            let more = (lines.split('\n').count().max(1) - 1) as f32 * JOIN_TEXT_SIZE * 1.2;
            let height = 112.2 + more + PROMPT_BUTTON.y;
            let top = PROMPT_CENTER.y - height * 0.5 - 10.0;
            let input_y = top + 69.0 + more;
            PromptLayout {
                box_rect: box_around(height),
                icon: Vec2::new(PROMPT_ICON_X, top + 40.0),
                title: Vec2::new(PROMPT_TEXT_X, top + 2.0),
                body: Vec2::new(PROMPT_TEXT_X, top + 32.0),
                lines,
                input: Some(Rect::new(
                    JOIN_INPUT_X,
                    input_y,
                    JOIN_INPUT_X + JOIN_INPUT.x,
                    input_y + JOIN_INPUT.y,
                )),
                buttons: buttons(
                    vec![accept, text.get("ButtonHints.Cancel", "Cancel")],
                    top + 112.2 + more,
                ),
            }
        }
        Prompt::Message { text: message, .. } | Prompt::Confirm { text: message, .. } => {
            // The title (its field 42.4 pixels tall), the text under it, the
            // buttons 10 pixels below that: a message's Ok, a question's Yes and No.
            let labels = if let Prompt::Confirm { action, .. } = prompt {
                match action {
                    ConfirmAction::PrimeMode => vec![
                        text.get("ButtonHints.Confirm", "Confirm"),
                        text.get("ButtonHints.Cancel", "Cancel"),
                    ],
                    ConfirmAction::ExitGame => vec![
                        text.get("MessagePrompts.GenericContinueButton", "Continue"),
                        text.get("MessagePrompts.GenericCancelButton", "Cancel"),
                    ],
                    _ => vec![
                        text.get("ButtonHints.Yes", "Yes"),
                        text.get("ButtonHints.No", "No"),
                    ],
                }
            } else {
                vec![text.get("ButtonHints.Ok", "Ok")]
            };
            let lines = wrap_text(
                text,
                DEMI_BOLD,
                message,
                MESSAGE_TEXT_SIZE,
                PROMPT_TEXT_WIDTH,
            );
            let count = lines.split('\n').count() as f32;
            let body_y = 2.0 + PROMPT_TITLE_SIZE * 1.2 + 4.0;
            let buttons_y = body_y + count * MESSAGE_TEXT_SIZE * 1.2 + 4.0 + 10.0;
            let height = buttons_y + PROMPT_BUTTON.y;
            let top = PROMPT_CENTER.y - height * 0.5;
            PromptLayout {
                box_rect: box_around(height),
                icon: Vec2::new(PROMPT_ICON_X, top + 37.0),
                title: Vec2::new(PROMPT_TEXT_X, top + 2.0),
                body: Vec2::new(PROMPT_TEXT_X + 1.0, top + body_y),
                lines,
                input: None,
                buttons: buttons(labels, top + buttons_y),
            }
        }
    }
}

// ---------------------------------------------------------------------------
// The pause menu (UI_GFxPause_p.PauseMenu_GFX's multiplayer shell)
// ---------------------------------------------------------------------------

/// The shell (shellAnchor_mc) sits at the stage's centre; its menu (menuAnchor_mc)
/// 124 pixels left of that, its Teletraan logo and pips 370 pixels left.
const PAUSE_SHELL: Vec2 = Vec2::new(560.0, 360.0);
const PAUSE_MENU_X: f32 = -124.0;
const PAUSE_LOGO_X: f32 = -370.0;
/// The menu's rows are 24 pixels apart (its mc_menu's ButtonSpacing).
const PAUSE_ROW_H: f32 = 24.0;
/// Its selection bar (mc_selectionBarArt, 256 x 32, a 9-slice: SELECTION_BAR_GRID)
/// at 273% x 106%, 24 pixels left of the menu; its background art
/// (mc_menuBackgroundArt, 256 x 256 from its left edge) 36 pixels left, at 273% ->
/// 313% wide and 281% tall, sliding in from 100 pixels right as it fades to 10%
/// (13 frames).
const PAUSE_BAR_SCALE: Vec2 = Vec2::new(2.734, 1.062);
const PAUSE_ART_SECS: f32 = 13.0 / 30.0;
/// The shell's opening ("shell" in teletranLogo_mc and pips_mc): 14 frames.
const PAUSE_SHELL_SECS: f32 = 14.0 / 30.0;
/// The logo (PauseMenu_GFX_I51, drawn 204 x 51 around logo_mc) slides from 145 to
/// 20 pixels; its caption ($NormalFont 12 px, #00ffd3) under it.
const PAUSE_LOGO_SIZE: Vec2 = Vec2::new(204.0, 51.0);
const PAUSE_CAPTION_SIZE: f32 = 12.0;
const PAUSE_CAPTION: Color = Color::srgb(0.0, 1.0, 211.0 / 255.0);
/// The Teletraan emblem (PauseMenu_GFX_I4D, 110 x 110 from (-55, -46)) slides 40
/// pixels left as it fades out.
const PAUSE_EMBLEM: Rect = Rect {
    min: Vec2::new(-55.0, -46.0),
    max: Vec2::new(55.0, 64.0),
};
/// Behind the logo (24 frames, from the shell's start): the warped hexagons
/// (600 x 600, shrinking from 113% while fading in to 10%) and the emblem's glow
/// (285 x 285 from (-142.5, -123)), both in teal.
const PAUSE_BACK_SECS: f32 = 24.0 / 30.0;
/// The warped hexagons (SharedAssets' mc_warpedHexagonBackgroundArt, its shape
/// 118) are a disc of four quarters round the logo, each the 256-pixel image
/// (WarpedHexagonBackground: a quarter of the disc, brightest at its bottom right)
/// at 117%, so 300 x 300: as it is at the top left, mirrored across at the top
/// right, both ways at the bottom right and up and down at the bottom left, its
/// bright corner always at the middle. By quarter: which way it lies from the
/// middle, and whether it is mirrored across and up and down.
const PAUSE_HEX_QUARTERS: [(Vec2, bool, bool); 4] = [
    (Vec2::new(-1.0, -1.0), false, false),
    (Vec2::new(1.0, -1.0), true, false),
    (Vec2::new(1.0, 1.0), true, true),
    (Vec2::new(-1.0, 1.0), false, true),
];
const PAUSE_GLOW: Rect = Rect {
    min: Vec2::new(-142.5, -123.0),
    max: Vec2::new(142.5, 162.0),
};
/// The glow's keys: (frame, x scale, y scale, opacity, blue).
const PAUSE_GLOW_KEYS: [(f32, f32, f32, f32, f32); 9] = [
    (0.0, 2.105, 0.070, 0.0, 1.0),
    (1.0, 1.691, 0.546, 0.352, 0.926),
    (2.0, 1.395, 0.886, 0.602, 0.871),
    (3.0, 1.217, 1.090, 0.750, 0.840),
    (4.0, 1.158, 1.158, 0.801, 0.828),
    (5.0, 1.142, 1.142, 0.551, 0.914),
    (6.0, 1.126, 1.126, 0.301, 1.0),
    (12.0, 1.056, 1.056, 0.246, 1.0),
    (24.0, 1.0, 1.0, 0.199, 1.0),
];
/// The pips (mc_lineSegmentArt: 16 x 4) in pips_mc: from where each starts to
/// where it ends, and whether it fades in (+1), out (-1) or neither, as they
/// shrink from 400% wide. They end at the corners of two boxes around the logo.
const PAUSE_PIPS: [(Vec2, Vec2, i8); 8] = [
    (Vec2::new(270.0, 260.0), Vec2::new(210.0, 260.0), 1),
    (Vec2::new(70.0, 260.0), Vec2::new(-70.0, 260.0), 1),
    (Vec2::new(270.0, 60.0), Vec2::new(211.0, 60.0), 0),
    (Vec2::new(70.0, 60.0), Vec2::new(-70.0, 60.0), 0),
    (Vec2::new(-70.0, 60.0), Vec2::new(-70.0, 60.0), -1),
    (Vec2::new(270.0, -50.0), Vec2::new(211.0, -50.0), 0),
    (Vec2::new(70.0, -50.0), Vec2::new(-70.0, -50.0), 0),
    (Vec2::new(-70.0, -50.0), Vec2::new(-70.0, -50.0), -1),
];

/// The pause menu's items: the multiplayer menu (MultiplayerMenuProvider) without
/// Switch Teams (there are no teams) and Invite Friends; the host's with Return to
/// Lobby over Quit Game. Choose Character opens the Choose Character screen over it
/// (choose.rs).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum PauseItem {
    Resume,
    ChooseCharacter,
    Settings,
    ReturnToLobby,
    Quit,
}

const PAUSE_ITEMS: [PauseItem; 4] = [
    PauseItem::Resume,
    PauseItem::ChooseCharacter,
    PauseItem::Settings,
    PauseItem::Quit,
];
const HOST_PAUSE_ITEMS: [PauseItem; 5] = [
    PauseItem::Resume,
    PauseItem::ChooseCharacter,
    PauseItem::Settings,
    PauseItem::ReturnToLobby,
    PauseItem::Quit,
];
const PAUSE_MAX_ROWS: usize = HOST_PAUSE_ITEMS.len();

/// The pause menu's items, for the host or another player.
fn pause_items(host: bool) -> &'static [PauseItem] {
    if host {
        &HOST_PAUSE_ITEMS
    } else {
        &PAUSE_ITEMS
    }
}

impl PauseItem {
    fn label(self) -> (&'static str, &'static str) {
        match self {
            PauseItem::Resume => ("Pause.ResumeGame", "Resume Game"),
            PauseItem::ChooseCharacter => ("Pause.ChooseCharacter", "Choose Character"),
            PauseItem::Settings => ("Shared.Settings", "Settings"),
            // (Not one of the game's.)
            PauseItem::ReturnToLobby => ("", RETURN_TO_LOBBY_TITLE),
            PauseItem::Quit => ("Pause.QuitGame", "Quit Game"),
        }
    }
}

/// Where row `i` of a pause menu of `rows` sits, from the menu's anchor.
fn pause_row_offset(i: usize, rows: usize) -> f32 {
    i as f32 * PAUSE_ROW_H - rows as f32 * PAUSE_ROW_H * 0.5
}

/// The pause menu, for the game's systems: it is open (the game keeps running, but
/// Jazz takes no orders), and where Jazz is (the hologram sweep starts there).
/// `choosing` is the same for the Choose Character screen (choose.rs: as the match
/// starts, or from the pause menu). `just_closed` marks the frame either closed
/// (the key or click that closed it isn't the game's), and `reset_match` asks the
/// game to start the match over (on leaving it for the menu). `forfeit_xp`: the
/// player quit the match (Quit Game), so the experience it earned is lost (xp.rs).
///
/// `seat`: whose controller opened it (split screen, split.rs: the guest's Start
/// his; anything else the first player's). Its Choose Character screen is his.
#[derive(Resource, Default)]
pub struct Pause {
    pub open: bool,
    pub choosing: bool,
    pub just_closed: bool,
    pub player_at: Vec3,
    pub reset_match: bool,
    pub forfeit_xp: bool,
    pub seat: usize,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Item {
    Customize,
    Escalation,
    Settings,
    Extras,
    Exit,
}

const ITEMS: [Item; 5] = [
    Item::Customize,
    Item::Escalation,
    Item::Settings,
    Item::Extras,
    Item::Exit,
];

impl Item {
    /// The label and description (UIText.int keys) and English fallbacks.
    fn text(self) -> (&'static str, &'static str, &'static str, &'static str) {
        match self {
            // The multiplayer lobby's button and description.
            Item::Customize => (
                "Lobby.Customize",
                "Customize",
                "Lobby.CustomizeDesc",
                "Create customized character builds for use in multiplayer matches.",
            ),
            Item::Escalation => (
                "MainMenu.Escalation",
                "Escalation",
                "MainMenu.EscalationDesc",
                "Survive waves of enemies with your friends.",
            ),
            Item::Settings => (
                "Shared.Settings",
                "Settings",
                "MainMenu.SettingsDesc",
                "Adjust Graphics, Audio, and Controls.",
            ),
            Item::Extras => (
                "Extras.ExtrasTitle",
                "Extras",
                "MainMenu.ExtrasDesc",
                "View unlocked bonus content.",
            ),
            Item::Exit => (
                "MainMenu.ExitGame",
                "Exit Game",
                "MainMenu.ExitDesc",
                "Exit TRANSFORMERS: Fall of Cybertron.",
            ),
        }
    }
}

/// The settings menus (SettingsMenu_GFX's RootMenuProvider and its sub-menus, as
/// the PC version lists them). Display has Brightness and Safe Frame, each opening
/// its own screen (`DisplayScreen`), and Team Coloring Mode; Graphics has its
/// Fullscreen Mode and the demo's Framerate Cap, Audio the demo's
/// Master Volume over the game's volumes (AudioMenuProvider's sliders; its
/// Subtitles, with nothing to subtitle, is left out), and Controls all of the
/// game's (ControlsMenuProvider, with the PC's Mouse Sensitivity) and its
/// Keyboard/Mouse Layout screen (KeyboardMouseLayoutMenuProvider).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum SetMenu {
    Root,
    Display,
    Graphics,
    Audio,
    Controls,
    KeyboardLayout,
    ControllerLayout,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum SetRow {
    Display,
    Graphics,
    Audio,
    Controls,
    Brightness,
    SafeFrame,
    TeamColorMode,
    /// The demo's own (split.rs).
    SplitScreen,
    /// The Graphics menu's own Graphics row (GraphicPreference).
    Quality,
    Fullscreen,
    Resolution,
    VSync,
    /// The demo's own.
    RenderScale,
    FramerateCap,
    MasterVolume,
    FxVolume,
    DialogVolume,
    MusicVolume,
    ActiveControls,
    KeyboardMouseLayout,
    ControllerLayout,
    Vibration,
    VehicleBoost,
    StickDelay,
    RobotCamera,
    VehicleCamera,
    JetCamera,
    CameraSensitivity,
    MouseSensitivity,
    SelectLayout,
    ViewLayout,
    /// A row of the Keyboard/Mouse Layout screen's table: an action and its key.
    Binding(usize),
    /// A row of the Controller Layout screen: its scheme for the robot, the
    /// vehicle or the thumbsticks.
    PadScheme(PadRow),
}

/// The most rows a settings menu has: the Keyboard/Mouse Layout screen's two
/// steppers and the robot's table.
const MAX_ROWS: usize = 2 + 18;
/// The most rows with labels of their own (the Controls menu's).
const MAX_LABELS: usize = 11;
/// The most sliders a menu has (the Audio menu's four volumes).
const MAX_SLIDERS: usize = 4;

impl SetMenu {
    /// Its rows (the Keyboard/Mouse Layout screen's table showing `view`'s keys).
    fn rows(self, view: Mode) -> Vec<SetRow> {
        match self {
            SetMenu::Root => vec![
                SetRow::Display,
                SetRow::Graphics,
                SetRow::Audio,
                SetRow::Controls,
            ],
            SetMenu::Display => vec![
                SetRow::Brightness,
                SetRow::SafeFrame,
                SetRow::TeamColorMode,
                SetRow::SplitScreen,
            ],
            SetMenu::Graphics => vec![
                SetRow::Quality,
                SetRow::Fullscreen,
                SetRow::Resolution,
                SetRow::VSync,
                SetRow::RenderScale,
                SetRow::FramerateCap,
            ],
            SetMenu::Audio => vec![
                SetRow::MasterVolume,
                SetRow::FxVolume,
                SetRow::DialogVolume,
                SetRow::MusicVolume,
            ],
            SetMenu::Controls => vec![
                SetRow::ActiveControls,
                SetRow::KeyboardMouseLayout,
                SetRow::ControllerLayout,
                SetRow::Vibration,
                SetRow::VehicleBoost,
                SetRow::StickDelay,
                SetRow::RobotCamera,
                SetRow::VehicleCamera,
                SetRow::JetCamera,
                SetRow::CameraSensitivity,
                SetRow::MouseSensitivity,
            ],
            SetMenu::KeyboardLayout => [SetRow::SelectLayout, SetRow::ViewLayout]
                .into_iter()
                .chain((0..view.actions().len()).map(SetRow::Binding))
                .collect(),
            SetMenu::ControllerLayout => PadRow::ALL.into_iter().map(SetRow::PadScheme).collect(),
        }
    }

    /// Its title (a UIText.int key and the English fallback); shown in upper case.
    fn title(self) -> (&'static str, &'static str) {
        match self {
            SetMenu::Root => ("Shared.Settings", "Settings"),
            SetMenu::Display => ("Settings.Display", "Display"),
            SetMenu::Graphics => ("Settings.Graphics", "Graphics"),
            SetMenu::Audio => ("Settings.Audio", "Audio"),
            SetMenu::Controls => ("Settings.Controls", "Controls"),
            SetMenu::KeyboardLayout => ("Settings.KeyboardMouseLayout", "Keyboard/Mouse Layout"),
            SetMenu::ControllerLayout => ("Settings.ControllerLayout", "Controller Layout"),
        }
    }

    /// How deep it is: the root menu, its sub-menus, and Controls' sub-menu.
    fn depth(self) -> usize {
        match self {
            SetMenu::Root => 0,
            SetMenu::Display | SetMenu::Graphics | SetMenu::Audio | SetMenu::Controls => 1,
            SetMenu::KeyboardLayout | SetMenu::ControllerLayout => 2,
        }
    }

    /// The menu it was opened from, and that menu's row which opened it.
    fn parent(self) -> Option<(SetMenu, SetRow)> {
        match self {
            SetMenu::Root => None,
            SetMenu::Display => Some((SetMenu::Root, SetRow::Display)),
            SetMenu::Graphics => Some((SetMenu::Root, SetRow::Graphics)),
            SetMenu::Audio => Some((SetMenu::Root, SetRow::Audio)),
            SetMenu::Controls => Some((SetMenu::Root, SetRow::Controls)),
            SetMenu::KeyboardLayout => Some((SetMenu::Controls, SetRow::KeyboardMouseLayout)),
            SetMenu::ControllerLayout => Some((SetMenu::Controls, SetRow::ControllerLayout)),
        }
    }

    /// Where its row `i` is (its top, from the anchor, before the drift). mc_menu
    /// spaces its items ButtonSpacing (27) apart, centred on the anchor; the
    /// Keyboard/Mouse Layout screen's are 220 higher, and its table's rows
    /// (mc_keyboardMouseLayoutRow) are 20 apart from where its third item would be.
    /// The Controller Layout screen's are 220 higher too, its controller map the
    /// fifth item (`i` 4).
    fn row_y(self, i: usize, rows: &[SetRow]) -> f32 {
        match self {
            SetMenu::KeyboardLayout => {
                row_offset(i.min(2), 3) - KEYBOARD_LAYOUT_RAISE
                    + i.saturating_sub(2) as f32 * TABLE_ROW_H
            }
            SetMenu::ControllerLayout => row_offset(i, PAD_MAP_ITEM + 1) - KEYBOARD_LAYOUT_RAISE,
            _ => row_offset(i, rows.len()),
        }
    }
}

impl SetRow {
    /// Its label (a UIText.int key and the English fallback), as written.
    fn label(self) -> (&'static str, &'static str) {
        match self {
            SetRow::Display => ("Settings.Display", "Display"),
            SetRow::Graphics => ("Settings.Graphics", "Graphics"),
            SetRow::Audio => ("Settings.Audio", "Audio"),
            SetRow::Controls => ("Settings.Controls", "Controls"),
            SetRow::Brightness => ("Settings.Brightness", "Brightness"),
            SetRow::SafeFrame => ("Settings.SafeFrame", "Safe Frame"),
            SetRow::TeamColorMode => ("Settings.TeamColorMode", "Team Coloring Mode"),
            SetRow::SplitScreen => ("", "Split Screen"),
            SetRow::Quality => ("Settings.Graphics", "Graphics"),
            SetRow::Fullscreen => ("Settings.Fullscreen", "Fullscreen Mode"),
            SetRow::Resolution => ("Settings.Resolution", "Resolution"),
            SetRow::VSync => ("Settings.VSync", "VSync"),
            SetRow::RenderScale => ("", "Render Scale"),
            // Not one of the game's settings (it has VSync); the demo's own.
            SetRow::FramerateCap => ("", "Framerate Cap"),
            // The demo's own, over the game's three.
            SetRow::MasterVolume => ("", "Master Volume"),
            SetRow::FxVolume => ("Settings.EffectsVolume", "FX Volume"),
            SetRow::DialogVolume => ("Settings.DialogVolume", "Dialog Volume"),
            SetRow::MusicVolume => ("Settings.MusicVolume", "Music Volume"),
            SetRow::ActiveControls => ("Settings.ActiveControls", "Active Game Controls"),
            SetRow::KeyboardMouseLayout => {
                ("Settings.KeyboardMouseLayout", "Keyboard/Mouse Layout")
            }
            SetRow::ControllerLayout => ("Settings.ControllerLayout", "Controller Layout"),
            SetRow::Vibration => ("Settings.ControllerVibration", "Controller Vibration"),
            SetRow::VehicleBoost => ("Settings.VehicleBoost", "Toggle Vehicle Boost"),
            // (The PC version labels it as the Xbox 360 version does.)
            SetRow::StickDelay => ("Settings.HeavyThumbXBOX", "LS / RS Button Delay"),
            SetRow::RobotCamera => ("Settings.RobotControl", "Robot Camera"),
            SetRow::VehicleCamera => ("Settings.GroundVehicleControl", "Vehicle Camera"),
            SetRow::JetCamera => ("Settings.AirVehicleControl", "Jet Camera"),
            SetRow::CameraSensitivity => ("Settings.CameraSensitivity", "Camera Sensitivity"),
            SetRow::MouseSensitivity => ("Settings.MouseSensitivity", "Mouse Sensitivity"),
            SetRow::SelectLayout => ("Settings.SelectLayout", "Select Layout"),
            SetRow::ViewLayout => ("Settings.ViewLayout", "View Layout"),
            SetRow::Binding(_) => ("", ""),
            SetRow::PadScheme(row) => row.text(),
        }
    }

    /// The stepper the row has, if it is one.
    fn stepper(self) -> Option<Stepper> {
        Some(match self {
            SetRow::TeamColorMode => Stepper::TeamColorMode,
            SetRow::SplitScreen => Stepper::SplitScreen,
            SetRow::Quality => Stepper::Quality,
            SetRow::Fullscreen => Stepper::Fullscreen,
            SetRow::VSync => Stepper::VSync,
            SetRow::FramerateCap => Stepper::FramerateCap,
            SetRow::ActiveControls => Stepper::ActiveControls,
            SetRow::Vibration => Stepper::Vibration,
            SetRow::VehicleBoost => Stepper::VehicleBoost,
            SetRow::StickDelay => Stepper::StickDelay,
            SetRow::RobotCamera => Stepper::RobotCamera,
            SetRow::VehicleCamera => Stepper::VehicleCamera,
            SetRow::JetCamera => Stepper::JetCamera,
            SetRow::SelectLayout => Stepper::SelectLayout,
            SetRow::ViewLayout => Stepper::ViewLayout,
            SetRow::PadScheme(row) => Stepper::PadScheme(row),
            _ => return None,
        })
    }

    /// The slider the row has, if it is one.
    fn slider(self) -> Option<Slider> {
        match self {
            SetRow::Resolution => Some(Slider::Resolution),
            SetRow::RenderScale => Some(Slider::RenderScale),
            SetRow::MasterVolume => Some(Slider::MasterVolume),
            SetRow::FxVolume => Some(Slider::FxVolume),
            SetRow::DialogVolume => Some(Slider::DialogVolume),
            SetRow::MusicVolume => Some(Slider::MusicVolume),
            SetRow::CameraSensitivity => Some(Slider::CameraSensitivity),
            SetRow::MouseSensitivity => Some(Slider::MouseSensitivity),
            _ => None,
        }
    }

    /// The menu the row opens, if it opens one.
    fn opens(self) -> Option<SetMenu> {
        match self {
            SetRow::Display => Some(SetMenu::Display),
            SetRow::Graphics => Some(SetMenu::Graphics),
            SetRow::Audio => Some(SetMenu::Audio),
            SetRow::Controls => Some(SetMenu::Controls),
            SetRow::KeyboardMouseLayout => Some(SetMenu::KeyboardLayout),
            SetRow::ControllerLayout => Some(SetMenu::ControllerLayout),
            _ => None,
        }
    }

    /// Whether it can be used: Controller Layout only while Active Game Controls
    /// is on Controller (mc_menu's prefersGamepad), and its rows only with a
    /// controller connected (mc_controllerMap: with none, they are disabled).
    fn enabled(self, pad: PadReady) -> bool {
        match self {
            SetRow::ControllerLayout => pad.chosen,
            SetRow::PadScheme(_) => pad.connected,
            _ => true,
        }
    }
}

/// Active Game Controls is on Controller, and a controller is connected.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
struct PadReady {
    chosen: bool,
    connected: bool,
}

/// The settings' steppers (MenuItemStepper).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Stepper {
    TeamColorMode,
    SplitScreen,
    Quality,
    Fullscreen,
    VSync,
    FramerateCap,
    ActiveControls,
    Vibration,
    VehicleBoost,
    StickDelay,
    RobotCamera,
    VehicleCamera,
    JetCamera,
    SelectLayout,
    ViewLayout,
    /// The Controller Layout screen's rows (their ControlSchemes).
    PadScheme(PadRow),
}

const STEPPERS: [Stepper; 18] = [
    Stepper::TeamColorMode,
    Stepper::SplitScreen,
    Stepper::Quality,
    Stepper::Fullscreen,
    Stepper::VSync,
    Stepper::FramerateCap,
    Stepper::ActiveControls,
    Stepper::Vibration,
    Stepper::VehicleBoost,
    Stepper::StickDelay,
    Stepper::RobotCamera,
    Stepper::VehicleCamera,
    Stepper::JetCamera,
    Stepper::SelectLayout,
    Stepper::ViewLayout,
    Stepper::PadScheme(PadRow::Robot),
    Stepper::PadScheme(PadRow::Vehicle),
    Stepper::PadScheme(PadRow::Sticks),
];
/// The most values a stepper has (Select Layout's, with Custom).
const MAX_STEPS: usize = controls::LAYOUTS + 1;

/// The settings' values, as the steppers show them.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
struct Choices {
    fullscreen: bool,
    vsync: bool,
    /// An index into `FRAMERATE_CAPS`.
    cap: usize,
    /// The Graphics level (graphics.rs's LEVELS).
    quality: usize,
    /// Team Coloring Mode on Autobot vs Decepticon.
    faction_colors: bool,
    /// Split Screen side by side.
    split_side: bool,
    vibration: bool,
    hold_boost: bool,
    stick_delay: bool,
    invert: [bool; 3],
    /// Active Game Controls: Controller.
    controller: bool,
    /// Select Layout's choice, and how many it offers.
    layout: usize,
    layouts: usize,
    /// View Layout's.
    view: Mode,
    /// Controller Layout's schemes (each row's).
    pad: [usize; 3],
}

impl Stepper {
    /// Where its tweens and value texts are kept.
    fn index(self) -> usize {
        STEPPERS.iter().position(|&s| s == self).unwrap_or(0)
    }

    /// Its values, in order (the movie's data providers: ToggleOnOffSettings' On,
    /// then Off; ToggleHoldSettings' Hold, Toggle; ToggleInvertedSettings' Normal,
    /// Inverted; TogglePreferredControls, KbdControlSchemes and
    /// KbdControlSchemeViews).
    fn values(self, text: &MenuText, choices: Choices) -> Vec<String> {
        let on_off = || {
            vec![
                text.get("Settings.On", "On"),
                text.get("Settings.Off", "Off"),
            ]
        };
        let normal_inverted = || {
            vec![
                text.get("Settings.Normal", "Normal"),
                text.get("Settings.Inverted", "Inverted"),
            ]
        };
        match self {
            // (Its values are drawn in their colours: `team_color_parts`.)
            Stepper::TeamColorMode => team_color_parts(text)
                .into_iter()
                .map(|parts| parts.map(|(part, _)| part).concat())
                .collect(),
            Stepper::SplitScreen => vec!["Top and Bottom".to_string(), "Side by Side".to_string()],
            // (GraphicsSettings.)
            Stepper::Quality => [
                ("Settings.Low", "Low"),
                ("Settings.Medium", "Medium"),
                ("Settings.High", "High"),
                ("Settings.Extra", "Extra"),
            ]
            .iter()
            .map(|(key, fallback)| text.get(key, fallback))
            .collect(),
            Stepper::Fullscreen | Stepper::VSync | Stepper::Vibration | Stepper::StickDelay => {
                on_off()
            }
            Stepper::FramerateCap => FRAMERATE_CAPS
                .iter()
                .map(|c| c.map_or_else(|| "None".to_string(), |fps| fps.to_string()))
                .collect(),
            Stepper::ActiveControls => vec![
                text.get("Settings.KeyboardMouse", "Keyboard/Mouse"),
                text.get("Settings.Controller", "Controller"),
            ],
            Stepper::VehicleBoost => vec![
                text.get("Settings.Hold", "Hold"),
                text.get("Settings.Toggle", "Toggle"),
            ],
            Stepper::RobotCamera | Stepper::VehicleCamera | Stepper::JetCamera => normal_inverted(),
            Stepper::SelectLayout => (0..choices.layouts)
                .map(|i| {
                    let (key, fallback) = controls::layout_text(i);
                    text.get(key, fallback)
                })
                .collect(),
            Stepper::ViewLayout => Mode::ALL
                .iter()
                .map(|m| {
                    let (key, fallback) = m.text();
                    text.get(key, fallback)
                })
                .collect(),
            Stepper::PadScheme(row) => row
                .schemes()
                .iter()
                .map(|s| text.get(s.key, s.name))
                .collect(),
        }
    }

    /// The selected value's index.
    fn selected(self, choices: Choices) -> usize {
        let index = |first: bool| if first { 0 } else { 1 };
        match self {
            Stepper::TeamColorMode => usize::from(choices.faction_colors),
            Stepper::SplitScreen => usize::from(choices.split_side),
            Stepper::Quality => choices.quality,
            Stepper::Fullscreen => index(choices.fullscreen),
            Stepper::VSync => index(choices.vsync),
            Stepper::FramerateCap => choices.cap,
            Stepper::ActiveControls => usize::from(choices.controller),
            Stepper::Vibration => index(choices.vibration),
            Stepper::VehicleBoost => index(choices.hold_boost),
            Stepper::StickDelay => index(choices.stick_delay),
            Stepper::RobotCamera => usize::from(choices.invert[0]),
            Stepper::VehicleCamera => usize::from(choices.invert[1]),
            Stepper::JetCamera => usize::from(choices.invert[2]),
            Stepper::SelectLayout => choices.layout,
            Stepper::ViewLayout => choices.view.index(),
            Stepper::PadScheme(row) => choices.pad[row.index()],
        }
    }

    /// Picks value `i`.
    fn select(self, i: usize, settings: &mut Settings) {
        let first = i == 0;
        match self {
            Stepper::TeamColorMode => settings.display.faction_colors = !first,
            Stepper::SplitScreen => settings.display.split_side_by_side = !first,
            Stepper::Quality => settings.quality.0 = i,
            // (Kept for Accept, as the game keeps them: gfxChange.)
            Stepper::Fullscreen => settings.change_display(|d| d.fullscreen = first),
            Stepper::VSync => settings.change_display(|d| d.vsync = first),
            Stepper::FramerateCap => settings.cap.0 = i,
            Stepper::ActiveControls => settings.controls.controller = i == 1,
            Stepper::Vibration => settings.controls.vibration = first,
            Stepper::VehicleBoost => settings.controls.hold_boost = first,
            Stepper::StickDelay => settings.controls.stick_delay = first,
            Stepper::RobotCamera => settings.controls.invert[0] = !first,
            Stepper::VehicleCamera => settings.controls.invert[1] = !first,
            Stepper::JetCamera => settings.controls.invert[2] = !first,
            Stepper::SelectLayout => settings.controls.layout = i,
            // (The view is the menu's own: see settings_input.)
            Stepper::ViewLayout => {}
            Stepper::PadScheme(row) => settings.controls.pad_schemes[row.index()] = i,
        }
    }

    /// The values' x (from the first) and widths when they are laid out side by
    /// side.
    fn spans(self, text: &MenuText, choices: Choices) -> Vec<(f32, f32)> {
        let mut x = 0.0;
        self.values(text, choices)
            .iter()
            .map(|value| {
                let w = text.width(DEMI_BOLD, value, SET_VALUE_SIZE);
                let span = (x, w);
                x += w + STEP_GAP;
                span
            })
            .collect()
    }
}

/// Team Coloring Mode's values as the settings movie writes them
/// (ToggleTeamColoringModeSettings' htmlText): "Ally vs Enemy" and "Autobot vs
/// Decepticon", each word in its own colour (Ally #7FFFE8, Enemy and Autobot
/// #FF8787, Decepticon #D1C0FF) and "vs" in the stepper's (None).
fn team_color_parts(text: &MenuText) -> [[(String, Option<Color>); 3]; 2] {
    let word = |key: &str, fallback: &str, color: u32| {
        let [_, r, g, b] = color.to_be_bytes();
        (text.get(key, fallback), Some(Color::srgb_u8(r, g, b)))
    };
    let versus = || (format!(" {} ", text.get("Shared.Versus", "vs")), None);
    [
        [
            word("Shared.Ally", "Ally", 0x7FFFE8),
            versus(),
            word("Shared.Enemy", "Enemy", 0xFF8787),
        ],
        [
            word("Shared.Autobot", "Autobot", 0xFF8787),
            versus(),
            word("Shared.Decepticon", "Decepticon", 0xD1C0FF),
        ],
    ]
}

/// The settings' sliders (MenuSlider).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Slider {
    MasterVolume,
    FxVolume,
    DialogVolume,
    MusicVolume,
    CameraSensitivity,
    MouseSensitivity,
    /// The Graphics menu's: the monitor's resolutions, then the desktop's.
    Resolution,
    /// The demo's own.
    RenderScale,
}

/// What the sliders show.
struct SliderSource<'a> {
    volumes: &'a Volumes,
    controls: &'a Controls,
    /// The Resolution as shown (waiting for Accept, or in effect), and the
    /// monitor's.
    resolution: Option<UVec2>,
    resolutions: &'a [UVec2],
    render_scale: f32,
}

impl Slider {
    /// Its range and step (snapInterval).
    fn range(self, s: &SliderSource) -> (f32, f32, f32) {
        let range = |(min, max, step): (u32, u32, u32)| (min as f32, max as f32, step as f32);
        match self {
            Slider::MasterVolume
            | Slider::FxVolume
            | Slider::DialogVolume
            | Slider::MusicVolume => (0.0, 1.0, VOLUME_STEP),
            Slider::CameraSensitivity => range(controls::CAMERA_SENSITIVITY),
            Slider::MouseSensitivity => range(controls::MOUSE_SENSITIVITY),
            // (Its maximum the number of resolutions less one, as the game's:
            // GetNumResolutions; one more here, the desktop's.)
            Slider::Resolution => (0.0, s.resolutions.len() as f32, 1.0),
            Slider::RenderScale => RENDER_SCALES,
        }
    }

    fn value(self, s: &SliderSource) -> f32 {
        match self {
            Slider::MasterVolume => s.volumes.master,
            Slider::FxVolume => s.volumes.effects,
            Slider::DialogVolume => s.volumes.dialog,
            Slider::MusicVolume => s.volumes.music,
            Slider::CameraSensitivity => s.controls.camera_sensitivity as f32,
            Slider::MouseSensitivity => s.controls.mouse_sensitivity as f32,
            // (A size the monitor hasn't, from another monitor, as the desktop's.)
            Slider::Resolution => s
                .resolution
                .and_then(|r| s.resolutions.iter().position(|&m| m == r))
                .unwrap_or(s.resolutions.len()) as f32,
            Slider::RenderScale => s.render_scale,
        }
    }

    /// Sets its value (one of its steps).
    fn set(self, value: f32, settings: &mut Settings) {
        match self {
            Slider::MasterVolume => settings.volumes.master = value,
            Slider::FxVolume => settings.volumes.effects = value,
            Slider::DialogVolume => settings.volumes.dialog = value,
            Slider::MusicVolume => settings.volumes.music = value,
            Slider::CameraSensitivity => {
                settings.controls.camera_sensitivity = value.round() as u32;
            }
            Slider::MouseSensitivity => {
                settings.controls.mouse_sensitivity = value.round() as u32;
            }
            // (Kept for Accept: gfxChange.)
            Slider::Resolution => {
                let size = settings.resolutions.0.get(value.round() as usize).copied();
                settings.change_display(|d| d.resolution = size);
            }
            Slider::RenderScale => settings.render_scale.0 = value,
        }
    }

    /// What a volume slider plays as it moves (Master Volume, the demo's own, the
    /// FX Volume's beep).
    fn sample(self) -> Option<&'static str> {
        match self {
            Slider::MasterVolume | Slider::FxVolume => Some(SND_SLIDER_VOLUME),
            Slider::DialogVolume => Some(SND_SLIDER_DIALOG),
            Slider::MusicVolume => Some(SND_SLIDER_MUSIC),
            Slider::CameraSensitivity
            | Slider::MouseSensitivity
            | Slider::Resolution
            | Slider::RenderScale => None,
        }
    }

    /// How full its bar is (0 to 1).
    fn fill(self, s: &SliderSource) -> f32 {
        let (min, max, _) = self.range(s);
        if max <= min {
            return 1.0;
        }
        ((self.value(s) - min) / (max - min)).clamp(0.0, 1.0)
    }

    /// The text beside it, if it has one: the resolution (the game's resLabel,
    /// beside its slider), the render scale.
    fn label(self, s: &SliderSource) -> Option<String> {
        match self {
            Slider::Resolution => Some(Resolution(s.resolution).text()),
            Slider::RenderScale => Some(format!("{:.0}%", s.render_scale * 100.0)),
            _ => None,
        }
    }
}

/// A stepper value's position, opacity and selected-ness (the renderer's "on" /
/// "off" state).
#[derive(Clone, Copy)]
struct StepTween {
    x: Tween,
    alpha: Tween,
    on: Tween,
}

impl StepTween {
    fn at(x: f32, alpha: f32, on: f32) -> Self {
        StepTween {
            x: Tween::at(x),
            alpha: Tween::at(alpha),
            on: Tween::at(on),
        }
    }
}

/// Where row `i` of a menu with `n` rows sits, from the anchor.
fn row_offset(i: usize, n: usize) -> f32 {
    i as f32 * SET_ROW_H - n as f32 * SET_ROW_H * 0.5
}

/// The movie's text tween (tweenFrom {text: old}): over the tween the new text
/// replaces the old from the left, letter by letter.
fn text_tween(from: &str, to: &str, k: f32) -> String {
    let from: Vec<char> = from.chars().collect();
    let to: Vec<char> = to.chars().collect();
    let k = k.clamp(0.0, 1.0);
    let a = ((k * to.len() as f32) as usize).min(to.len());
    let b = ((k * from.len() as f32) as usize).min(from.len());
    to[..a].iter().chain(from[b..].iter()).collect()
}

// ---------------------------------------------------------------------------
// Tweens (the movie's hms.motion.Tween with Penner's easing curves)
// ---------------------------------------------------------------------------

#[derive(Clone, Copy)]
enum Ease {
    Linear,
    RegularIn,
    RegularOut,
    StrongIn,
    StrongOut,
}

impl Ease {
    fn apply(self, t: f32) -> f32 {
        match self {
            Ease::Linear => t,
            Ease::RegularIn => t * t,
            Ease::RegularOut => t * (2.0 - t),
            Ease::StrongIn => t.powi(5),
            Ease::StrongOut => 1.0 - (1.0 - t).powi(5),
        }
    }
}

#[derive(Clone, Copy)]
struct Tween {
    from: f32,
    to: f32,
    start: f32,
    secs: f32,
    ease: Ease,
}

impl Tween {
    const fn at(value: f32) -> Self {
        Tween {
            from: value,
            to: value,
            start: 0.0,
            secs: 0.0,
            ease: Ease::Linear,
        }
    }

    fn get(&self, now: f32) -> f32 {
        if self.secs <= 0.0 {
            return self.to;
        }
        let t = ((now - self.start) / self.secs).clamp(0.0, 1.0);
        self.from + (self.to - self.from) * self.ease.apply(t)
    }

    /// Tween from where it is now.
    fn to(&mut self, now: f32, to: f32, secs: f32, ease: Ease) {
        let from = self.get(now);
        self.from_to(now, from, to, secs, ease);
    }

    fn from_to(&mut self, now: f32, from: f32, to: f32, secs: f32, ease: Ease) {
        *self = Tween {
            from,
            to,
            start: now,
            secs,
            ease,
        };
    }
}

// ---------------------------------------------------------------------------
// State
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Page {
    Main,
    Settings,
    Extras,
    /// The ending credits playing (from Extras).
    Credits,
    /// The Escalation lobby (its party lobby, joining, or a game's lobby).
    Lobby,
    /// In the game, no menu up.
    Game,
    /// In the game, the pause menu up.
    Pause,
    /// In the game, the Choose Character screen up over the pause menu (choose.rs).
    Choose,
    /// The Customize screen up (customize.rs), the menu hidden under it.
    Customize,
}

/// A table row waiting for its new key (on the Keyboard/Mouse Layout screen): the
/// row, and which of Move's keys it is on (0 for the others).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
struct Capture {
    row: usize,
    slot: usize,
}

struct ButtonState {
    /// Slide (0 or OVER_SHIFT), opacity and turn about the vertical axis (degrees).
    shift: Tween,
    alpha: Tween,
    turn: Tween,
    /// When the brackets started animating in (while focused).
    brackets_at: Option<f32>,
}

#[derive(Resource)]
pub(crate) struct MenuState {
    page: Page,
    focus: usize,
    buttons: Vec<ButtonState>,
    shell_y: Tween,
    logo_scale: Tween,
    logo_alpha: Tween,
    started: f32,
    summary_alpha: Tween,
    /// A choice being acted on, and when it was made.
    leaving: Option<(Item, f32)>,
    settings_at: f32,
    settings_alpha: Tween,
    /// The settings menu showing and its focused row.
    set_menu: SetMenu,
    set_focus: usize,
    /// Each row's focus (0 to 1: its label darkening and sliding left).
    row_focus: [Tween; MAX_ROWS],
    /// The selection bar's centre and height (1 for a menu row, shorter for a
    /// table row), and the menu's drift, from the anchor.
    bar_y: Tween,
    bar_h: Tween,
    drift: Tween,
    /// Where each open menu (the root menu, its sub-menu, and that one's) drifts
    /// from, from the anchor: mc_menu's initPosY, where the anchor was when the
    /// menu opened.
    set_bases: [f32; 3],
    /// The Keyboard/Mouse Layout screen's View Layout (the robot's keys whenever
    /// it opens), and the table row waiting for its new key.
    kbd_view: Mode,
    capture: Option<Capture>,
    /// Whether the controller can be used (Controller Layout and its rows), and
    /// the Controller Layout screen's labels.
    pad_ready: PadReady,
    pad_labels: PadLabels,
    pad_map: PadMap,
    /// The Graphics level the player was last told needs the demo started again
    /// (Game Restart Required: its textures).
    restart_told: Option<usize>,
    /// A left click ended the wait for a key: its release isn't a click.
    release_wait: bool,
    /// The Display menu's Brightness or Safe Frame screen, if one is open, and the
    /// mouse over its Accept button.
    screen: Option<DisplayScreen>,
    accept_over: bool,
    /// The title: its text tweening from `title_from` to `title_to` since
    /// `title_at`, and its height (the first row's).
    title_from: String,
    title_to: String,
    title_at: f32,
    title_y: Tween,
    /// Each stepper's values (by `Stepper::index`).
    steps: [[StepTween; MAX_STEPS]; STEPPERS.len()],
    /// The Extras menu: when it opened, its focused button and each button's focus,
    /// its selection bar's centre (from the menu) and when the bar last moved, and
    /// whether the credits movie is there to play.
    extras_at: f32,
    extras_focus: usize,
    extras_row_focus: [Tween; EXTRAS_ITEMS.len()],
    extras_bar_y: Tween,
    extras_bar_at: f32,
    credits_ready: bool,
    /// The mouse is over the Back button.
    back_over: bool,
    /// Black over everything (fading out to start the game, then back in).
    fade: Tween,
    /// The menu music's volume (faded out when the game starts).
    music_level: Tween,
    last_cursor: Option<Vec2>,
    dragging: bool,
    /// Key/stick repeat for the slider: (direction, next repeat time).
    repeat: Option<(f32, f32)>,
    stick_y: f32,
    stick_x: f32,
    quit_at: Option<f32>,
    game_at: Option<f32>,
    /// The pause menu: when it opened, whether it is the host's (with Return to
    /// Lobby), its focused row and each row's focus, its selection bar, drift and
    /// title (as the settings menus' are), and when the game, faded out, goes back
    /// to the menus (Quit Game; the match ended, or online play stopped).
    pause_at: f32,
    pause_host: bool,
    pause_focus: usize,
    pause_row_focus: [Tween; PAUSE_MAX_ROWS],
    pause_bar_y: Tween,
    pause_drift: Tween,
    pause_title_from: String,
    pause_title_to: String,
    pause_title_at: f32,
    pause_title_y: Tween,
    quit_game_at: Option<f32>,
    /// The game has started: only the fader is left, fading out.
    gone: bool,
    /// The Escalation lobby: when it opened from the main menu and when it closed
    /// (its clouds fade in and out), its view and when that came up, its menu's
    /// focused row (in `lobby_rows`) with each row's focus and the selection bar's
    /// centre (from the first row's top), and the mouse over its Back button.
    lobby_opened: f32,
    lobby_closed: f32,
    lobby_view: LobbyView,
    lobby_at: f32,
    lobby_focus: usize,
    lobby_row_focus: [Tween; LOBBY_MAX_ROWS],
    lobby_bar_y: Tween,
    lobby_back_over: bool,
    /// Split screen in the game's lobby (lobby_guest): what Start Game's hint
    /// adds (how the second player comes in, or goes).
    guest_hint: &'static str,
    /// Split screen in the party lobby (party_guest): Player 2 in the party (listed
    /// under the first player), or "Press START to join" there in his place while a
    /// controller he could join on is connected.
    party_guest: bool,
    party_hint: bool,
    /// The ticker's message and when it started going by.
    ticker: String,
    ticker_at: f32,
    /// The countdown's second (for its sound and its number's pop) and when it
    /// came.
    countdown_second: Option<u32>,
    countdown_at: f32,
    /// The settings menus were opened from the lobby (and close back to it).
    settings_from_lobby: bool,
    /// The Customize screen was opened from the lobby (and closes back to it).
    customize_from_lobby: bool,
    /// What the party lobby offers of Prime Mode (prime_mode keeps it as xp.rs's
    /// progress has it), and its question just confirmed (prime_mode turns it on).
    prime: PrimeOffer,
    prime_confirmed: bool,
    /// A prompt over the menus, when it opened, the button the mouse is over and
    /// the button with the focus (prompt_buttons).
    prompt: Option<Prompt>,
    prompt_at: f32,
    prompt_over: Option<usize>,
    prompt_focus: usize,
    /// Join Game's address, as typed (kept in settings.txt).
    address: String,
    /// Display Name's name, as typed.
    name_entry: String,
    /// Game Password's password, as typed (hosting, or for the game being joined;
    /// never saved).
    password_entry: String,
    /// A message (title and text) for the main menu to show when it comes back
    /// (online play stopped in a match; the host ended it).
    pending_message: Option<(String, String)>,
    /// The match is over and its players still in the game: the menus come back
    /// in its lobby rather than at the main menu.
    return_to_lobby: bool,
}

impl MenuState {
    fn new(now: f32) -> Self {
        let buttons = (0..ITEMS.len())
            .map(|i| {
                let mut alpha = Tween::at(0.0);
                // initButtons: each visible button fades in, the later ones slower.
                alpha.from_to(now, 0.0, 1.0, 0.5 + 0.1 * (i + 1) as f32, Ease::StrongIn);
                ButtonState {
                    shift: Tween::at(0.0),
                    alpha,
                    turn: Tween::at(0.0),
                    brackets_at: None,
                }
            })
            .collect();
        let mut shell_y = Tween::at(SHELL_Y - SHELL_DROP);
        shell_y.to(now, SHELL_Y, 0.3, Ease::RegularOut);
        let mut state = MenuState {
            page: Page::Main,
            focus: 1,
            buttons,
            shell_y,
            logo_scale: Tween::at(1.0),
            logo_alpha: Tween::at(1.0),
            started: now,
            summary_alpha: Tween::at(0.0),
            leaving: None,
            settings_at: 0.0,
            settings_alpha: Tween::at(0.0),
            set_menu: SetMenu::Root,
            set_focus: 0,
            row_focus: [Tween::at(0.0); MAX_ROWS],
            bar_y: Tween::at(0.0),
            bar_h: Tween::at(1.0),
            drift: Tween::at(0.0),
            set_bases: [0.0; 3],
            kbd_view: Mode::Robot,
            pad_ready: PadReady::default(),
            pad_labels: PadLabels::default(),
            pad_map: PadMap::default(),
            capture: None,
            restart_told: None,
            release_wait: false,
            screen: None,
            accept_over: false,
            title_from: String::new(),
            title_to: String::new(),
            title_at: 0.0,
            title_y: Tween::at(0.0),
            steps: [[StepTween::at(0.0, 0.0, 0.0); MAX_STEPS]; STEPPERS.len()],
            extras_at: -100.0,
            extras_focus: 0,
            extras_row_focus: [Tween::at(0.0); EXTRAS_ITEMS.len()],
            extras_bar_y: Tween::at(0.0),
            extras_bar_at: -100.0,
            credits_ready: false,
            back_over: false,
            fade: Tween::at(0.0),
            music_level: Tween::at(1.0),
            last_cursor: None,
            dragging: false,
            repeat: None,
            stick_y: 0.0,
            stick_x: 0.0,
            quit_at: None,
            game_at: None,
            pause_at: -100.0,
            pause_host: false,
            pause_focus: 0,
            pause_row_focus: [Tween::at(0.0); PAUSE_MAX_ROWS],
            pause_bar_y: Tween::at(0.0),
            pause_drift: Tween::at(0.0),
            pause_title_from: String::new(),
            pause_title_to: String::new(),
            pause_title_at: 0.0,
            pause_title_y: Tween::at(0.0),
            quit_game_at: None,
            gone: false,
            lobby_opened: -100.0,
            lobby_closed: -100.0,
            lobby_view: LobbyView::Party,
            lobby_at: -100.0,
            lobby_focus: 0,
            lobby_row_focus: [Tween::at(0.0); LOBBY_MAX_ROWS],
            lobby_bar_y: Tween::at(0.0),
            lobby_back_over: false,
            guest_hint: "",
            party_guest: false,
            party_hint: false,
            ticker: String::new(),
            ticker_at: 0.0,
            countdown_second: None,
            countdown_at: 0.0,
            settings_from_lobby: false,
            customize_from_lobby: false,
            prime: PrimeOffer::Locked,
            prime_confirmed: false,
            prompt: None,
            prompt_at: 0.0,
            prompt_over: None,
            prompt_focus: 0,
            address: saved_setting("join_address").unwrap_or_default(),
            name_entry: String::new(),
            password_entry: String::new(),
            pending_message: None,
            return_to_lobby: false,
        };
        // The game focuses CAMPAIGN; here ESCALATION, the item that plays.
        state.set_focus(now, 1, true);
        state
    }

    /// MainMenuButton's stateHandler: the newly focused button slides left and its
    /// brackets open; the old one slides back; the description fades in anew.
    fn set_focus(&mut self, now: f32, focus: usize, force: bool) {
        if focus == self.focus && !force {
            return;
        }
        let old = self.focus;
        if old != focus {
            let b = &mut self.buttons[old];
            b.shift.to(now, 0.0, 0.5, Ease::RegularOut);
            b.brackets_at = None;
        }
        self.focus = focus;
        let b = &mut self.buttons[focus];
        b.shift.to(now, OVER_SHIFT, 0.5, Ease::RegularOut);
        b.brackets_at = Some(now);
        self.summary_alpha
            .from_to(now, 0.0, 1.0, 1.0, Ease::RegularOut);
    }

    /// onExitMainMenuBegin: the logo shrinks away, the buttons turn and fade one
    /// after another, the description fades.
    fn leave_main(&mut self, now: f32, item: Item, sfx: &mut Sfx) {
        sfx.play(SND_PRESS_IN);
        self.leaving = Some((item, now));
        self.logo_scale.to(now, 0.7, 0.25, Ease::Linear);
        self.logo_alpha.to(now, 0.0, 0.25, Ease::Linear);
        for (i, b) in self.buttons.iter_mut().enumerate() {
            let secs = 0.1 * i as f32;
            b.alpha.to(now, 0.0, secs, Ease::RegularIn);
            b.turn
                .to(now, 10.0 + 20.0 * i as f32, secs, Ease::RegularIn);
        }
        self.summary_alpha.to(now, 0.0, 0.25, Ease::Linear);
    }

    /// onEnterMainMenu: the buttons turn back and fade in, the logo returns.
    fn enter_main(&mut self, now: f32, sfx: &mut Sfx) {
        sfx.play(SND_PRESS_OUT);
        self.page = Page::Main;
        self.leaving = None;
        for (i, b) in self.buttons.iter_mut().enumerate() {
            let secs = 0.4 + 0.2 * i as f32;
            b.alpha.from_to(now, 0.0, 1.0, secs, Ease::StrongOut);
            b.turn
                .from_to(now, 10.0 + 10.0 * i as f32, 0.0, secs, Ease::StrongOut);
        }
        self.summary_alpha
            .from_to(now, 0.0, 1.0, 1.0, Ease::StrongOut);
        self.logo_scale.to(now, 1.0, 1.0, Ease::StrongOut);
        self.logo_alpha.to(now, 1.0, 1.0, Ease::StrongOut);
        let focus = self.focus;
        self.set_focus(now, focus, true);
    }

    /// openSettingsMenu: the root menu, focused on its first row, its title typed
    /// in.
    fn open_settings(&mut self, now: f32, text: &MenuText, choices: Choices) {
        self.page = Page::Settings;
        self.leaving = None;
        self.screen = None;
        self.settings_at = now;
        self.settings_alpha
            .from_to(now, 0.0, 1.0, 0.2, Ease::StrongOut);
        self.dragging = false;
        self.back_over = false;
        self.title_from.clear();
        self.title_to.clear();
        self.set_menu = SetMenu::Root;
        self.set_bases = [0.0; 3];
        let y = self.row_y(0);
        self.bar_y = Tween::at(y + 14.5);
        self.bar_h = Tween::at(1.0);
        self.drift = Tween::at(SET_DRIFT * y);
        self.title_y = Tween::at(y);
        self.show_menu(now, SetMenu::Root, 0, text, choices);
    }

    /// A Display screen opens over the settings (openBrightness, openSafeFrame).
    fn open_screen(&mut self, now: f32, kind: ScreenKind, display: &DisplaySettings) {
        self.screen = Some(DisplayScreen {
            kind,
            at: now,
            brightness: display.brightness,
            frame: display.safe_frame,
            changed: false,
            steps: [None; 2],
        });
        self.dragging = false;
        self.repeat = None;
        self.accept_over = false;
    }

    /// The rows of the settings menu showing.
    fn rows(&self) -> Vec<SetRow> {
        self.set_menu.rows(self.kbd_view)
    }

    /// updateControllerMap: on the Controller Layout screen, each label says what
    /// its control does in the focused row's mode with the schemes `chosen` (or
    /// nothing with no controller): a label whose text changes comes in anew, one
    /// left with nothing fades out. (The game's updates as the focus moves; here as
    /// a scheme changes too, which changes them as much.)
    fn update_pad_labels(&mut self, now: f32, chosen: [usize; 3]) {
        if self.set_menu != SetMenu::ControllerLayout {
            self.pad_labels.shown_for = None;
            return;
        }
        let row = match self.rows().get(self.set_focus) {
            Some(&SetRow::PadScheme(row)) => row,
            _ => PadRow::Robot,
        };
        let connected = self.pad_ready.connected;
        let key = (row, chosen, connected);
        if self.pad_labels.shown_for == Some(key) {
            return;
        }
        // (Opened afresh, every label starts faded out: ControllerLabel's
        // initialized state.)
        let fresh = self.pad_labels.shown_for.is_none();
        let was_connected = self.pad_labels.shown_for.is_some_and(|(_, _, c)| c);
        self.pad_labels.shown_for = Some(key);
        if !connected && (fresh || was_connected) {
            self.pad_labels.none_at = now;
        }
        let bindings = padlayout::bindings(chosen);
        let labels = &mut self.pad_labels.labels;
        labels.resize(self.pad_map.labels.len(), PadLabel::default());
        for (map, label) in self.pad_map.labels.iter().zip(labels.iter_mut()) {
            if fresh {
                *label = PadLabel::default();
            }
            let text = if connected {
                padlayout::description(&bindings, &map.key, row)
            } else {
                String::new()
            };
            if text == label.text {
                continue;
            }
            if text.is_empty() {
                label.leaving = Some((std::mem::take(&mut label.text), now));
            } else {
                *label = PadLabel {
                    text,
                    at: now,
                    leaving: None,
                };
            }
        }
    }

    /// Where its row `i` is (its top, from the anchor, before the drift).
    fn row_y(&self, i: usize) -> f32 {
        self.set_menu.row_y(i, &self.rows())
    }

    /// The title as it reads now (mid-tween, it is part old and part new).
    fn title(&self, now: f32) -> String {
        let secs = self.title_to.chars().count() as f32 * 0.05;
        let k = if secs > 0.0 {
            (now - self.title_at) / secs
        } else {
            1.0
        };
        text_tween(&self.title_from, &self.title_to, k)
    }

    /// A menu takes the focus (mc_menu's initMenu / handleFocus): only it shows,
    /// its row `focus` focused; the title types its name over the old one and moves
    /// to its first row; the selection bar and the drift follow the focus. (The
    /// Keyboard/Mouse Layout screen opens on the robot's keys: its
    /// initModeSelectedIndex.)
    fn show_menu(
        &mut self,
        now: f32,
        menu: SetMenu,
        focus: usize,
        text: &MenuText,
        choices: Choices,
    ) {
        if menu == SetMenu::KeyboardLayout && self.set_menu != SetMenu::KeyboardLayout {
            self.kbd_view = Mode::Robot;
        }
        // A sub-menu drifts from where the menus were when it opened.
        if menu.depth() > self.set_menu.depth() {
            self.set_bases[menu.depth()] = self.drift.get(now);
        }
        let choices = Choices {
            view: self.kbd_view,
            ..choices
        };
        self.set_menu = menu;
        self.set_focus = focus;
        self.dragging = false;
        self.repeat = None;
        self.capture = None;
        for (i, t) in self.row_focus.iter_mut().enumerate() {
            *t = Tween::at(0.0);
            if i == focus {
                t.from_to(now, 0.0, 1.0, SET_FOCUS_SECS, Ease::RegularOut);
            }
        }
        let (key, fallback) = menu.title();
        self.title_from = self.title(now);
        self.title_to = text.get(key, fallback).to_uppercase();
        self.title_at = now;
        let y = self.row_y(0);
        self.title_y.to(now, y, 0.2, Ease::RegularOut);
        self.move_bar(now);
        // The steppers start collapsed, or open where the row has the focus.
        for stepper in STEPPERS {
            let selected = stepper.selected(choices);
            for (i, step) in self.steps[stepper.index()].iter_mut().enumerate() {
                let on = if i == selected { 1.0 } else { 0.0 };
                *step = StepTween::at(0.0, on, on);
            }
        }
        if let Some(stepper) = self.rows().get(focus).and_then(|r| r.stepper()) {
            self.lay_out_stepper(now, stepper, true, text, choices);
        }
    }

    fn move_bar(&mut self, now: f32) {
        let y = self.row_y(self.set_focus);
        let table = matches!(self.rows().get(self.set_focus), Some(SetRow::Binding(_)));
        let h = if table { TABLE_BAR_SCALE } else { 1.0 };
        self.bar_y.to(now, y + 14.5, 0.3, Ease::StrongOut);
        self.bar_h.to(now, h, 0.3, Ease::StrongOut);
        // (The table is one of the menu's items: it drifts the menu as one.)
        let item_y = if table { self.row_y(2) } else { y };
        let base = self.set_bases[self.set_menu.depth()];
        self.drift
            .to(now, base + SET_DRIFT * item_y, 0.3, Ease::RegularOut);
    }

    /// handleBtnFocus: the focus moves to another row of the menu (not to one that
    /// can't be used).
    fn focus_row(&mut self, now: f32, i: usize, text: &MenuText, choices: Choices, sfx: &mut Sfx) {
        let rows = self.rows();
        if i == self.set_focus || i >= rows.len() || !rows[i].enabled(self.pad_ready) {
            return;
        }
        // (The movie plays these the other way round from the main menu: "down"
        // when the bar moves up, "up" when it moves down.)
        let y = self.row_y(i);
        if self.bar_y.get(now) > y {
            sfx.play(SND_SCROLL_DOWN);
        } else {
            sfx.play(SND_SCROLL_UP);
        }
        let old = self.set_focus;
        self.row_focus[old].to(now, 0.0, SET_FOCUS_SECS, Ease::RegularOut);
        self.row_focus[i].to(now, 1.0, SET_FOCUS_SECS, Ease::RegularOut);
        if let Some(stepper) = rows[old].stepper() {
            self.lay_out_stepper(now, stepper, false, text, choices);
        }
        if let Some(stepper) = rows[i].stepper() {
            self.lay_out_stepper(now, stepper, true, text, choices);
        }
        self.set_focus = i;
        self.move_bar(now);
    }

    /// The next row up or down from the focus that can be used (None at the end).
    fn next_row(&self, down: bool) -> Option<usize> {
        let rows = self.rows();
        let mut i = self.set_focus;
        loop {
            i = if down { i + 1 } else { i.checked_sub(1)? };
            if rows.get(i)?.enabled(self.pad_ready) {
                return Some(i);
            }
        }
    }

    /// ItemStepper's setState for a collapsible stepper: focused, its values open
    /// out side by side; otherwise the others fade and the selected one moves to
    /// the start.
    fn lay_out_stepper(
        &mut self,
        now: f32,
        stepper: Stepper,
        focused: bool,
        text: &MenuText,
        choices: Choices,
    ) {
        let selected = stepper.selected(choices);
        let steps = &mut self.steps[stepper.index()];
        for (i, (x, _)) in stepper.spans(text, choices).into_iter().enumerate() {
            let step = &mut steps[i];
            if focused {
                step.x.to(now, x, STEP_SECS, Ease::RegularOut);
                step.alpha.to(now, 1.0, STEP_SECS, Ease::RegularOut);
            } else if i == selected {
                step.x.to(now, 0.0, STEP_SECS, Ease::RegularOut);
                step.alpha.to(now, 1.0, STEP_SECS, Ease::RegularOut);
            } else {
                step.alpha.to(now, 0.0, STEP_SECS, Ease::RegularOut);
            }
        }
    }

    /// A stepper's selection changed: its renderers swap "on" and "off".
    fn select_step(&mut self, now: f32, stepper: Stepper, choices: Choices) {
        let selected = stepper.selected(choices);
        for (i, step) in self.steps[stepper.index()].iter_mut().enumerate() {
            let on = if i == selected { 1.0 } else { 0.0 };
            step.on.to(now, on, STEP_STATE_SECS, Ease::Linear);
        }
    }

    /// The Extras menu opens (ExtrasMenu_GFX loaded over the front end): its title
    /// slides in, its background darkens the movie further, and its first button
    /// takes the focus.
    fn open_extras(&mut self, now: f32) {
        self.page = Page::Extras;
        self.leaving = None;
        self.extras_at = now;
        self.back_over = false;
        self.extras_focus = 0;
        for t in self.extras_row_focus.iter_mut() {
            *t = Tween::at(0.0);
        }
        self.extras_row_focus[0].from_to(now, 0.0, 1.0, EXTRAS_FOCUS_SECS, Ease::RegularOut);
        self.extras_bar_y = Tween::at(EXTRAS_ROW_Y + EXTRAS_PANEL_H * 0.5);
        self.extras_bar_at = now;
    }

    /// Another Extras button takes the focus (changeHover: the bar moves under it,
    /// growing from a quarter of its width; the button's "over" state).
    fn focus_extras_row(&mut self, now: f32, i: usize, sfx: &mut Sfx) {
        if i == self.extras_focus || i >= EXTRAS_ITEMS.len() {
            return;
        }
        sfx.play(SND_BUTTON_UP);
        let old = self.extras_focus;
        self.extras_row_focus[old].to(now, 0.0, EXTRAS_FOCUS_SECS, Ease::RegularOut);
        self.extras_row_focus[i].to(now, 1.0, EXTRAS_FOCUS_SECS, Ease::RegularOut);
        self.extras_focus = i;
        let y = EXTRAS_ROW_Y + i as f32 * EXTRAS_ROW_H + EXTRAS_PANEL_H * 0.5;
        self.extras_bar_y
            .to(now, y, EXTRAS_BAR_SECS, Ease::RegularOut);
        self.extras_bar_at = now;
    }

    /// How far the Extras movie's own background has faded in (0 to 1).
    fn extras_background(&self, now: f32) -> f32 {
        Ease::RegularOut.apply(((now - self.extras_at) / EXTRAS_BACKGROUND_SECS).clamp(0.0, 1.0))
    }

    /// The pause menu opens (the shell attached, its menu focused on Resume Game),
    /// the host's with Return to Lobby. Its title types "PAUSED" over the title
    /// field's own text.
    fn open_pause(&mut self, now: f32, text: &MenuText, host: bool) {
        self.page = Page::Pause;
        self.pause_at = now;
        self.pause_host = host;
        self.pause_focus = 0;
        for (i, t) in self.pause_row_focus.iter_mut().enumerate() {
            *t = Tween::at(0.0);
            if i == 0 {
                t.from_to(now, 0.0, 1.0, SET_FOCUS_SECS, Ease::RegularOut);
            }
        }
        let title = text.get("Pause.Paused", "Paused");
        self.pause_title_to = title.to_uppercase();
        self.pause_title_from = title;
        self.pause_title_at = now;
        // The title starts 80 pixels up and drops onto the first row.
        let y = self.pause_row_offset(0);
        self.pause_title_y = Tween::at(-80.0);
        self.pause_title_y.to(now, y, 0.2, Ease::StrongOut);
        self.pause_bar_y = Tween::at(y + 14.5);
        self.pause_drift = Tween::at(SET_DRIFT * y);
        self.back_over = false;
    }

    /// The pause title as it reads now.
    fn pause_title(&self, now: f32) -> String {
        let secs = self.pause_title_to.chars().count() as f32 * 0.05;
        let k = if secs > 0.0 {
            (now - self.pause_title_at) / secs
        } else {
            1.0
        };
        text_tween(&self.pause_title_from, &self.pause_title_to, k)
    }

    /// The pause menu's items as it shows them.
    fn pause_items(&self) -> &'static [PauseItem] {
        pause_items(self.pause_host)
    }

    /// Where its row `i` sits, from the menu's anchor.
    fn pause_row_offset(&self, i: usize) -> f32 {
        pause_row_offset(i, self.pause_items().len())
    }

    /// The focus moves to another pause row (the shell's focusChange: the bar and
    /// the drift follow).
    fn focus_pause_row(&mut self, now: f32, i: usize) {
        if i == self.pause_focus || i >= self.pause_items().len() {
            return;
        }
        let old = self.pause_focus;
        self.pause_row_focus[old].to(now, 0.0, SET_FOCUS_SECS, Ease::RegularOut);
        self.pause_row_focus[i].to(now, 1.0, SET_FOCUS_SECS, Ease::RegularOut);
        self.pause_focus = i;
        let y = self.pause_row_offset(i);
        self.pause_bar_y.to(now, y + 14.5, 0.3, Ease::StrongOut);
        self.pause_drift
            .to(now, SET_DRIFT * y, 0.3, Ease::RegularOut);
    }

    /// The settings close back to the pause menu (closeSettingsMenu: the shell
    /// shows again as it was, its menu focused).
    fn back_to_pause(&mut self) {
        self.page = Page::Pause;
        self.back_over = false;
    }

    /// ESCALATION: the lobby opens over the main menu, in its party lobby.
    fn open_lobby(&mut self, now: f32, net: &Net) {
        self.page = Page::Lobby;
        self.leaving = None;
        self.lobby_opened = now;
        self.prompt = None;
        self.show_lobby(now, LobbyView::Party, net);
    }

    /// The lobby shows `view`, its contents coming in, focused on its first row
    /// that can be used.
    fn show_lobby(&mut self, now: f32, view: LobbyView, net: &Net) {
        self.lobby_view = view;
        self.lobby_at = now;
        self.lobby_back_over = false;
        let rows = lobby_rows(view, net.is_host(), self.prime);
        let focus = rows
            .iter()
            .position(|row| row.state(net) == RowState::Enabled)
            .unwrap_or(0);
        self.lobby_focus = focus;
        for (i, t) in self.lobby_row_focus.iter_mut().enumerate() {
            *t = Tween::at(0.0);
            if i == focus {
                t.from_to(now, 0.0, 1.0, LOBBY_FOCUS_SECS, Ease::RegularOut);
            }
        }
        self.lobby_bar_y = Tween::at(focus as f32 * LOBBY_ROW_H + LOBBY_BAR_Y);
        self.ticker = TICKER_MESSAGE.to_string();
        self.ticker_at = now;
        self.countdown_second = None;
    }

    /// The focus moves to lobby row `i` (changeHover: the selection bar follows);
    /// false if it was there already.
    fn focus_lobby_row(&mut self, now: f32, i: usize) -> bool {
        if i == self.lobby_focus || i >= LOBBY_MAX_ROWS {
            return false;
        }
        let old = self.lobby_focus.min(LOBBY_MAX_ROWS - 1);
        self.lobby_row_focus[old].to(now, 0.0, LOBBY_FOCUS_SECS, Ease::RegularOut);
        self.lobby_row_focus[i].to(now, 1.0, LOBBY_FOCUS_SECS, Ease::RegularOut);
        self.lobby_focus = i;
        let y = i as f32 * LOBBY_ROW_H + LOBBY_BAR_Y;
        self.lobby_bar_y
            .to(now, y, LOBBY_BAR_SECS, Ease::RegularOut);
        true
    }

    /// The settings close back to the lobby they were opened from.
    fn back_to_lobby(&mut self) {
        self.page = Page::Lobby;
        self.settings_from_lobby = false;
        self.back_over = false;
    }

    /// A prompt opens over the menus.
    fn open_prompt(&mut self, now: f32, prompt: Prompt) {
        self.prompt = Some(prompt);
        self.prompt_at = now;
        self.prompt_over = None;
        self.prompt_focus = 0;
    }
}

/// The text and font widths from menu.txt.
#[derive(Resource, Default)]
pub(crate) struct MenuText {
    strings: HashMap<String, String>,
    /// Advance widths in ems, per font and character.
    advances: HashMap<String, HashMap<u32, f32>>,
}

impl MenuText {
    pub(crate) fn load(path: &std::path::Path) -> Self {
        let Ok(data) = std::fs::read_to_string(path) else {
            warn!("No {:?} (tools/build_menu.py makes it)", path);
            return MenuText::default();
        };
        MenuText::parse(&data)
    }

    /// From a file's text (menu.txt's lines; customize.txt holds its own).
    pub(crate) fn parse(data: &str) -> Self {
        let mut text = MenuText::default();
        for line in data.lines() {
            let mut parts = line.splitn(3, ' ');
            match (parts.next(), parts.next(), parts.next()) {
                (Some("string"), Some(key), Some(value)) => {
                    text.strings.insert(key.to_string(), value.to_string());
                }
                (Some("advance"), Some(font), Some(rest)) => {
                    let mut nums = rest.split_whitespace();
                    if let (Some(Ok(code)), Some(Ok(adv))) = (
                        nums.next().map(str::parse::<u32>),
                        nums.next().map(str::parse::<f32>),
                    ) {
                        text.advances
                            .entry(font.to_string())
                            .or_default()
                            .insert(code, adv);
                    }
                }
                _ => {}
            }
        }
        text
    }

    pub(crate) fn get(&self, key: &str, fallback: &str) -> String {
        self.strings
            .get(key)
            .cloned()
            .unwrap_or_else(|| fallback.to_string())
    }

    /// The width of `text` in `font` at `size` pixels (the sum of its advances,
    /// as Flash lays out a line).
    pub(crate) fn width(&self, font: &str, text: &str, size: f32) -> f32 {
        let table = self.advances.get(font);
        text.chars()
            .map(|c| {
                table
                    .and_then(|t| t.get(&(c as u32)).copied())
                    .unwrap_or(0.55)
            })
            .sum::<f32>()
            * size
    }
}

const DEMI_BOLD: &str = "RegisterDemiBold";
const MEDIUM: &str = "RegisterMedium";
const LIGHT: &str = "RegisterLight";

/// The menu's UI entities.
#[derive(Resource)]
struct MenuUi {
    root: Entity,
    background: Entity,
    logo: Entity,
    labels: Vec<Entity>,
    brackets: Vec<[Entity; 2]>,
    label_widths: Vec<f32>,
    descriptions: Vec<String>,
    summary: Entity,
    summary_glow: Vec<Entity>,
    hint_icon: Entity,
    hint_text: Entity,
    hint_width: f32,
    /// The demo's version at the main menu's bottom left (version_label).
    version: Entity,
    version_text: String,
    set_art: Entity,
    set_bar: Entity,
    set_title: Entity,
    /// The rows' labels, each stepper's values (by `Stepper::index`), the sliders,
    /// and the Keyboard/Mouse Layout screen's table (each row's action and key).
    set_rows: Vec<Entity>,
    set_values: [[Entity; MAX_STEPS]; STEPPERS.len()],
    /// The glow under the focused stepper's values (8 copies of each).
    set_value_glow: [[Entity; 8]; MAX_STEPS],
    set_sliders: Vec<SliderUi>,
    set_table: Vec<[Entity; 2]>,
    /// The Controller Layout screen's controller map.
    pad_map: PadMapUi,
    back_box: Entity,
    back_text: Entity,
    back_width: f32,
    /// The Display menu's screens and values.
    display: DisplayUi,
    /// The pause menu's parts.
    pause: PauseUi,
    /// The Extras menu's parts.
    extras: ExtrasUi,
    /// The Escalation lobby's, and the prompts'.
    lobby: LobbyUi,
    prompt: PromptUi,
    /// The ending credits: black behind them, and the movie.
    credits_back: Entity,
    credits_image: Entity,
    fader: Entity,
}

/// The Controller Layout screen's controller map: the controller's picture, each
/// label's art, the art in white over it (for ControllerLabel's WhiteColor) and its
/// text, and the line said with no controller.
struct PadMapUi {
    picture: Entity,
    labels: Vec<PadLabelUi>,
    none: PadTextUi,
}

struct PadLabelUi {
    art: Entity,
    white: Entity,
    text: PadTextUi,
}

/// A label's text, and the ring of copies drawing its shadow.
struct PadTextUi {
    text: Entity,
    shadow: Vec<Entity>,
}

/// A settings slider's parts: its track, its bar (in three: the ends keep their
/// size as it stretches, the movie's scale-9 grid) and its arrows.
struct SliderUi {
    track: Entity,
    fill: [Entity; 3],
    arrows: [Entity; 2],
    /// The text beside it (Slider::label).
    label: Entity,
}

/// The Extras menu's UI entities.
struct ExtrasUi {
    bar: Entity,
    title: Entity,
    hint: Entity,
    rows: Vec<Entity>,
}

/// The Escalation lobby's UI entities.
struct LobbyUi {
    /// clouds_bg's tiles: the image, and over it the image turned a quarter more;
    /// the shade over them (and black under the stage, for a tall window).
    clouds: Vec<[Entity; 2]>,
    clouds_shade: Entity,
    clouds_below: Entity,
    panels: Entity,
    mode_icon: Entity,
    /// The title (with a white glow) and subtitle.
    title_glow: Vec<Entity>,
    title: Entity,
    subtitle: Entity,
    /// The ticker's window (it clips its messages), and the messages going by.
    ticker_box: Entity,
    ticker: Vec<Entity>,
    /// A game's map: black behind it, its picture and shade, its frame and name.
    map_back: Entity,
    map_picture: Entity,
    /// Each of MAPS's pictures.
    map_pictures: Vec<Handle<Image>>,
    map_shade: Entity,
    map_frame: Entity,
    map_label_glow: Vec<Entity>,
    map_label: Entity,
    /// The selection bar (and its two images: a button's, a selector's), each row's
    /// label, value and arrows (and the arrows' image).
    bar: Entity,
    bar_images: [Handle<Image>; 2],
    rows: Vec<Entity>,
    values: Vec<Entity>,
    arrows: Vec<[Entity; 2]>,
    arrow_image: Handle<Image>,
    hint: Entity,
    /// The player list: its heading (with a glow), columns, and each player's
    /// name, score and connection (and the connection's three images).
    header_glow: Vec<Entity>,
    header: Entity,
    columns: [Entity; 2],
    names: Vec<Entity>,
    scores: Vec<Entity>,
    pings: Vec<Entity>,
    ping_images: [Handle<Image>; 3],
    /// Each player's Prime medal (its star and its ring), and the Prime Mode row's
    /// lock.
    medals: Vec<[Entity; 2]>,
    lock: Entity,
    status: Entity,
    countdown_label: Entity,
    countdown_number: Entity,
    /// Connecting to Host: the band, the icon spinning and the words.
    band: Entity,
    spinner: Entity,
    connecting: Entity,
    back_box: Entity,
    back_text: Entity,
    back_width: f32,
}

/// The prompts' UI entities.
struct PromptUi {
    dim: Entity,
    box_fill: Entity,
    bars: [Entity; 2],
    icon: Entity,
    icon_images: [Handle<Image>; 2],
    join_title: Entity,
    message_title: Entity,
    body: Entity,
    /// Join Game's text input: its border, fill and the light strip along its top,
    /// and in its window (which clips them) the address and the caret.
    input_border: Entity,
    input_fill: Entity,
    input_strip: Entity,
    input_clip: Entity,
    input_text: Entity,
    caret: Entity,
    /// The buttons: each a box, its label and the bar under it (the focused
    /// one's), and the focused box's picture (PROMPT_BUTTON_ON).
    buttons: Vec<[Entity; 3]>,
    button_on: Handle<Image>,
}

/// The pause shell's UI entities.
struct PauseUi {
    /// The warped hexagons' four quarters (PAUSE_HEX_QUARTERS).
    hexagons: [Entity; 4],
    glow: Entity,
    emblem: Entity,
    logo: Entity,
    caption: Entity,
    pips: Vec<Entity>,
    art: Entity,
    bar: Entity,
    title: Entity,
    rows: Vec<Entity>,
}

/// The Display menu's UI entities: its screens' (drawn over the settings, which
/// hide meanwhile), the Accept button they show beside Back, and Team Coloring
/// Mode's values (each in three parts, in their colours).
struct DisplayUi {
    /// Brightness: the glowing panel, the calibration picture, and the slider's
    /// track, bar (in three: its ends keep their size as it stretches) and arrows.
    panel: Entity,
    picture: Entity,
    track: Entity,
    bar: [Entity; 3],
    arrows: [Entity; 2],
    /// Safe Frame: the safe area, its arrows (up, right, down, left) and the white
    /// they flash as they step, and the band behind the directions (in halves).
    area: Entity,
    area_arrows: [Entity; 4],
    area_flashes: [Entity; 4],
    band: [Entity; 2],
    /// Either screen's directions.
    directions: Entity,
    accept_box: Entity,
    accept_text: Entity,
    accept_width: f32,
    team_colors: [[Entity; 3]; 2],
}

/// A decoded movie frame: its size and pixels (RGBA), and whether it was
/// darkened for the Extras menu as it was decoded.
struct Frame {
    w: u32,
    h: u32,
    rgba: Vec<u8>,
    darkened: bool,
}

type Frames = Mutex<Receiver<Frame>>;

/// The background movie: decoded on its own thread, a frame at a time.
#[derive(Resource)]
struct Movie {
    frames: Option<Frames>,
    /// How far behind the Extras menu the decoder darkens frames (see `darken`;
    /// an f32's bits, negative for not at all).
    darken: Arc<AtomicU32>,
    image: Option<Handle<Image>>,
    /// Menu time the next frame is due.
    next_at: f32,
    paused: bool,
    still: Handle<Image>,
    /// The movie file (decoded again when the game returns to the menu).
    path: PathBuf,
}

/// The ending credits movie (from Extras): decoded on its own thread while it
/// plays, with its sound.
#[derive(Resource)]
struct Credits {
    /// credits.h264, and its sound (loaded with the menu, so it starts at once).
    path: PathBuf,
    audio: Handle<AudioSource>,
    /// While playing: the decoded frames, the latest one, when it started, how
    /// many frames have been shown, and the sound playing.
    frames: Option<Frames>,
    image: Option<Handle<Image>>,
    started: f32,
    shown: u32,
    sound: Option<Entity>,
}

/// The Display screens' pictures made here: the calibration picture as the
/// brightness shows it (the game's display gamma shows the whole screen, the
/// picture with it; the menus here are drawn as they are, so the picture is made
/// to look as the gamma would show it: each value v as v ^ (2.2 / gamma)), and the
/// safe frame's arrow in white, for its flash.
#[derive(Resource)]
struct DisplayArt {
    picture: Handle<Image>,
    shown: Handle<Image>,
    /// The gamma `shown` was made for.
    shown_gamma: Option<f32>,
    arrow: Handle<Image>,
    flash: Handle<Image>,
    flash_made: bool,
}

/// Makes DisplayArt's pictures once their images have loaded, and the
/// calibration picture again as the brightness moves.
fn make_display_art(
    art: Option<ResMut<DisplayArt>>,
    mut images: ResMut<Assets<Image>>,
    display: Res<DisplaySettings>,
) {
    let Some(mut art) = art else {
        return;
    };
    // (Only RGBA pictures, as the menu's PNGs load.)
    let rgba = |image: &Image| {
        matches!(
            image.texture_descriptor.format,
            TextureFormat::Rgba8UnormSrgb | TextureFormat::Rgba8Unorm
        )
    };
    if !art.flash_made
        && let Some(arrow) = images.get(&art.arrow).filter(|i| rgba(i))
    {
        let mut white = arrow.clone();
        if let Some(data) = white.data.as_mut() {
            for pixel in data.chunks_exact_mut(4) {
                pixel[..3].fill(255);
            }
        }
        if images.insert(&art.flash, white).is_ok() {
            art.flash_made = true;
        }
    }
    let gamma = display.gamma();
    if art.shown_gamma != Some(gamma)
        && let Some(picture) = images.get(&art.picture).filter(|i| rgba(i))
    {
        let shown: [u8; 256] = std::array::from_fn(|v| {
            ((v as f32 / 255.0).powf(2.2 / gamma) * 255.0)
                .round()
                .clamp(0.0, 255.0) as u8
        });
        let mut image = picture.clone();
        if let Some(data) = image.data.as_mut() {
            for pixel in data.chunks_exact_mut(4) {
                for v in &mut pixel[..3] {
                    *v = shown[*v as usize];
                }
            }
        }
        if images.insert(&art.shown, image).is_ok() {
            art.shown_gamma = Some(gamma);
        }
    }
}

#[derive(Resource, Default)]
struct MenuMusic {
    music: Option<Music>,
    started: f32,
    voices: Vec<Option<Entity>>,
}

// ---------------------------------------------------------------------------
// Setup
// ---------------------------------------------------------------------------

fn setup_menu(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    time: Res<Time>,
    mut images: ResMut<Assets<Image>>,
    mut cursor: Single<&mut CursorOptions, With<PrimaryWindow>>,
    set_aside: Option<ResMut<crate::xp::ProgressSetAside>>,
) {
    let now = time.elapsed_secs();
    let dir = assets_dir().join("menu");
    let text = MenuText::load(&dir.join("menu.txt"));
    // progress.txt changed by hand, and set aside (xp.rs): the game's notice for a
    // damaged profile, as its profile is read (TnPlayerController.
    // OnReadProfileSettingsComplete: ShowPopupUI with GenericErrorTitle and
    // ResetCorruptProfile), over the main menu as it first shows.
    let set_aside_notice = set_aside
        .is_some_and(|mut s| std::mem::take(&mut s.0))
        .then(|| Prompt::Message {
            title: text.get("MessagePrompts.GenericErrorTitle", "Error"),
            text: text.get(
                "MessagePrompts.ResetCorruptProfile",
                "Profile data is corrupt.",
            ),
        });
    let img = |name: &str| asset_server.load::<Image>(format!("menu/img/{name}"));
    let demi: Handle<Font> = asset_server.load("menu/fonts/RegisterDemiBold.ttf");
    let medium: Handle<Font> = asset_server.load("menu/fonts/RegisterMedium.ttf");
    // $HeaderFont (the Extras menu's title and buttons).
    let light: Handle<Font> = asset_server.load("menu/fonts/RegisterLight.ttf");

    cursor.grab_mode = CursorGrabMode::None;
    cursor.visible = true;

    // The menu is drawn by its own camera, over the game's.
    let camera = commands
        .spawn((
            Camera2d,
            Camera {
                order: 10,
                clear_color: ClearColorConfig::None,
                ..default()
            },
            Tonemapping::None,
            // HDR like the game camera it draws over (see setup_camera).
            Hdr,
        ))
        .id();
    let root = commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                overflow: Overflow::clip(),
                ..default()
            },
            UiTargetCamera(camera),
            GlobalZIndex(100),
            BackgroundColor(Color::BLACK),
        ))
        .id();

    let image_node = |commands: &mut Commands, image: Handle<Image>| {
        commands
            .spawn((
                ImageNode {
                    image,
                    image_mode: NodeImageMode::Stretch,
                    ..default()
                },
                Node {
                    position_type: PositionType::Absolute,
                    ..default()
                },
                ChildOf(root),
            ))
            .id()
    };
    // An image drawn as a 9-slice: its `border` (texture pixels) keeps its size as
    // the rest stretches, as a movie's scale-9 grid does.
    let sliced_node = |commands: &mut Commands, image: Handle<Image>, border: BorderRect| {
        commands
            .spawn((
                ImageNode {
                    image,
                    image_mode: NodeImageMode::Sliced(TextureSlicer {
                        border,
                        center_scale_mode: SliceScaleMode::Stretch,
                        sides_scale_mode: SliceScaleMode::Stretch,
                        // (The ends follow the stage's scale.)
                        max_corner_scale: 100.0,
                    }),
                    ..default()
                },
                Node {
                    position_type: PositionType::Absolute,
                    ..default()
                },
                ChildOf(root),
            ))
            .id()
    };
    let text_node = |commands: &mut Commands, value: String, font: &Handle<Font>| {
        commands
            .spawn((
                Text::new(value),
                TextFont {
                    font: font.clone(),
                    font_size: 20.0,
                    ..default()
                },
                TextColor(Color::WHITE),
                TextLayout::new_with_no_wrap(),
                Node {
                    position_type: PositionType::Absolute,
                    ..default()
                },
                ChildOf(root),
            ))
            .id()
    };

    let still = img("background_still.png");
    let background = image_node(&mut commands, still.clone());
    // Gradients made here: the lobby's shade over its clouds, the prompts'
    // darkening of the screen and their box, and Connecting to Host's band. (The
    // UI blends in linear light, the movie in sRGB: darkenings are made to look as
    // dark as the movie's.)
    let mut gradient = |w: u32, h: u32, color: &dyn Fn(f32) -> [u8; 4]| {
        let n = w.max(h);
        let data = (0..n)
            .flat_map(|i| color(i as f32 / (n - 1) as f32))
            .collect();
        images.add(Image::new(
            Extent3d {
                width: w,
                height: h,
                depth_or_array_layers: 1,
            },
            TextureDimension::D2,
            data,
            TextureFormat::Rgba8UnormSrgb,
            RenderAssetUsages::default(),
        ))
    };
    let linear_darkening = |keep: f32| 1.0 - keep.clamp(0.0, 1.0).powf(2.2);
    // (Its colour solved for, so that over the clouds' average colour it blends
    // in linear light to what the movie's gradient makes of it in sRGB.)
    let shade_image = gradient(1, 64, &|t| {
        let alpha = linear_darkening(1.0 - t);
        let mut pixel = [0, 0, 0, (alpha * 255.0) as u8];
        for i in 0..3 {
            let cloud = CLOUDS_MEAN[i] / 255.0;
            let movie = cloud * (1.0 - t) + CLOUDS_SHADE_TOP[i] / 255.0 * (1.0 - t) * t;
            let linear = if alpha > 1e-3 {
                ((movie.powf(2.2) - cloud.powf(2.2) * (1.0 - alpha)) / alpha).clamp(0.0, 1.0)
            } else {
                0.0
            };
            pixel[i] = (linear.powf(1.0 / 2.2) * 255.0) as u8;
        }
        pixel
    });
    // (The front end's overlay, multiplied in: as the darkening it makes.)
    let dim_image = gradient(1, 64, &|t| {
        let c = overlay_color(t);
        let keep = (c[1] + c[2]) / (2.0 * 255.0);
        [0, 0, 0, (linear_darkening(keep) * 255.0) as u8]
    });
    // TextPrompt_GFX's box: (7, 30, 35) at its top and bottom, black between.
    let box_image = gradient(1, 64, &|t| {
        let top = (90.0 / 255.0 - t) / (90.0 / 255.0);
        let bottom = (t - 168.0 / 255.0) / (87.0 / 255.0);
        let k = top.max(bottom).clamp(0.0, 1.0);
        [(7.0 * k) as u8, (30.0 * k) as u8, (35.0 * k) as u8, 240]
    });
    // A prompt's focused button (PROMPT_BUTTON_ON): orange, deeper at its top.
    let button_on = gradient(1, 64, &|t| {
        let [top, bottom] = PROMPT_BUTTON_ON;
        let c = |i: usize| (top[i] + (bottom[i] - top[i]) * t).clamp(0.0, 255.0) as u8;
        [c(0), c(1), c(2), 255]
    });
    // White, fading out to its ends (its middle at 130 / 255).
    let band_image = gradient(64, 1, &|t| {
        let middle = 130.0 / 255.0;
        let k = if t < middle {
            t / middle
        } else {
            (1.0 - t) / (1.0 - middle)
        };
        [255, 255, 255, (k.clamp(0.0, 1.0) * 255.0) as u8]
    });

    // The Escalation lobby's clouds and their shade, over the background (and
    // under the settings menus, which open over them from the lobby).
    let clouds_image = img("lobby_clouds.png");
    let clouds = (0..CLOUD_COLUMNS.len() * CLOUD_ROWS.len())
        .map(|_| {
            [
                image_node(&mut commands, clouds_image.clone()),
                image_node(&mut commands, clouds_image.clone()),
            ]
        })
        .collect();
    let clouds_shade = image_node(&mut commands, shade_image);
    let clouds_below = image_node(&mut commands, Handle::default());
    let set_art = sliced_node(&mut commands, img("settings_background.png"), MENU_ART_GRID);
    let logo = image_node(&mut commands, img("logo.png"));

    let brackets_image = img("brackets.png");
    let mut labels = Vec::new();
    let mut brackets = Vec::new();
    let mut label_widths = Vec::new();
    let mut descriptions = Vec::new();
    for item in ITEMS {
        let (key, fallback, desc_key, desc_fallback) = item.text();
        let label = text.get(key, fallback).to_uppercase();
        label_widths.push(text.width(DEMI_BOLD, &label, LABEL_SIZE));
        descriptions.push(text.get(desc_key, desc_fallback));
        // Each side shows its corner of the 9-slice image (the middle is empty).
        let left = image_node(&mut commands, brackets_image.clone());
        let right = image_node(&mut commands, brackets_image.clone());
        brackets.push([left, right]);
        labels.push(text_node(&mut commands, label, &demi));
    }
    let summary_glow = (0..8)
        .map(|_| text_node(&mut commands, String::new(), &demi))
        .collect();
    let summary = text_node(&mut commands, String::new(), &demi);

    let hint = text.get("ButtonHints.Accept", "Select").to_uppercase();
    let hint_width = text.width(MEDIUM, &hint, HINT_SIZE);
    let hint_icon = image_node(&mut commands, img("button_a.png"));
    let hint_text = text_node(&mut commands, hint, &medium);
    let version_text = version_label();
    let version = text_node(&mut commands, version_text.clone(), &demi);

    // The settings menus.
    let set_bar = sliced_node(&mut commands, img("selection_bar.png"), SELECTION_BAR_GRID);
    let set_title = text_node(&mut commands, String::new(), &demi);
    let set_rows = (0..MAX_LABELS)
        .map(|_| text_node(&mut commands, String::new(), &demi))
        .collect();
    let set_value_glow =
        [(); MAX_STEPS].map(|_| [(); 8].map(|_| text_node(&mut commands, String::new(), &demi)));
    let set_values = [(); STEPPERS.len()]
        .map(|_| [(); MAX_STEPS].map(|_| text_node(&mut commands, String::new(), &demi)));
    let team_colors =
        [(); 2].map(|_| [(); 3].map(|_| text_node(&mut commands, String::new(), &demi)));
    let arrow = img("slider_arrow.png");
    let mut slider_labels: Vec<Entity> = (0..MAX_SLIDERS)
        .map(|_| text_node(&mut commands, String::new(), &demi))
        .collect();
    // (MenuSlider_track and MenuSlider_bar stretch with scale-9 grids: 6 pixels at
    // either end keep their size. The track is always 200 wide, so a sliced image
    // draws it; the bar's ends are images of their own.)
    let mut slider_part = |image: Handle<Image>, mode: NodeImageMode, rect: Option<Rect>| {
        commands
            .spawn((
                ImageNode {
                    image,
                    image_mode: mode,
                    rect,
                    ..default()
                },
                Node {
                    position_type: PositionType::Absolute,
                    ..default()
                },
                ChildOf(root),
            ))
            .id()
    };
    let track_mode = NodeImageMode::Sliced(TextureSlicer {
        border: BorderRect {
            min_inset: Vec2::new(SLIDER_END, 11.0),
            max_inset: Vec2::new(SLIDER_END, 11.0),
        },
        center_scale_mode: SliceScaleMode::Stretch,
        sides_scale_mode: SliceScaleMode::Stretch,
        max_corner_scale: 100.0,
    });
    let set_sliders = (0..MAX_SLIDERS)
        .map(|_| SliderUi {
            track: slider_part(img("slider_track.png"), track_mode.clone(), None),
            fill: [
                Rect::new(0.0, 0.0, SLIDER_END, 32.0),
                Rect::new(SLIDER_END, 0.0, 128.0 - SLIDER_END, 32.0),
                Rect::new(128.0 - SLIDER_END, 0.0, 128.0, 32.0),
            ]
            .map(|r| slider_part(img("slider_bar.png"), NodeImageMode::Stretch, Some(r))),
            arrows: [
                slider_part(arrow.clone(), NodeImageMode::Stretch, None),
                slider_part(arrow.clone(), NodeImageMode::Stretch, None),
            ],
            label: slider_labels.pop().unwrap_or(Entity::PLACEHOLDER),
        })
        .collect();
    // ($HeaderFont: Register Light.)
    let set_table = (0..MAX_ROWS - 2)
        .map(|_| {
            [
                text_node(&mut commands, String::new(), &light),
                text_node(&mut commands, String::new(), &light),
            ]
        })
        .collect();
    // The Controller Layout screen's controller map (padmap.txt).
    let pad_map = PadMap::load(&dir.join("padmap.txt"));
    let pad_text = |commands: &mut Commands| {
        let shadow = (0..GLOW_RING)
            .map(|_| text_node(commands, String::new(), &demi))
            .collect();
        PadTextUi {
            text: text_node(commands, String::new(), &demi),
            shadow,
        }
    };
    // (The controller under its labels: their lines run over it to its buttons.)
    let pad_picture = image_node(
        &mut commands,
        pad_map
            .picture
            .as_ref()
            .map_or_else(Handle::default, |(image, _)| img(image)),
    );
    let mut pad_labels = Vec::new();
    for label in &pad_map.labels {
        let white = label.image.replace(".png", "_white.png");
        pad_labels.push(PadLabelUi {
            art: image_node(&mut commands, img(&label.image)),
            white: image_node(&mut commands, img(&white)),
            text: pad_text(&mut commands),
        });
    }
    let pad_map_ui = PadMapUi {
        picture: pad_picture,
        labels: pad_labels,
        none: pad_text(&mut commands),
    };
    // The Back button (a plain box, as the PC version shows it).
    let back = text.get("ButtonHints.Back", "Back");
    let back_width = text.width(MEDIUM, &back, BACK_TEXT_SIZE);
    let back_box = image_node(&mut commands, Handle::default());
    let back_text = text_node(&mut commands, back, &medium);

    // The Display screens (over the button hints, as their movies are): their
    // Accept beside Back, Brightness's panel, picture and slider, Safe Frame's
    // area, arrows and band, and either's directions.
    let accept = text.get("Settings.Accept", "Accept");
    let accept_width = text.width(MEDIUM, &accept, BACK_TEXT_SIZE);
    let accept_box = image_node(&mut commands, Handle::default());
    let accept_text = text_node(&mut commands, accept, &medium);
    let art = DisplayArt {
        picture: img("calibration.png"),
        shown: images.reserve_handle(),
        shown_gamma: None,
        arrow: img("safe_frame_arrow.png"),
        flash: images.reserve_handle(),
        flash_made: false,
    };
    let bar_image = img("brightness_bar.png");
    let brightness_arrow = img("brightness_arrow.png");
    let band_half = img("safe_frame_band.png");
    let display = DisplayUi {
        panel: sliced_node(
            &mut commands,
            img("brightness_panel.png"),
            BorderRect::axes(BRIGHTNESS_PANEL_EDGE, 0.0),
        ),
        picture: image_node(&mut commands, art.shown.clone()),
        track: sliced_node(
            &mut commands,
            img("brightness_track.png"),
            BorderRect::all(TRANS_TRACK_END),
        ),
        bar: [
            Rect::new(0.0, 0.0, TRANS_BAR_END, 32.0),
            Rect::new(TRANS_BAR_END, 0.0, 128.0 - TRANS_BAR_END, 32.0),
            Rect::new(128.0 - TRANS_BAR_END, 0.0, 128.0, 32.0),
        ]
        .map(|rect| {
            commands
                .spawn((
                    ImageNode {
                        image: bar_image.clone(),
                        rect: Some(rect),
                        ..default()
                    },
                    Node {
                        position_type: PositionType::Absolute,
                        ..default()
                    },
                    ChildOf(root),
                ))
                .id()
        }),
        arrows: [(); 2].map(|_| image_node(&mut commands, brightness_arrow.clone())),
        // (Its scale-9 grid keeps the glow's 56 pixels at each edge, drawn at
        // twice their size: see layout_display.)
        area: sliced_node(
            &mut commands,
            img("safe_frame_area.png"),
            BorderRect::all(SAFE_AREA_EDGE / 2.0),
        ),
        area_arrows: [(); 4].map(|_| image_node(&mut commands, art.arrow.clone())),
        area_flashes: [(); 4].map(|_| image_node(&mut commands, art.flash.clone())),
        band: [(); 2].map(|_| image_node(&mut commands, band_half.clone())),
        directions: text_node(&mut commands, String::new(), &demi),
        accept_box,
        accept_text,
        accept_width,
        team_colors,
    };
    commands.insert_resource(art);

    // The pause menu (drawn over the game).
    let pause = PauseUi {
        hexagons: PAUSE_HEX_QUARTERS.map(|(_, flip_x, flip_y)| {
            let quarter = image_node(&mut commands, img("pause_hexagons.png"));
            commands.entity(quarter).insert(ImageNode {
                image: img("pause_hexagons.png"),
                image_mode: NodeImageMode::Stretch,
                flip_x,
                flip_y,
                ..default()
            });
            quarter
        }),
        glow: image_node(&mut commands, img("pause_emblem_glow.png")),
        emblem: image_node(&mut commands, img("pause_emblem.png")),
        logo: image_node(&mut commands, img("pause_logo.png")),
        caption: text_node(
            &mut commands,
            "CYBERTRON DEFENSE NETWORK".to_string(),
            &demi,
        ),
        pips: (0..PAUSE_PIPS.len())
            .map(|_| image_node(&mut commands, img("pause_pip.png")))
            .collect(),
        art: sliced_node(&mut commands, img("settings_background.png"), MENU_ART_GRID),
        bar: sliced_node(&mut commands, img("selection_bar.png"), SELECTION_BAR_GRID),
        title: text_node(&mut commands, String::new(), &demi),
        // (One for each item either menu has, as the host's lists them.)
        rows: HOST_PAUSE_ITEMS
            .iter()
            .map(|item| {
                let (key, fallback) = item.label();
                text_node(&mut commands, text.get(key, fallback), &demi)
            })
            .collect(),
    };

    // The Extras menu. Its selection bar is a 9-slice (the bitmap's scale9Grid).
    let extras = ExtrasUi {
        bar: commands
            .spawn((
                ImageNode {
                    image: img("extras_selection_bar.png"),
                    image_mode: NodeImageMode::Sliced(TextureSlicer {
                        border: BorderRect {
                            min_inset: Vec2::new(10.0, 13.0),
                            max_inset: Vec2::new(6.0, 13.0),
                        },
                        center_scale_mode: SliceScaleMode::Stretch,
                        sides_scale_mode: SliceScaleMode::Stretch,
                        // (The corners follow the stage's scale.)
                        max_corner_scale: 100.0,
                    }),
                    ..default()
                },
                Node {
                    position_type: PositionType::Absolute,
                    ..default()
                },
                ChildOf(root),
            ))
            .id(),
        title: text_node(
            &mut commands,
            text.get("Extras.ExtrasTitle", "Extras").to_uppercase(),
            &light,
        ),
        hint: text_node(&mut commands, String::new(), &demi),
        rows: EXTRAS_ITEMS
            .iter()
            .map(|item| {
                let (key, fallback, _, _) = item.text();
                text_node(&mut commands, text.get(key, fallback), &light)
            })
            .collect(),
    };
    // The Escalation lobby, and the prompts over it (and over the menus). The
    // ticker and the text input clip what goes on in them.
    let clip_node = |commands: &mut Commands| {
        commands
            .spawn((
                Node {
                    position_type: PositionType::Absolute,
                    overflow: Overflow::clip(),
                    ..default()
                },
                ChildOf(root),
            ))
            .id()
    };
    let child_text = |commands: &mut Commands, parent: Entity, font: &Handle<Font>| {
        commands
            .spawn((
                Text::new(String::new()),
                TextFont {
                    font: font.clone(),
                    font_size: 16.0,
                    ..default()
                },
                TextColor(Color::WHITE),
                TextLayout::new_with_no_wrap(),
                Node {
                    position_type: PositionType::Absolute,
                    ..default()
                },
                ChildOf(parent),
            ))
            .id()
    };
    let child_image = |commands: &mut Commands, parent: Entity| {
        commands
            .spawn((
                ImageNode {
                    image: Handle::default(),
                    image_mode: NodeImageMode::Stretch,
                    ..default()
                },
                Node {
                    position_type: PositionType::Absolute,
                    ..default()
                },
                ChildOf(parent),
            ))
            .id()
    };
    let texts = |commands: &mut Commands, n: usize, font: &Handle<Font>| -> Vec<Entity> {
        (0..n)
            .map(|_| text_node(commands, String::new(), font))
            .collect()
    };

    let panels = image_node(&mut commands, img("lobby_panels.png"));
    let mode_icon = image_node(&mut commands, img("lobby_mode_icon.png"));
    let title_glow = texts(&mut commands, 8, &demi);
    let lobby_title = text_node(&mut commands, String::new(), &demi);
    let subtitle = text_node(&mut commands, String::new(), &demi);
    let ticker_box = clip_node(&mut commands);
    let ticker = (0..4)
        .map(|_| child_text(&mut commands, ticker_box, &demi))
        .collect();
    let map_back = image_node(&mut commands, Handle::default());
    // Each map's picture (shown for the map picked).
    let map_pictures: Vec<Handle<Image>> = crate::MAPS.iter().map(|m| img(m.picture)).collect();
    let map_picture = image_node(&mut commands, map_pictures[0].clone());
    let map_shade = image_node(&mut commands, img("lobby_map_shade.png"));
    let map_frame = image_node(&mut commands, img("lobby_map_frame.png"));
    let map_label_glow = texts(&mut commands, 8, &medium);
    let map_label = text_node(&mut commands, String::new(), &medium);
    // The selection bar, a 9-slice as the Extras menu's (the same image).
    let bar_images = [img("lobby_bar.png"), img("lobby_bar_selector.png")];
    let lobby_bar = commands
        .spawn((
            ImageNode {
                image: bar_images[0].clone(),
                image_mode: NodeImageMode::Sliced(TextureSlicer {
                    border: BorderRect {
                        min_inset: Vec2::new(10.0, 13.0),
                        max_inset: Vec2::new(6.0, 13.0),
                    },
                    center_scale_mode: SliceScaleMode::Stretch,
                    sides_scale_mode: SliceScaleMode::Stretch,
                    max_corner_scale: 100.0,
                }),
                ..default()
            },
            Node {
                position_type: PositionType::Absolute,
                ..default()
            },
            ChildOf(root),
        ))
        .id();
    let lobby_rows = texts(&mut commands, LOBBY_MAX_ROWS, &light);
    let lobby_values = texts(&mut commands, LOBBY_MAX_ROWS, &light);
    let arrow_image = img("lobby_arrow.png");
    let lobby_arrows = (0..LOBBY_MAX_ROWS)
        .map(|_| {
            [
                image_node(&mut commands, arrow_image.clone()),
                image_node(&mut commands, arrow_image.clone()),
            ]
        })
        .collect();
    let lobby_hint = text_node(&mut commands, String::new(), &demi);
    // (Two rings of glow: the Decepticons' has two, LIST_DECEPTICON_GLOW.)
    let header_glow = texts(&mut commands, 2 * GLOW_RING, &demi);
    let header = text_node(&mut commands, String::new(), &demi);
    let columns = [
        text_node(&mut commands, String::new(), &demi),
        text_node(&mut commands, String::new(), &demi),
    ];
    let names = texts(&mut commands, crate::MAX_PLAYERS, &demi);
    let scores = texts(&mut commands, crate::MAX_PLAYERS, &demi);
    let ping_images = [
        img("lobby_ping_1.png"),
        img("lobby_ping_2.png"),
        img("lobby_ping_3.png"),
    ];
    let pings = (0..crate::MAX_PLAYERS)
        .map(|_| image_node(&mut commands, ping_images[2].clone()))
        .collect();
    let (star, ring) = (img("lobby_prime_star.png"), img("lobby_prime_ring.png"));
    let medals = (0..crate::MAX_PLAYERS)
        .map(|_| {
            [
                image_node(&mut commands, ring.clone()),
                image_node(&mut commands, star.clone()),
            ]
        })
        .collect();
    let lock = image_node(&mut commands, img("lobby_lock.png"));
    let status = text_node(&mut commands, String::new(), &demi);
    let countdown_label = text_node(&mut commands, String::new(), &demi);
    let countdown_number = text_node(&mut commands, String::new(), &demi);
    let band = image_node(&mut commands, band_image);
    let spinner = image_node(&mut commands, img("lobby_spinner.png"));
    let connecting = text_node(&mut commands, String::new(), &light);
    let lobby_back = text.get("ButtonHints.Back", "Back");
    let lobby_back_width = text.width(MEDIUM, &lobby_back, BACK_TEXT_SIZE);
    let lobby_back_box = image_node(&mut commands, Handle::default());
    let lobby_back_text = text_node(&mut commands, lobby_back, &medium);
    let lobby = LobbyUi {
        clouds,
        clouds_shade,
        clouds_below,
        panels,
        mode_icon,
        title_glow,
        title: lobby_title,
        subtitle,
        ticker_box,
        ticker,
        map_back,
        map_picture,
        map_pictures,
        map_shade,
        map_frame,
        map_label_glow,
        map_label,
        bar: lobby_bar,
        bar_images,
        rows: lobby_rows,
        values: lobby_values,
        arrows: lobby_arrows,
        arrow_image,
        hint: lobby_hint,
        header_glow,
        header,
        columns,
        names,
        scores,
        pings,
        ping_images,
        medals,
        lock,
        status,
        countdown_label,
        countdown_number,
        band,
        spinner,
        connecting,
        back_box: lobby_back_box,
        back_text: lobby_back_text,
        back_width: lobby_back_width,
    };

    let dim = image_node(&mut commands, dim_image);
    let box_fill = image_node(&mut commands, box_image);
    let bars = [
        image_node(&mut commands, Handle::default()),
        image_node(&mut commands, Handle::default()),
    ];
    let icon_images = [img("prompt_icon.png"), img("prompt_alert.png")];
    let icon = image_node(&mut commands, icon_images[0].clone());
    let join_title = text_node(&mut commands, String::new(), &medium);
    let message_title = text_node(&mut commands, String::new(), &demi);
    let body = text_node(&mut commands, String::new(), &demi);
    let input_border = image_node(&mut commands, Handle::default());
    let input_fill = image_node(&mut commands, Handle::default());
    let input_strip = image_node(&mut commands, Handle::default());
    let input_clip = clip_node(&mut commands);
    let input_text = child_text(&mut commands, input_clip, &demi);
    let caret = child_image(&mut commands, input_clip);
    let buttons = (0..2)
        .map(|_| {
            [
                image_node(&mut commands, Handle::default()),
                text_node(&mut commands, String::new(), &medium),
                image_node(&mut commands, Handle::default()),
            ]
        })
        .collect();
    let prompt = PromptUi {
        dim,
        box_fill,
        bars,
        icon,
        icon_images,
        join_title,
        message_title,
        body,
        input_border,
        input_fill,
        input_strip,
        input_clip,
        input_text,
        caret,
        buttons,
        button_on,
    };

    // The credits, over everything but the fader.
    let credits_back = commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                ..default()
            },
            BackgroundColor(Color::NONE),
            ChildOf(root),
        ))
        .id();
    let credits_image = image_node(&mut commands, Handle::default());

    let fader = commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                ..default()
            },
            BackgroundColor(Color::NONE),
            ChildOf(root),
        ))
        .id();

    commands.insert_resource(MenuUi {
        root,
        background,
        logo,
        labels,
        brackets,
        label_widths,
        descriptions,
        summary,
        summary_glow,
        hint_icon,
        hint_text,
        hint_width,
        version,
        version_text,
        set_art,
        set_bar,
        set_title,
        set_rows,
        set_values,
        set_value_glow,
        set_sliders,
        set_table,
        pad_map: pad_map_ui,
        back_box,
        back_text,
        back_width,
        display,
        pause,
        extras,
        lobby,
        prompt,
        credits_back,
        credits_image,
        fader,
    });
    commands.insert_resource(text);
    let credits_path = dir.join("credits.h264");
    let mut state = MenuState::new(now);
    state.pad_map = pad_map;
    state.credits_ready = credits_path.exists();
    if !state.credits_ready {
        warn!(
            "No credits movie in {:?}: Extras > Credits does nothing",
            dir
        );
    }
    if let Some(notice) = set_aside_notice {
        state.open_prompt(now, notice);
    }
    commands.insert_resource(state);
    commands.insert_resource(MenuMusic::default());
    commands.insert_resource(Credits {
        path: credits_path,
        audio: asset_server.load("menu/credits.ogg"),
        frames: None,
        image: None,
        started: 0.0,
        shown: 0,
        sound: None,
    });

    // The background movie, decoded on its own thread. Two decoded frames are
    // kept ready; the thread waits while they are.
    let movie_path = dir.join("background.h264");
    let movie_darken = Arc::new(AtomicU32::new((-1.0f32).to_bits()));
    let frames = start_movie(&movie_path, true, Some(movie_darken.clone()));
    if frames.is_none() {
        warn!("No menu movie in {:?}: showing a still", dir);
    }
    commands.insert_resource(Movie {
        frames,
        darken: movie_darken,
        image: None,
        next_at: now,
        paused: false,
        still,
        path: movie_path,
    });
}

/// Starts decoding a movie on its own thread (two decoded frames are kept ready;
/// the thread waits while they are), if it is there: over and over (`looping`) or
/// once (the channel closes at its end). With `darken`, frames are darkened as it
/// says when they are decoded (off the main thread).
fn start_movie(
    path: &std::path::Path,
    looping: bool,
    darken: Option<Arc<AtomicU32>>,
) -> Option<Frames> {
    if !path.exists() {
        return None;
    }
    let (tx, rx) = sync_channel(2);
    let path = path.to_path_buf();
    std::thread::spawn(move || decode_movie(path, tx, looping, darken));
    Some(Mutex::new(rx))
}

/// Decodes the H.264 movie (over and over if `looping`), sending each frame as
/// RGBA.
fn decode_movie(
    path: PathBuf,
    tx: SyncSender<Frame>,
    looping: bool,
    darken_by: Option<Arc<AtomicU32>>,
) {
    use openh264::decoder::Decoder;
    use openh264::formats::YUVSource;
    use openh264::nal_units;

    let Ok(data) = std::fs::read(&path) else {
        return;
    };
    loop {
        let Ok(mut decoder) = Decoder::new() else {
            return;
        };
        let mut decoded = false;
        for packet in nal_units(&data) {
            let Ok(Some(yuv)) = decoder.decode(packet) else {
                continue;
            };
            let (w, h) = yuv.dimensions();
            let mut rgba = vec![0u8; w * h * 4];
            yuv.write_rgba8(&mut rgba);
            decoded = true;
            let again = darken_by
                .as_ref()
                .map(|a| f32::from_bits(a.load(Ordering::Relaxed)))
                .filter(|a| *a >= 0.0);
            if let Some(again) = again {
                darken(&mut rgba, w, h, again);
            }
            let frame = Frame {
                w: w as u32,
                h: h as u32,
                rgba,
                darkened: again.is_some(),
            };
            if tx.send(frame).is_err() {
                return; // the menu has closed
            }
        }
        if !decoded || !looping {
            return;
        }
    }
}

// ---------------------------------------------------------------------------
// Input
// ---------------------------------------------------------------------------

/// The stage's scale and offset in the window (logical pixels).
#[derive(Clone, Copy)]
struct View {
    scale: f32,
    offset: Vec2,
    window: Vec2,
}

impl View {
    fn new(window: &Window) -> Self {
        let size = Vec2::new(window.width(), window.height()).max(Vec2::ONE);
        let scale = (size.x / STAGE_W).min(size.y / STAGE_H);
        View {
            scale,
            offset: (size - Vec2::new(STAGE_W, STAGE_H) * scale) * 0.5,
            window: size,
        }
    }

    fn to_stage(&self, p: Vec2) -> Vec2 {
        (p - self.offset) / self.scale
    }
}

/// Where button `i` sits (its origin, in stage pixels) without its slide.
fn button_origin(state: &MenuState, now: f32, i: usize) -> Vec2 {
    Vec2::new(SHELL_X + BUTTON_X, state.shell_y.get(now) + SLOTS[i])
}

/// The settings row `i` of the menu showing: its top (stage pixels).
fn row_top(state: &MenuState, now: f32, i: usize) -> f32 {
    SET_ANCHOR.y + state.drift.get(now) + state.row_y(i)
}

/// A slider's track and arrows, on the row whose top is at `y`.
fn slider_rects(y: f32) -> (Rect, Rect, Rect) {
    let track = Rect::new(
        SET_ANCHOR.x + SLIDER_X,
        y - 1.0,
        SET_ANCHOR.x + SLIDER_X + SLIDER_W,
        y - 1.0 + SLIDER_H,
    );
    let dec = Rect::new(
        SET_ANCHOR.x + 310.0 - ARROW,
        y + 2.0,
        SET_ANCHOR.x + 310.0,
        y + 2.0 + ARROW,
    );
    let inc = Rect::new(
        SET_ANCHOR.x + 486.0,
        y + 2.0,
        SET_ANCHOR.x + 486.0 + ARROW,
        y + 2.0 + ARROW,
    );
    (track, dec, inc)
}

#[allow(clippy::too_many_arguments)]
fn menu_input(
    time: Res<Time>,
    keys: Res<ButtonInput<KeyCode>>,
    mouse: Res<ButtonInput<MouseButton>>,
    scroll: Res<AccumulatedMouseScroll>,
    (gamepads, pads): (Query<&Gamepad>, Query<(Entity, &Gamepad)>),
    window: Single<&Window, With<PrimaryWindow>>,
    ui: Option<Res<MenuUi>>,
    mut state: Option<ResMut<MenuState>>,
    text: Option<Res<MenuText>>,
    mut settings: Settings,
    mut sfx: ResMut<Sfx>,
    (mut net, mut split): (ResMut<Net>, ResMut<crate::split::Split>),
    mut key_events: MessageReader<KeyboardInput>,
    mut next: ResMut<NextState<AppState>>,
    mut exit: MessageWriter<AppExit>,
    mut link: ResMut<CustomizeLink>,
) {
    // (Read every frame, so that a prompt sees only what is typed while it is up.)
    let typed: Vec<KeyboardInput> = key_events.read().cloned().collect();
    let (Some(ui), Some(state), Some(text)) = (ui, state.as_mut(), text) else {
        return;
    };
    let text: &MenuText = &text;
    let now = time.elapsed_secs();

    // Choices made a moment ago.
    if let Some(at) = state.quit_at
        && now >= at
    {
        exit.write(AppExit::Success);
        state.quit_at = None;
    }
    if let Some((item, at)) = state.leaving
        && now - at >= LEAVE_SECS
    {
        match item {
            Item::Settings => state.open_settings(now, text, settings.choices(Mode::Robot)),
            Item::Extras => state.open_extras(now),
            Item::Escalation => state.open_lobby(now, &net),
            Item::Customize => {
                // The menu is black now: the Customize screen opens under it.
                state.page = Page::Customize;
                state.leaving = None;
                link.closed = false;
                link.open = true;
            }
            _ => {}
        }
    }
    lobby_net(state, now, text, &mut net, &mut sfx);
    let pad_list: Vec<(Entity, &Gamepad)> = pads.iter().collect();
    party_guest(state, &mut split, &pad_list, &settings.controls, &mut sfx);
    lobby_guest(
        state,
        &mut net,
        &mut split,
        &pad_list,
        &settings.controls,
        &mut sfx,
    );
    if let Some(at) = state.game_at
        && now >= at
    {
        next.set(AppState::Game);
        return;
    }

    // (Split screen's guest's controller isn't the menus', nor Player 2's in the
    // party: in the lobbies its Start and B bring him in and take him out,
    // party_guest and lobby_guest.)
    let nav_pads: Vec<&Gamepad> = pads
        .iter()
        .filter(|(e, _)| !crate::split::is_guest_pad(&split, *e) && split.party != Some(*e))
        .map(|(_, g)| g)
        .collect();
    let nav = read_nav(state, &keys, &mouse, &nav_pads, &window, &scroll);

    if state.leaving.is_some() || state.game_at.is_some() || state.quit_at.is_some() {
        return;
    }
    // A prompt takes the input while it is up.
    if state.prompt.is_some() {
        prompt_input(
            state, now, &nav, &keys, &mouse, &gamepads, &typed, text, &settings, &mut net, &mut sfx,
        );
        return;
    }

    match state.page {
        Page::Main => {
            // The mouse focuses the button under it (CLIK's rollOver).
            let mut clicked = None;
            if let Some(p) = nav.cursor {
                for i in 0..ITEMS.len() {
                    let o = button_origin(state, now, i);
                    let w = ui.label_widths[i];
                    let hit = Rect::new(
                        o.x + LABEL_RIGHT - w - 20.0,
                        o.y + 2.0,
                        o.x + LABEL_RIGHT + 15.0,
                        o.y + 41.0,
                    );
                    if hit.contains(p) {
                        if nav.moved {
                            state.set_focus(now, i, false);
                        }
                        if mouse.just_released(MouseButton::Left) && state.focus == i {
                            clicked = Some(i);
                        }
                    }
                }
            }
            if nav.up && state.focus > 0 {
                let f = state.focus - 1;
                state.set_focus(now, f, false);
                sfx.play(SND_SCROLL_UP);
            }
            if nav.down && state.focus + 1 < ITEMS.len() {
                let f = state.focus + 1;
                state.set_focus(now, f, false);
                sfx.play(SND_SCROLL_DOWN);
            }
            let chosen = clicked.or(nav.accept.then_some(state.focus));
            if let Some(i) = chosen {
                match ITEMS[i] {
                    Item::Escalation | Item::Settings | Item::Extras => {
                        state.leave_main(now, ITEMS[i], &mut sfx);
                    }
                    // The menu leaves as it does for the others, fading to black.
                    Item::Customize if link.available => {
                        state.leave_main(now, ITEMS[i], &mut sfx);
                        state.fade.to(now, 1.0, LEAVE_SECS, Ease::RegularIn);
                        state
                            .music_level
                            .to(now, CUSTOMIZE_MUSIC_LEVEL, LEAVE_SECS, Ease::Linear);
                    }
                    // exitGame: the question first (Warning, with Continue and
                    // Cancel); Continue quits (prompt_input).
                    Item::Exit => {
                        sfx.play(SND_SUBMENU_ENTER);
                        state.open_prompt(
                            now,
                            Prompt::Confirm {
                                title: text
                                    .get("MessagePrompts.GenericWarningTitle", EXIT_GAME_TITLE),
                                text: text.get("MessagePrompts.ExitGameMessage", EXIT_GAME_TEXT),
                                action: ConfirmAction::ExitGame,
                            },
                        );
                    }
                    // (Without its data: tools/build_customize.py.)
                    Item::Customize => {}
                }
            }
        }
        Page::Settings => {
            if settings_input(
                state,
                now,
                &nav,
                &keys,
                &mouse,
                &gamepads,
                text,
                &mut settings,
                &mut sfx,
            ) {
                if state.settings_from_lobby {
                    state.back_to_lobby();
                } else {
                    state.enter_main(now, &mut sfx);
                }
            }
        }
        Page::Lobby => lobby_input(
            state,
            now,
            &nav,
            &mouse,
            text,
            &settings,
            &mut net,
            &mut sfx,
            link.available,
        ),
        Page::Extras => match extras_input(state, now, &nav, &mouse, &mut sfx) {
            Some(ExtrasItem::Credits) if state.credits_ready => {
                // StartExtrasMovie: the credits play (menu_credits).
                state.page = Page::Credits;
            }
            _ => {}
        },
        // The credits take their own input (menu_credits).
        Page::Credits | Page::Game | Page::Pause | Page::Choose => {}
        // The Customize screen takes the input; the menu's black stays until it
        // shows, and comes back when it has closed (faded to black itself).
        Page::Customize => {
            if link.closed {
                link.closed = false;
                if std::mem::take(&mut state.customize_from_lobby) {
                    state.back_to_lobby();
                } else {
                    state.enter_main(now, &mut sfx);
                }
                state
                    .fade
                    .from_to(now, 1.0, 0.0, LEAVE_SECS, Ease::RegularOut);
                state.music_level.to(now, 1.0, LEAVE_SECS, Ease::Linear);
            } else {
                if link.showing {
                    state.fade = Tween::at(0.0);
                }
                // (Opened from the lobby: the match's countdown starting, the
                // match itself, or the game gone, and it closes, for the lobby to
                // show it: lobby_net.)
                if state.customize_from_lobby
                    && (net.countdown().is_some() || net.in_match() || net.failed())
                {
                    link.close = true;
                }
            }
        }
    }
}

/// The Extras menu's input: the mouse or Up / Down move the focus, a click or
/// Enter presses the focused button (returned), Back or Esc closes the menu (back
/// to the main menu).
fn extras_input(
    state: &mut MenuState,
    now: f32,
    nav: &Nav,
    mouse: &ButtonInput<MouseButton>,
    sfx: &mut Sfx,
) -> Option<ExtrasItem> {
    let cursor = nav.cursor;
    let back_rect = Rect::from_center_size(BACK_CENTER, BACK_SIZE);
    state.back_over = cursor.is_some_and(|p| back_rect.contains(p));
    let back_clicked = state.back_over && mouse.just_released(MouseButton::Left);
    if nav.back || back_clicked {
        // closeMenu (CloseExtrasMenu): the menu goes and the main menu comes back.
        sfx.play(SND_SUBMENU_EXIT);
        state.enter_main(now, sfx);
        return None;
    }
    let mut clicked = None;
    if let Some(p) = cursor {
        for i in 0..EXTRAS_ITEMS.len() {
            let top = extras_row_top(i);
            let hit = Rect::new(
                EXTRAS_MENU.x,
                top,
                EXTRAS_MENU.x + EXTRAS_ROW_W,
                top + EXTRAS_PANEL_H,
            );
            if hit.contains(p) {
                if nav.moved {
                    state.focus_extras_row(now, i, sfx);
                }
                if mouse.just_released(MouseButton::Left) && state.extras_focus == i {
                    clicked = Some(i);
                }
            }
        }
    }
    if nav.up && state.extras_focus > 0 {
        let f = state.extras_focus - 1;
        state.focus_extras_row(now, f, sfx);
    }
    if nav.down && state.extras_focus + 1 < EXTRAS_ITEMS.len() {
        let f = state.extras_focus + 1;
        state.focus_extras_row(now, f, sfx);
    }
    let pressed = clicked.or(nav.accept.then_some(state.extras_focus))?;
    sfx.play(SND_BUTTON_ACCEPT);
    Some(EXTRAS_ITEMS[pressed])
}

/// Online play, for the menus: a failure is shown (over the party lobby, if the
/// lobby is up); joining opens the game's lobby once the host answers; the ticker
/// keeps up with what there is to say; each second of the countdown sounds; and
/// the match starting fades into the game.
fn lobby_net(state: &mut MenuState, now: f32, text: &MenuText, net: &mut Net, sfx: &mut Sfx) {
    // (The Customize screen opened from the lobby closes first: the menu's
    // Page::Customize.)
    if state.page == Page::Customize {
        return;
    }
    let in_lobby =
        state.page == Page::Lobby || (state.page == Page::Settings && state.settings_from_lobby);
    if let Some(failure) = net.take_failure() {
        if state.game_at.is_some() {
            // (The match was about to start.)
            state.game_at = None;
            state.fade.to(now, 0.0, LOBBY_FADE_SECS, Ease::RegularOut);
            state
                .music_level
                .to(now, 1.0, LOBBY_FADE_SECS, Ease::Linear);
        }
        if in_lobby {
            state.back_to_lobby();
            if state.lobby_view != LobbyView::Party {
                state.show_lobby(now, LobbyView::Party, net);
            }
        }
        // A game with a password asks for it (again, when the one given was wrong).
        if let (true, Failure::Password { wrong }) = (in_lobby, &failure) {
            if *wrong {
                sfx.play(SND_LOCKED);
            }
            state.password_entry.clear();
            state.open_prompt(now, Prompt::JoinPassword { wrong: *wrong });
            return;
        }
        let (title, message) = failure_message(&failure, text);
        state.open_prompt(
            now,
            Prompt::Message {
                title,
                text: message,
            },
        );
        sfx.play(SND_LOCKED);
        return;
    }
    if !in_lobby {
        return;
    }
    if state.page == Page::Lobby
        && state.lobby_view == LobbyView::Connecting
        && net.status() == NetStatus::Lobby
    {
        state.show_lobby(now, LobbyView::Game, net);
    }
    let second = net.countdown().map(|left| left.ceil() as u32);
    if second != state.countdown_second {
        if second.is_some_and(|s| s > 0) {
            sfx.play(SND_COUNTDOWN);
        }
        state.countdown_second = second;
        state.countdown_at = now;
    }
    // The match has started: fade into the game, as the front end does.
    if net.in_match() && state.game_at.is_none() {
        state.prompt = None;
        state.fade.to(now, 1.0, FADE_SECS, Ease::RegularOut);
        state.music_level.to(now, 0.0, FADE_SECS, Ease::Linear);
        state.game_at = Some(now + FADE_SECS);
    }
}

/// Split screen (split.rs) in the game's lobby, hosting: Player 2 in the party
/// (party_guest) comes in on his controller as it opens; else Start on a
/// controller other than the first player's (his: the first connected, while
/// Active Game Controls is on Controller) brings the second player in on it (in
/// the party too), unless someone online has joined (or the countdown has
/// started); B on his controller takes him out again (of the party too). Start
/// Game's hint says how.
fn lobby_guest(
    state: &mut MenuState,
    net: &mut Net,
    split: &mut crate::split::Split,
    pads: &[(Entity, &Gamepad)],
    controls: &Controls,
    sfx: &mut Sfx,
) {
    state.guest_hint = "";
    let open = state.page == Page::Lobby
        && state.lobby_view == LobbyView::Game
        && state.prompt.is_none()
        && state.leaving.is_none()
        && state.game_at.is_none()
        && net.is_host()
        && !net.in_match()
        && net.countdown().is_none();
    if !open {
        return;
    }
    let pad = |e: Entity| pads.iter().find(|(p, _)| *p == e).map(|(_, g)| *g);
    match split.guest {
        Some(guest) => {
            state.guest_hint = GUEST_LEAVE_HINT;
            let leaves = guest
                .gamepad
                .and_then(pad)
                .is_some_and(|g| g.just_pressed(GamepadButton::East));
            if leaves {
                net.remove_guest();
                split.guest = None;
                split.party = None;
                sfx.play(SND_BUTTON_BACK);
            }
        }
        None => {
            if net.player_count() != 1 {
                return;
            }
            // (Player 2 came with the first player from the party: in as the lobby
            // opens.)
            if let Some(party) = split.party
                && pad(party).is_some()
                && net.add_guest(GUEST_NAME).is_some()
            {
                split.guest = Some(crate::split::Guest {
                    gamepad: Some(party),
                });
                info!("Split screen: the second player came from the party on {party:?}");
                return;
            }
            let first = controls
                .controller
                .then(|| crate::split::first_pad(split, pads.iter().map(|(e, g)| (*e, *g))))
                .flatten()
                .map(|(e, _)| e);
            let mut joining = None;
            for (e, g) in pads.iter().filter(|(e, _)| Some(*e) != first) {
                state.guest_hint = GUEST_JOIN_HINT;
                if g.just_pressed(GamepadButton::Start) {
                    joining = Some(*e);
                    break;
                }
            }
            if let Some(pad) = joining
                && net.add_guest(GUEST_NAME).is_some()
            {
                split.guest = Some(crate::split::Guest { gamepad: Some(pad) });
                split.party = Some(pad);
                sfx.play(SND_BUTTON_ACCEPT);
                info!("Split screen: the second player joined on {pad:?}");
            }
        }
    }
}

/// Split screen (split.rs) in the party lobby: while a controller other than the
/// first player's (his: the first connected, while Active Game Controls is on
/// Controller) is connected, "Press START to join" under the first player's name;
/// Start on one brings Player 2 into the party on it (listed there in its place),
/// and he comes into the next game this PC hosts with the first player
/// (lobby_guest). B on his controller takes him out again. In a game of this PC's
/// he is its guest (on the controller he has there); his controller gone outside
/// one, or the party lobby left for the main menu, he leaves the party.
fn party_guest(
    state: &mut MenuState,
    split: &mut crate::split::Split,
    pads: &[(Entity, &Gamepad)],
    controls: &Controls,
    sfx: &mut Sfx,
) {
    state.party_hint = false;
    if let Some(guest) = split.guest {
        if guest.gamepad.is_some() {
            split.party = guest.gamepad;
        }
    } else if split
        .party
        .is_some_and(|p| !pads.iter().any(|(e, _)| *e == p))
        || state.page == Page::Main
    {
        split.party = None;
    }
    let open = state.page == Page::Lobby
        && state.lobby_view == LobbyView::Party
        && state.prompt.is_none()
        && state.leaving.is_none()
        && state.game_at.is_none();
    if open {
        match split.party {
            Some(party) => {
                let leaves = pads
                    .iter()
                    .find(|(e, _)| *e == party)
                    .is_some_and(|(_, g)| g.just_pressed(GamepadButton::East));
                if leaves {
                    split.party = None;
                    sfx.play(SND_BUTTON_BACK);
                    info!("Split screen: the second player left the party");
                }
            }
            None => {
                let first = controls
                    .controller
                    .then(|| crate::split::first_pad(split, pads.iter().map(|(e, g)| (*e, *g))))
                    .flatten()
                    .map(|(e, _)| e);
                for (e, g) in pads.iter().filter(|(e, _)| Some(*e) != first) {
                    state.party_hint = true;
                    if g.just_pressed(GamepadButton::Start) {
                        split.party = Some(*e);
                        state.party_hint = false;
                        sfx.play(SND_BUTTON_ACCEPT);
                        info!("Split screen: the second player joined the party on {e:?}");
                        break;
                    }
                }
            }
        }
    }
    state.party_guest = split.party.is_some();
}

/// Split screen's guest's name in the lobby and the match, and Start Game's hints.
const GUEST_NAME: &str = "Player 2";
/// Under the first player's name in the party lobby while Player 2 can join.
const PARTY_JOIN_PROMPT: &str = "Press START to join";
const GUEST_JOIN_HINT: &str = "Split screen: press START on another controller to play together.";
const GUEST_LEAVE_HINT: &str = "Split screen: Player 2 presses B to leave.";

/// The lobby's input (its menus, HmMenu's): the mouse or Up / Down move the focus
/// through the rows that can be used, a click or Enter presses the focused row,
/// Left / Right or a click on its arrows change the difficulty (hosting), and Back
/// or Esc goes back (from a game's lobby leaving the game; from the party lobby to
/// the main menu).
#[allow(clippy::too_many_arguments)]
fn lobby_input(
    state: &mut MenuState,
    now: f32,
    nav: &Nav,
    mouse: &ButtonInput<MouseButton>,
    text: &MenuText,
    settings: &Settings,
    net: &mut Net,
    sfx: &mut Sfx,
    customize_available: bool,
) {
    let view = state.lobby_view;
    state.lobby_back_over = nav.cursor.is_some_and(|p| LOBBY_BACK.contains(p));
    let back_clicked = state.lobby_back_over && mouse.just_released(MouseButton::Left);
    if nav.back || back_clicked {
        sfx.play(SND_BUTTON_BACK);
        if view == LobbyView::Party {
            // Back to the main menu, the clouds fading out.
            state.lobby_closed = now;
            state.enter_main(now, sfx);
        } else {
            net.leave();
            state.show_lobby(now, LobbyView::Party, net);
        }
        return;
    }
    let rows = lobby_rows(view, net.is_host(), state.prime);
    if rows.is_empty() {
        return;
    }
    let enabled = |i: usize, net: &Net| row_enabled(rows, i, net);
    // The focus leaves a row that can no longer be used (Start Game, as the
    // countdown starts) for the nearest that can.
    let at = state.lobby_focus.min(rows.len() - 1);
    if !enabled(at, &*net)
        && let Some(i) = (at..rows.len())
            .chain((0..at).rev())
            .find(|&i| enabled(i, &*net))
    {
        state.focus_lobby_row(now, i);
    }

    // The mouse focuses the row under it (and, clicked, presses it, or on a
    // selector steps it by its arrows).
    let mut clicked = false;
    let mut step = 0;
    if let Some(p) = nav.cursor {
        for i in 0..rows.len() {
            let top = lobby_row_top(view, i);
            let hit = Rect::new(
                LOBBY_MENU_X - 15.0,
                top - 2.0,
                LOBBY_MENU_X + LOBBY_ROW_W + 15.0,
                top + LOBBY_ROW_H - 2.0,
            );
            if !hit.contains(p) {
                continue;
            }
            let click = mouse.just_released(MouseButton::Left);
            if !enabled(i, &*net) {
                if click && rows[i].state(net) == RowState::Disabled {
                    sfx.play(SND_LOCKED);
                }
                continue;
            }
            if (nav.moved || click) && state.focus_lobby_row(now, i) {
                sfx.play(SND_BUTTON_UP);
            }
            if click {
                let x = p.x - LOBBY_MENU_X;
                if !rows[i].selector() {
                    clicked = true;
                } else if x < 6.0 {
                    step = -1;
                } else if x > LOBBY_SELECTOR_W - 10.0 {
                    step = 1;
                }
            }
        }
    }
    // Up and Down skip the rows that can't be used, going round from either end
    // (as the menus' focus links do).
    let focus = state.lobby_focus.min(rows.len() - 1);
    if nav.up
        && let Some(i) = (0..focus)
            .rev()
            .chain((focus + 1..rows.len()).rev())
            .find(|&i| enabled(i, &*net))
        && state.focus_lobby_row(now, i)
    {
        sfx.play(SND_BUTTON_UP);
    }
    if nav.down
        && let Some(i) = (focus + 1..rows.len())
            .chain(0..focus)
            .find(|&i| enabled(i, &*net))
        && state.focus_lobby_row(now, i)
    {
        sfx.play(SND_BUTTON_DOWN);
    }
    let focus = state.lobby_focus.min(rows.len() - 1);
    let row = rows[focus];
    if !enabled(focus, &*net) {
        return;
    }
    if row == LobbyRow::Difficulty {
        if nav.left {
            step = -1;
        } else if nav.right {
            step = 1;
        }
        let difficulty = net.difficulty() as i32 + step;
        if step != 0 && (0..=2).contains(&difficulty) {
            net.set_difficulty(difficulty as usize);
            sfx.play(SND_SELECTOR);
        }
        return;
    }
    if row == LobbyRow::SelectMap {
        if nav.left {
            step = -1;
        } else if nav.right {
            step = 1;
        }
        let (at, _) = map_place(net);
        if step != 0
            && let Some(&map) = at
                .checked_add_signed(step as isize)
                .and_then(|i| crate::built_maps().get(i))
        {
            net.set_map(map);
            sfx.play(SND_SELECTOR);
        }
        return;
    }
    if !(clicked || nav.accept) {
        return;
    }
    match row {
        // Offline Game: straight into a game's lobby of this PC's own (no password
        // to ask for: nobody online can join it).
        LobbyRow::OfflineGame => {
            sfx.play(SND_BUTTON_ACCEPT);
            net.host_offline();
            state.show_lobby(now, LobbyView::Game, net);
        }
        LobbyRow::JoinGame => {
            sfx.play(SND_BUTTON_ACCEPT);
            state.open_prompt(now, Prompt::Address);
        }
        // Customize: the lobby fades to black and the Customize screen opens, as
        // from the main menu (its music down to the same level); it closes back
        // here (the menu's Page::Customize).
        LobbyRow::Customize if customize_available => {
            sfx.play(SND_PRESS_IN);
            state.customize_from_lobby = true;
            state.leaving = Some((Item::Customize, now));
            state.fade.to(now, 1.0, LEAVE_SECS, Ease::RegularIn);
            state
                .music_level
                .to(now, CUSTOMIZE_MUSIC_LEVEL, LEAVE_SECS, Ease::Linear);
        }
        // (Without its data: tools/build_customize.py.)
        LobbyRow::Customize => sfx.play(SND_LOCKED),
        LobbyRow::DisplayName => {
            sfx.play(SND_BUTTON_ACCEPT);
            state.name_entry = net.name().to_string();
            state.open_prompt(now, Prompt::DisplayName);
        }
        LobbyRow::HostGame => {
            sfx.play(SND_BUTTON_ACCEPT);
            state.password_entry.clear();
            state.open_prompt(now, Prompt::HostPassword);
        }
        LobbyRow::StartGame => {
            sfx.play(SND_BUTTON_START);
            net.start_countdown();
        }
        LobbyRow::Settings => {
            sfx.play(SND_BUTTON_ACCEPT);
            state.settings_from_lobby = true;
            state.open_settings(now, text, settings.choices(Mode::Robot));
        }
        // Prime Mode's question (DisplayMessagePrompt: Confirm unlockPrimeMode,
        // Cancel). Locked, it does nothing (only says what it needs).
        LobbyRow::PrimeMode => {
            sfx.play(SND_BUTTON_ACCEPT);
            state.open_prompt(
                now,
                Prompt::Confirm {
                    title: text.get("Lobby.PrimeMode", "Prime Mode"),
                    text: text.get("MessagePrompts.PrimeModeMessage", PRIME_MODE_TEXT),
                    action: ConfirmAction::PrimeMode,
                },
            );
        }
        _ => {}
    }
}

/// Prime Mode (TnCharacterScriptBinding's IsPrimeModeAvailable and
/// UnlockPrimeMode): what the party lobby offers of it as xp.rs's progress stands
/// (open once every class is at the top level, gone once it is on), this PC's
/// player a Prime or not for the others (net.rs); and, its question confirmed, it
/// turned on (OnPrimeModeUnlocked: every class's experience cleared, every build
/// made anew by ResetCharacters), saved, the focus back on the menu's first row.
fn prime_mode(
    time: Res<Time>,
    mut state: Option<ResMut<MenuState>>,
    mut progress: ResMut<crate::xp::Progress>,
    data: Option<Res<crate::customize::CustData>>,
    mut net: ResMut<Net>,
) {
    if let Some(state) = state.as_mut() {
        if std::mem::take(&mut state.prime_confirmed) && progress.prime_mode_available() {
            progress.unlock_prime_mode();
            progress.save();
            if let Some(data) = data.as_deref() {
                crate::customize::reset_builds(data, &progress);
            }
            state.focus_lobby_row(time.elapsed_secs(), 0);
        }
        let offer = if progress.prime {
            PrimeOffer::Gone
        } else if progress.prime_mode_available() {
            PrimeOffer::Open
        } else {
            PrimeOffer::Locked
        };
        if state.prime != offer {
            state.prime = offer;
        }
    }
    net.set_prime(progress.prime);
}

/// Whether row `i` of `rows` can be used (or, locked, focused).
fn row_enabled(rows: &[LobbyRow], i: usize, net: &Net) -> bool {
    rows.get(i)
        .is_some_and(|row| matches!(row.state(net), RowState::Enabled | RowState::Locked))
}

/// A prompt's buttons this frame, as the game's (its CLIK buttons, the PC
/// version's): the focus starts on the first (open_prompt); Left and Right move
/// it (the arrow keys and the d-pad; A and D and the stick too, where nothing is
/// typed: `typing`), and the mouse moving over a button gives it the focus; Enter
/// or A presses the focused button (Space too, where nothing is typed), a click
/// the one under the mouse. Returns the button pressed.
fn prompt_buttons(
    state: &mut MenuState,
    nav: &Nav,
    (keys, mouse, gamepads): (
        &ButtonInput<KeyCode>,
        &ButtonInput<MouseButton>,
        &Query<&Gamepad>,
    ),
    layout: &PromptLayout,
    typing: bool,
    sfx: &mut Sfx,
) -> Option<usize> {
    let n = layout.buttons.len();
    if n == 0 {
        return None;
    }
    let over = nav
        .cursor
        .and_then(|p| layout.buttons.iter().position(|(r, _)| r.contains(p)));
    state.prompt_focus = state.prompt_focus.min(n - 1);
    if let Some(i) = over
        && nav.moved
        && i != state.prompt_focus
    {
        state.prompt_focus = i;
        sfx.play(SND_BUTTON_UP);
    }
    state.prompt_over = over;
    let pad = |button: GamepadButton| gamepads.iter().any(|pad| pad.just_pressed(button));
    let (left, right) = if typing {
        (
            keys.just_pressed(KeyCode::ArrowLeft) || pad(GamepadButton::DPadLeft),
            keys.just_pressed(KeyCode::ArrowRight)
                || pad(GamepadButton::DPadRight)
                || keys.just_pressed(KeyCode::Tab),
        )
    } else {
        (nav.left, nav.right || keys.just_pressed(KeyCode::Tab))
    };
    if left && state.prompt_focus > 0 {
        state.prompt_focus -= 1;
        sfx.play(SND_BUTTON_UP);
    } else if right && state.prompt_focus + 1 < n {
        state.prompt_focus += 1;
        sfx.play(SND_BUTTON_DOWN);
    }
    let pressed = if typing {
        keys.any_just_pressed([KeyCode::Enter, KeyCode::NumpadEnter]) || pad(GamepadButton::South)
    } else {
        nav.accept
    };
    over.filter(|_| mouse.just_released(MouseButton::Left))
        .or(pressed.then_some(state.prompt_focus))
}

/// A question's answer this frame (a Confirm prompt's): Yes (true) as its first
/// button is pressed (prompt_buttons), No as its second is, or on Esc, Backspace,
/// B, Start or a right click.
fn confirm_answer(
    state: &mut MenuState,
    nav: &Nav,
    input: (
        &ButtonInput<KeyCode>,
        &ButtonInput<MouseButton>,
        &Query<&Gamepad>,
    ),
    text: &MenuText,
    sfx: &mut Sfx,
) -> Option<bool> {
    let prompt = state.prompt.as_ref()?;
    let layout = prompt_layout(prompt, text);
    match prompt_buttons(state, nav, input, &layout, false, sfx) {
        Some(0) => Some(true),
        Some(_) => Some(false),
        None if nav.back || nav.start => Some(false),
        None => None,
    }
}

/// A prompt's input. Join Game's takes the address as it is typed (Backspace
/// rubbing out); Enter or Join joins, Esc or Cancel closes it. Display Name's takes
/// the name the same way; Enter or Ok makes it the player's name (and saves it),
/// Esc or Cancel keeps the old one. Game Password's takes the password the same
/// way: hosting, Enter or Ok hosts the game with it (none, left empty); joining,
/// Enter or Join joins with it; Esc or Cancel does neither. A message closes on
/// Enter, Esc or Ok. (A question is asked over the pause menu only: pause_input.)
#[allow(clippy::too_many_arguments)]
fn prompt_input(
    state: &mut MenuState,
    now: f32,
    nav: &Nav,
    keys: &ButtonInput<KeyCode>,
    mouse: &ButtonInput<MouseButton>,
    gamepads: &Query<&Gamepad>,
    typed: &[KeyboardInput],
    text: &MenuText,
    settings: &Settings,
    net: &mut Net,
    sfx: &mut Sfx,
) {
    let Some(prompt) = state.prompt.clone() else {
        return;
    };
    let layout = prompt_layout(&prompt, text);
    // (The button pressed: the first accepts, the second cancels. A question's
    // buttons are read by confirm_answer.)
    let pressed = if matches!(prompt, Prompt::Confirm { .. }) {
        None
    } else {
        prompt_buttons(
            state,
            nav,
            (keys, mouse, gamepads),
            &layout,
            prompt.typed(),
            sfx,
        )
    };
    let pad = |button: GamepadButton| gamepads.iter().any(|pad| pad.just_pressed(button));
    let escape = keys.just_pressed(KeyCode::Escape)
        || pad(GamepadButton::East)
        || mouse.just_pressed(MouseButton::Right);
    // Ctrl+V or Shift+Insert: the clipboard's first line, typed into the field
    // (clipboard.rs); the shortcut's V isn't typed.
    let pasted = crate::clipboard::paste_pressed(keys)
        .then(crate::clipboard::line)
        .flatten();
    let typed: Vec<&KeyboardInput> = typed
        .iter()
        .filter(|k| k.state == KeyState::Pressed && !crate::clipboard::is_paste_key(k, keys))
        .collect();
    let name_char = |c: &char| c.is_ascii_graphic() || *c == ' ';
    match prompt {
        Prompt::Address => {
            for key in &typed {
                if key.key_code == KeyCode::Backspace {
                    state.address.pop();
                } else if let Some(chars) = &key.text {
                    for c in chars.chars().filter(char::is_ascii_graphic) {
                        if state.address.len() < ADDRESS_MAX {
                            state.address.push(c);
                        }
                    }
                }
            }
            if let Some(line) = &pasted {
                crate::clipboard::paste_into(
                    &mut state.address,
                    line.trim(),
                    ADDRESS_MAX,
                    char::is_ascii_graphic,
                );
            }
            if escape || pressed == Some(1) {
                sfx.play(SND_BUTTON_BACK);
                state.prompt = None;
            } else if pressed == Some(0) {
                let address = state.address.trim().to_string();
                save_settings(
                    *settings.volumes,
                    *settings.fullscreen,
                    *settings.cap,
                    Some(&address),
                );
                match net.join(&address, "") {
                    Ok(()) => {
                        sfx.play(SND_BUTTON_ACCEPT);
                        state.prompt = None;
                        state.show_lobby(now, LobbyView::Connecting, net);
                    }
                    Err(failure) => {
                        sfx.play(SND_LOCKED);
                        let (title, message) = failure_message(&failure, text);
                        state.open_prompt(
                            now,
                            Prompt::Message {
                                title,
                                text: message,
                            },
                        );
                    }
                }
            }
        }
        Prompt::DisplayName => {
            for key in &typed {
                let chars = match (&key.text, key.key_code) {
                    (_, KeyCode::Backspace) => {
                        state.name_entry.pop();
                        continue;
                    }
                    (Some(chars), _) => chars.to_string(),
                    (None, KeyCode::Space) => " ".to_string(),
                    _ => continue,
                };
                for c in chars.chars().filter(name_char) {
                    if state.name_entry.len() < NAME_MAX {
                        state.name_entry.push(c);
                    }
                }
            }
            if let Some(line) = &pasted {
                crate::clipboard::paste_into(&mut state.name_entry, line, NAME_MAX, name_char);
            }
            if escape || pressed == Some(1) {
                sfx.play(SND_BUTTON_BACK);
                state.prompt = None;
            } else if pressed == Some(0) {
                sfx.play(SND_BUTTON_ACCEPT);
                state.prompt = None;
                // (Left empty, the name stays as it was.)
                if !state.name_entry.trim().is_empty() {
                    net.set_name(&state.name_entry);
                    save_display_name(net.name());
                }
            }
        }
        Prompt::HostPassword | Prompt::JoinPassword { .. } => {
            for key in &typed {
                let chars = match (&key.text, key.key_code) {
                    (_, KeyCode::Backspace) => {
                        state.password_entry.pop();
                        continue;
                    }
                    (Some(chars), _) => chars.to_string(),
                    (None, KeyCode::Space) => " ".to_string(),
                    _ => continue,
                };
                for c in chars.chars().filter(name_char) {
                    if state.password_entry.len() < PASSWORD_MAX {
                        state.password_entry.push(c);
                    }
                }
            }
            if let Some(line) = &pasted {
                crate::clipboard::paste_into(
                    &mut state.password_entry,
                    line,
                    PASSWORD_MAX,
                    name_char,
                );
            }
            if escape || pressed == Some(1) {
                sfx.play(SND_BUTTON_BACK);
                state.password_entry.clear();
                state.prompt = None;
            } else if pressed == Some(0) {
                let password = std::mem::take(&mut state.password_entry);
                let opened = if prompt == Prompt::HostPassword {
                    net.host(&password).map(|()| LobbyView::Game)
                } else {
                    net.join(state.address.trim(), &password)
                        .map(|()| LobbyView::Connecting)
                };
                match opened {
                    Ok(view) => {
                        sfx.play(SND_BUTTON_ACCEPT);
                        state.prompt = None;
                        state.show_lobby(now, view, net);
                    }
                    Err(failure) => {
                        sfx.play(SND_LOCKED);
                        let (title, message) = failure_message(&failure, text);
                        state.open_prompt(
                            now,
                            Prompt::Message {
                                title,
                                text: message,
                            },
                        );
                    }
                }
            }
        }
        // Prime Mode's question: Confirm turns it on (prime_mode), Cancel goes back.
        Prompt::Confirm {
            action: ConfirmAction::PrimeMode,
            ..
        } => match confirm_answer(state, nav, (keys, mouse, gamepads), text, sfx) {
            Some(true) => {
                sfx.play(SND_BUTTON_ACCEPT);
                state.prompt = None;
                state.prime_confirmed = true;
            }
            Some(false) => {
                sfx.play(SND_BUTTON_BACK);
                state.prompt = None;
            }
            None => {}
        },
        // Exit Game's question: Continue quits (the sound, then quit), Cancel goes
        // back to the menu.
        Prompt::Confirm {
            action: ConfirmAction::ExitGame,
            ..
        } => match confirm_answer(state, nav, (keys, mouse, gamepads), text, sfx) {
            Some(true) => {
                sfx.play(SND_PRESS_IN);
                state.prompt = None;
                state.quit_at = Some(now + 0.3);
            }
            Some(false) => {
                sfx.play(SND_BUTTON_BACK);
                state.prompt = None;
            }
            None => {}
        },
        // (A question left up as the game ended goes, whatever the answer.)
        Prompt::Message { .. } => {
            if escape || nav.back || pressed.is_some() {
                sfx.play(SND_BUTTON_ACCEPT);
                state.prompt = None;
            }
        }
        Prompt::Confirm { .. } => {
            if confirm_answer(state, nav, (keys, mouse, gamepads), text, sfx).is_some() {
                sfx.play(SND_BUTTON_ACCEPT);
                state.prompt = None;
            }
        }
    }
}

/// The ending credits (HmExternalInterface.Game.PlayMovie): the movie plays over
/// everything, with its sound, the menu music fading out under it; at its end, or
/// on a key, click or button press, it stops and the Extras menu has the input
/// again (MovieEnded).
#[allow(clippy::too_many_arguments)]
fn menu_credits(
    mut commands: Commands,
    time: Res<Time>,
    real: Res<Time<Real>>,
    keys: Res<ButtonInput<KeyCode>>,
    mut mouse: ResMut<ButtonInput<MouseButton>>,
    gamepads: Query<&Gamepad>,
    mut cursor: Single<&mut CursorOptions, With<PrimaryWindow>>,
    mut state: Option<ResMut<MenuState>>,
    mut credits: Option<ResMut<Credits>>,
    mut images: ResMut<Assets<Image>>,
    volumes: Res<Volumes>,
) {
    let (Some(state), Some(credits)) = (state.as_mut(), credits.as_mut()) else {
        return;
    };
    let state: &mut MenuState = state;
    let credits: &mut Credits = credits;
    let now = time.elapsed_secs();
    // The movie keeps real time, as its sound does (the game clock skips a long
    // stall, such as dragging the window).
    let clock = real.elapsed_secs();
    let playing = credits.frames.is_some();
    if state.page != Page::Credits {
        if playing {
            stop_credits(&mut commands, state, credits, &mut images, &mut cursor, now);
        }
        return;
    }
    if !playing {
        let Some(frames) = start_movie(&credits.path, false, None) else {
            state.page = Page::Extras;
            return;
        };
        credits.frames = Some(frames);
        credits.started = clock;
        credits.shown = 0;
        // (Its sound is the credits' music: it follows Music Volume.)
        let loudness = Loudness {
            gain: 1.0,
            class: SoundClass::Music,
        };
        credits.sound = Some(
            commands
                .spawn((
                    AudioPlayer::new(credits.audio.clone()),
                    PlaybackSettings::DESPAWN.with_volume(loudness.volume(&volumes)),
                    loudness,
                ))
                .id(),
        );
        state
            .music_level
            .to(now, 0.0, CREDITS_MUSIC_OUT_SECS, Ease::Linear);
        cursor.visible = false;
        return;
    }

    // Skipping.
    let skip = keys.any_just_pressed([
        KeyCode::Escape,
        KeyCode::Enter,
        KeyCode::NumpadEnter,
        KeyCode::Space,
        KeyCode::Backspace,
    ])
        // (A left click on its release, as the menu takes clicks: on its press the
        // release would go on to the Extras menu.)
        || mouse.just_released(MouseButton::Left)
        || mouse.just_pressed(MouseButton::Right)
        || gamepads.iter().any(|pad| {
            pad.any_just_pressed([
                GamepadButton::South,
                GamepadButton::East,
                GamepadButton::Start,
            ])
        });
    if skip && clock - credits.started > CREDITS_SKIP_GRACE {
        state.page = Page::Extras;
        stop_credits(&mut commands, state, credits, &mut images, &mut cursor, now);
        return;
    }

    // The frames due by now (the movie keeps time with its sound: when the decoder
    // falls behind, it shows the latest frame it has and catches up).
    let mut latest = None;
    let mut ended = false;
    if let Some(rx) = credits.frames.as_ref().and_then(|m| m.lock().ok()) {
        while credits.started + credits.shown as f32 / CREDITS_FPS <= clock {
            match rx.try_recv() {
                Ok(frame) => {
                    latest = Some(frame);
                    credits.shown += 1;
                }
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => {
                    ended = true;
                    break;
                }
            }
        }
    }
    if let Some(frame) = latest {
        show_frame(
            &mut images,
            &mut credits.image,
            frame.w,
            frame.h,
            frame.rgba,
        );
    }
    if ended {
        state.page = Page::Extras;
        stop_credits(&mut commands, state, credits, &mut images, &mut cursor, now);
        // A click held as they end isn't one for the Extras menu.
        mouse.reset(MouseButton::Left);
    }
}

/// The credits stop (ended or skipped): the decoder thread ends, the sound stops,
/// the music comes back.
fn stop_credits(
    commands: &mut Commands,
    state: &mut MenuState,
    credits: &mut Credits,
    images: &mut Assets<Image>,
    cursor: &mut CursorOptions,
    now: f32,
) {
    // (Dropping the channel ends the thread at its next frame.)
    credits.frames = None;
    if let Some(e) = credits.sound.take() {
        commands.entity(e).try_despawn();
    }
    if let Some(image) = credits.image.take() {
        images.remove(&image);
    }
    state
        .music_level
        .to(now, 1.0, CREDITS_MUSIC_IN_SECS, Ease::Linear);
    cursor.visible = true;
}

/// Puts a decoded frame into `slot`'s image (made, or remade at a new size, as
/// needed).
fn show_frame(
    images: &mut Assets<Image>,
    slot: &mut Option<Handle<Image>>,
    w: u32,
    h: u32,
    rgba: Vec<u8>,
) {
    let reuse = slot
        .as_ref()
        .and_then(|handle| images.get_mut(handle))
        .filter(|img| img.width() == w && img.height() == h);
    if let Some(image) = reuse {
        image.data = Some(rgba);
        return;
    }
    let mut image = Image::new_fill(
        Extent3d {
            width: w,
            height: h,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        &[0, 0, 0, 255],
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::default(),
    );
    image.data = Some(rgba);
    *slot = Some(images.add(image));
}

/// The pause menu, in the game: Esc (or Start) opens it over the running game
/// (TnPlayerController's pause: the hologram sweeps in, the HUD goes, the mouse is
/// free); from it Resume Game (or Esc) goes back, Settings opens the settings menus
/// and Quit Game asks whether to quit (TnQuitMessageBox: the match's experience is
/// lost), then fades back to the main menu. The host's has Return to Lobby too,
/// which asks likewise, then ends the match: every player fades back to the game's
/// lobby, still in the game. Runs before the game's systems, which take no orders
/// while it is up.
#[allow(clippy::too_many_arguments)]
pub fn pause_input(
    time: Res<Time>,
    keys: Res<ButtonInput<KeyCode>>,
    mouse: Res<ButtonInput<MouseButton>>,
    scroll: Res<AccumulatedMouseScroll>,
    gamepads: Query<&Gamepad>,
    window: Single<&Window, With<PrimaryWindow>>,
    mut cursor: Single<&mut CursorOptions, With<PrimaryWindow>>,
    mut state: Option<ResMut<MenuState>>,
    text: Option<Res<MenuText>>,
    mut settings: Settings,
    mut sfx: ResMut<Sfx>,
    mut net: ResMut<Net>,
    mut pause: ResMut<Pause>,
    mut effect: ResMut<crate::hologram::PauseEffect>,
    mut next: ResMut<NextState<AppState>>,
    (mut choose, team, split): (
        ResMut<crate::choose::ChooseLink>,
        Res<crate::revive::Team>,
        Res<crate::split::Split>,
    ),
) {
    let (Some(state), Some(text)) = (state.as_mut(), text) else {
        return;
    };
    let text: &MenuText = &text;
    let now = time.elapsed_secs();

    // Leaving the match (Quit Game, or it ended): once the screen is black, back to
    // the menus (the main menu, or the game's lobby).
    if let Some(at) = state.quit_game_at {
        if now >= at {
            state.quit_game_at = None;
            pause.open = false;
            pause.reset_match = true;
            effect.on = false;
            effect.visible = false;
            next.set(AppState::Menu);
        }
        return;
    }
    // Online play stopped (the host left, or stopped answering): back to the main
    // menu, which says why.
    if let Some(failure) = net.take_failure() {
        state.pending_message = Some(failure_message(&failure, text));
        state.fade.to(now, 1.0, FADE_SECS, Ease::RegularOut);
        state.quit_game_at = Some(now + FADE_SECS);
        return;
    }
    // The game is over (revive.rs: the team's last life lost) and its final scores
    // have shown (scoreboard.rs): the host ends the match, every player back in the
    // lobby (below); playing without the network, back to the menus.
    if team.over && team.over_for >= crate::scoreboard::END_SCREEN_SECS {
        if net.is_host() {
            net.end_match();
        } else if net.status() == NetStatus::Offline {
            state.prompt = None;
            state.fade.to(now, 1.0, FADE_SECS, Ease::RegularOut);
            state.quit_game_at = Some(now + FADE_SECS);
            return;
        }
    }
    // The host ended the match (Return to Lobby, or the game over): back to the
    // game's lobby, the others told why (not after the game over, which they saw).
    if net.status() == NetStatus::Lobby {
        state.prompt = None;
        state.return_to_lobby = true;
        if !net.is_host() && !team.over {
            state.pending_message = Some((
                RETURN_TO_LOBBY_TITLE.to_string(),
                MATCH_ENDED_TEXT.to_string(),
            ));
        }
        state.fade.to(now, 1.0, FADE_SECS, Ease::RegularOut);
        state.quit_game_at = Some(now + FADE_SECS);
        return;
    }

    let all_pads: Vec<&Gamepad> = gamepads.iter().collect();
    let nav = read_nav(state, &keys, &mouse, &all_pads, &window, &scroll);

    // A question over the pause menu (Quit Game's, Return to Lobby's) takes the
    // input while it is up: Yes does what it asks, No goes back to the menu.
    if let Some(Prompt::Confirm { action, .. }) = state.prompt.clone() {
        match confirm_answer(state, &nav, (&keys, &mouse, &gamepads), text, &mut sfx) {
            Some(true) => {
                sfx.play(SND_BUTTON_ACCEPT);
                state.prompt = None;
                match action {
                    ConfirmAction::QuitGame => {
                        // quitGame (QuitToMainMenu): leave the game (the others
                        // play on; the host leaving ends it for them), losing the
                        // match's experience, fade to black, then the menu.
                        pause.forfeit_xp = true;
                        net.leave();
                        state.fade.to(now, 1.0, FADE_SECS, Ease::RegularOut);
                        state.quit_game_at = Some(now + FADE_SECS);
                    }
                    // (Every player, this one too, sees the match end: above.)
                    ConfirmAction::ReturnToLobby => net.end_match(),
                    ConfirmAction::PrimeMode | ConfirmAction::ExitGame => {}
                }
            }
            Some(false) => {
                sfx.play(SND_BUTTON_BACK);
                state.prompt = None;
            }
            None => {}
        }
        return;
    }

    match state.page {
        Page::Game => {
            // (Not while the Choose Character screen is up as the match starts.)
            if pause.choosing {
                return;
            }
            if keys.just_pressed(KeyCode::Escape) || nav.start {
                // (Whose it is: split screen's guest's Start opens his.)
                let by_guest = split
                    .guest_pad()
                    .and_then(|e| gamepads.get(e).ok())
                    .is_some_and(|g| g.just_pressed(GamepadButton::Start));
                pause.seat = usize::from(by_guest);
                sfx.play(SND_PAUSE_OPEN);
                state.open_pause(now, text, net.is_host());
                cursor.grab_mode = CursorGrabMode::None;
                cursor.visible = true;
                pause.open = true;
                effect.start(pause.player_at);
            }
        }
        Page::Pause => {
            if nav.back || nav.start {
                sfx.play(SND_SUBMENU_EXIT);
                close_pause(state, &mut cursor, &mut pause, &mut effect, &mut sfx);
                return;
            }
            // The mouse focuses the row under it; a click presses it.
            let items = state.pause_items();
            let mut clicked = None;
            if let Some(p) = nav.cursor {
                let top = PAUSE_SHELL.y + state.pause_drift.get(now);
                for i in 0..items.len() {
                    let y = top + state.pause_row_offset(i);
                    let x = PAUSE_SHELL.x + PAUSE_MENU_X;
                    let hit = Rect::new(x - 10.0, y, x + 400.0, y + PAUSE_ROW_H);
                    if hit.contains(p) {
                        if nav.moved {
                            state.focus_pause_row(now, i);
                        }
                        if mouse.just_released(MouseButton::Left) && state.pause_focus == i {
                            clicked = Some(i);
                        }
                    }
                }
            }
            // Keys move the focus with the pause menu's own sounds.
            if nav.up && state.pause_focus > 0 {
                let f = state.pause_focus - 1;
                state.focus_pause_row(now, f);
                sfx.play(SND_PAUSE_UP);
            }
            if nav.down && state.pause_focus + 1 < items.len() {
                let f = state.pause_focus + 1;
                state.focus_pause_row(now, f);
                sfx.play(SND_PAUSE_DOWN);
            }
            let chosen = clicked.or(nav.accept.then_some(state.pause_focus));
            match chosen.and_then(|i| items.get(i).copied()) {
                Some(PauseItem::Resume) => {
                    close_pause(state, &mut cursor, &mut pause, &mut effect, &mut sfx);
                }
                Some(PauseItem::ChooseCharacter) if choose.available => {
                    // openCharacterSelect: the shell hides and the Choose Character
                    // screen opens over the pause menu.
                    choose.open_from_pause = true;
                    choose.closed = None;
                    state.page = Page::Choose;
                }
                Some(PauseItem::ChooseCharacter) => {}
                Some(PauseItem::Settings) => {
                    // openSettings: the shell hides and the settings menus open.
                    sfx.play(SND_SUBMENU_ENTER);
                    state.open_settings(now, text, settings.choices(Mode::Robot));
                }
                Some(PauseItem::ReturnToLobby) => {
                    sfx.play(SND_SUBMENU_ENTER);
                    state.open_prompt(
                        now,
                        Prompt::Confirm {
                            title: RETURN_TO_LOBBY_TITLE.to_string(),
                            text: RETURN_TO_LOBBY_TEXT.to_string(),
                            action: ConfirmAction::ReturnToLobby,
                        },
                    );
                }
                Some(PauseItem::Quit) => {
                    // QuitToMainMenu asks first (TnQuitMessageBox: Quit Game, and
                    // the GRI's GetQuitGameMessage).
                    sfx.play(SND_SUBMENU_ENTER);
                    state.open_prompt(
                        now,
                        Prompt::Confirm {
                            title: text.get("MessagePrompts.LeaveLobby", "Quit Game"),
                            text: QUIT_GAME_TEXT.to_string(),
                            action: ConfirmAction::QuitGame,
                        },
                    );
                }
                None => {}
            }
        }
        Page::Settings => {
            if settings_input(
                state,
                now,
                &nav,
                &keys,
                &mouse,
                &gamepads,
                text,
                &mut settings,
                &mut sfx,
            ) {
                state.back_to_pause();
            }
        }
        // closeCustomize: back to the pause menu as it was, or, a character chosen,
        // back to the game (onCustomizeClose: closePauseMenu).
        Page::Choose => match choose.closed.take() {
            Some(true) => close_pause(state, &mut cursor, &mut pause, &mut effect, &mut sfx),
            Some(false) => state.back_to_pause(),
            None => {}
        },
        Page::Main | Page::Extras | Page::Credits | Page::Lobby | Page::Customize => {}
    }
}

/// closePauseMenu: back to the game.
fn close_pause(
    state: &mut MenuState,
    cursor: &mut CursorOptions,
    pause: &mut Pause,
    effect: &mut crate::hologram::PauseEffect,
    sfx: &mut Sfx,
) {
    sfx.play(SND_REENTER);
    state.page = Page::Game;
    cursor.grab_mode = CursorGrabMode::Locked;
    cursor.visible = false;
    pause.open = false;
    pause.just_closed = true;
    effect.stop();
}

/// Back from the game to the main menu (Quit Game): the menu starts over as it does
/// when the demo starts, fading in from black, with its movie and music. A match
/// the host ended (Return to Lobby) comes back to the game's lobby instead, its
/// players still in it.
#[allow(clippy::too_many_arguments)]
fn reenter_menu(
    mut commands: Commands,
    time: Res<Time>,
    mut cursor: Single<&mut CursorOptions, With<PrimaryWindow>>,
    ui: Option<Res<MenuUi>>,
    state: Option<ResMut<MenuState>>,
    movie: Option<ResMut<Movie>>,
    mut pause: ResMut<Pause>,
    net: Res<Net>,
) {
    // (The menu's first showing, as the demo starts, needs none of this.)
    let (Some(ui), Some(mut state), Some(mut movie)) = (ui, state, movie) else {
        return;
    };
    if !state.gone {
        return;
    }
    let now = time.elapsed_secs();
    let credits_ready = state.credits_ready;
    let message = state.pending_message.take();
    let to_lobby = state.return_to_lobby;
    let pad_map = std::mem::take(&mut state.pad_map);
    *state = MenuState::new(now);
    state.credits_ready = credits_ready;
    state.pad_map = pad_map;
    // (Unless online play stopped meanwhile: the main menu says why.)
    if to_lobby && net.status() == NetStatus::Lobby {
        state.open_lobby(now, &net);
        state.show_lobby(now, LobbyView::Game, &net);
    }
    if let Some((title, text)) = message {
        state.open_prompt(now, Prompt::Message { title, text });
    }
    state
        .fade
        .from_to(now, 1.0, 0.0, FADE_SECS, Ease::RegularOut);
    let frames = start_movie(&movie.path, true, Some(movie.darken.clone()));
    movie.frames = frames;
    movie.next_at = now;
    movie.paused = false;
    commands
        .entity(ui.root)
        .insert(BackgroundColor(Color::BLACK));
    cursor.grab_mode = CursorGrabMode::None;
    cursor.visible = true;
    pause.open = false;
}

/// What the player did this frame, for the menus (keys, mouse and gamepad).
struct Nav {
    up: bool,
    down: bool,
    left: bool,
    right: bool,
    accept: bool,
    /// Esc, Backspace, B or a right click.
    back: bool,
    /// The gamepad's Start (it opens and closes the pause menu).
    start: bool,
    /// The mouse on the stage, and whether it moved.
    cursor: Option<Vec2>,
    moved: bool,
    stick: Vec2,
    /// How far the mouse wheel turned (up positive).
    wheel: f32,
}

fn read_nav(
    state: &mut MenuState,
    keys: &ButtonInput<KeyCode>,
    mouse: &ButtonInput<MouseButton>,
    gamepads: &[&Gamepad],
    window: &Window,
    scroll: &AccumulatedMouseScroll,
) -> Nav {
    // Gamepad: d-pad or left stick to move, A to choose, B to go back.
    let mut pad_up = false;
    let mut pad_down = false;
    let mut pad_left = false;
    let mut pad_right = false;
    let mut pad_accept = false;
    let mut pad_back = false;
    let mut start = false;
    let mut stick = Vec2::ZERO;
    for pad in gamepads {
        pad_up |= pad.just_pressed(GamepadButton::DPadUp);
        pad_down |= pad.just_pressed(GamepadButton::DPadDown);
        pad_left |= pad.just_pressed(GamepadButton::DPadLeft);
        pad_right |= pad.just_pressed(GamepadButton::DPadRight);
        pad_accept |= pad.just_pressed(GamepadButton::South);
        pad_back |= pad.just_pressed(GamepadButton::East);
        start |= pad.just_pressed(GamepadButton::Start);
        let s = pad.left_stick();
        if s.length() > stick.length() {
            stick = s;
        }
    }
    if stick.y > 0.6 && state.stick_y <= 0.6 {
        pad_up = true;
    }
    if stick.y < -0.6 && state.stick_y >= -0.6 {
        pad_down = true;
    }
    if stick.x < -0.6 && state.stick_x >= -0.6 {
        pad_left = true;
    }
    if stick.x > 0.6 && state.stick_x <= 0.6 {
        pad_right = true;
    }
    state.stick_y = stick.y;
    state.stick_x = stick.x;

    let view = View::new(window);
    let cursor = window.cursor_position().map(|p| view.to_stage(p));
    let moved = cursor.is_some() && cursor != state.last_cursor;
    state.last_cursor = cursor;
    Nav {
        up: keys.any_just_pressed([KeyCode::ArrowUp, KeyCode::KeyW]) || pad_up,
        down: keys.any_just_pressed([KeyCode::ArrowDown, KeyCode::KeyS]) || pad_down,
        left: keys.any_just_pressed([KeyCode::ArrowLeft, KeyCode::KeyA]) || pad_left,
        right: keys.any_just_pressed([KeyCode::ArrowRight, KeyCode::KeyD]) || pad_right,
        accept: keys.any_just_pressed([KeyCode::Enter, KeyCode::NumpadEnter, KeyCode::Space])
            || pad_accept,
        back: keys.any_just_pressed([KeyCode::Escape, KeyCode::Backspace])
            || pad_back
            || mouse.just_pressed(MouseButton::Right),
        start,
        cursor,
        moved,
        stick,
        wheel: scroll.delta.y,
    }
}

/// The settings menus' input (from the main menu or the pause menu). Returns true
/// when the root menu is closed (Back from it), for the caller to return.
#[allow(clippy::too_many_arguments)]
fn settings_input(
    state: &mut MenuState,
    now: f32,
    nav: &Nav,
    keys: &ButtonInput<KeyCode>,
    mouse: &ButtonInput<MouseButton>,
    gamepads: &Query<&Gamepad>,
    text: &MenuText,
    settings: &mut Settings,
    sfx: &mut Sfx,
) -> bool {
    // (Whether the controller can be used, and the Controller Layout's labels.)
    state.pad_ready = PadReady {
        chosen: settings.controls.controller,
        connected: !gamepads.is_empty(),
    };
    state.update_pad_labels(now, settings.controls.pad_schemes);
    // A table row waiting for its key takes every key, button and turn of the
    // wheel; a Display screen takes the input while it is up.
    if state.capture.is_some() {
        capture_input(state, now, nav, keys, mouse, gamepads, text, settings, sfx);
        return false;
    }
    if state.screen.is_some() {
        screen_input(state, now, nav, keys, mouse, gamepads, settings, sfx);
        return false;
    }
    // (The click that ended a wait for a key doesn't click again as it lets go.)
    let released = !state.release_wait && mouse.just_released(MouseButton::Left);
    if !mouse.pressed(MouseButton::Left) {
        state.release_wait = false;
    }
    let rows = state.rows();
    let choices = settings.choices(state.kbd_view);
    let cursor = nav.cursor;
    // The Back button.
    let back_rect = Rect::from_center_size(BACK_CENTER, BACK_SIZE);
    state.back_over = cursor.is_some_and(|p| back_rect.contains(p));
    let back_clicked = state.back_over && !state.dragging && released;
    if nav.back || back_clicked {
        // closeMenu: a sub-menu goes back to the menu it was opened from, focused
        // on the row that opened it; the root menu closes.
        sfx.play(SND_SUBMENU_EXIT);
        // (Changes waiting for Accept are left untaken.)
        settings.pending.0 = None;
        // Leaving the Graphics menu at a level whose textures differ from those
        // the demo started with, the game's notice that they wait for a restart
        // (UIText_PC's RestartToApplyTextureSettings), once for each level.
        let quality = settings.quality.0;
        if state.set_menu == SetMenu::Graphics
            && crate::graphics::needs_restart(settings.start_quality.0, *settings.quality)
            && state.restart_told != Some(quality)
        {
            state.restart_told = Some(quality);
            state.open_prompt(
                now,
                Prompt::Message {
                    title: text.get(
                        "MessagePrompts.RestartToApplyTextureSettingsTitle",
                        "Game Restart Required",
                    ),
                    text: text.get(
                        "MessagePrompts.RestartToApplyTextureSettingsMessage",
                        "Restarting the game is required for the texture quality settings to take effect.",
                    ),
                },
            );
        }
        let Some((parent, opener)) = state.set_menu.parent() else {
            return true;
        };
        let focus = parent
            .rows(state.kbd_view)
            .iter()
            .position(|&r| r == opener)
            .unwrap_or(0);
        state.show_menu(now, parent, focus, text, choices);
        return false;
    }

    // Accept: once Fullscreen Mode, Resolution or VSync has changed, beside Back,
    // and Enter or A (the game's A hint turns to Accept: updateGfxProfiles), all
    // three take effect together.
    let pending = state.set_menu == SetMenu::Graphics && settings.display_pending();
    let accept_rect = Rect::from_center_size(ACCEPT_CENTER, BACK_SIZE);
    state.accept_over = pending && cursor.is_some_and(|p| accept_rect.contains(p));
    if pending && (nav.accept || (state.accept_over && released && !state.dragging)) {
        sfx.play(SND_APPLY);
        if let Some(d) = settings.pending.0.take() {
            settings.fullscreen.0 = d.fullscreen;
            settings.resolution.0 = d.resolution;
            settings.vsync.0 = d.vsync;
        }
        return false;
    }

    // The mouse focuses the row under it (handleItemRollOver) and a click presses
    // it (not a row that can't be used).
    let mut clicked = None;
    if let Some(p) = cursor
        && !state.dragging
    {
        for (i, row) in rows.iter().enumerate() {
            let top = row_top(state, now, i);
            let hit = match row {
                // (Around the bar under it, centred 14.5 pixels down.)
                SetRow::Binding(_) => {
                    Rect::new(TABLE_HIT.0, top + 4.5, TABLE_HIT.1, top + 4.5 + TABLE_ROW_H)
                }
                _ => Rect::new(
                    SET_ANCHOR.x - 10.0,
                    top,
                    SET_ANCHOR.x + SLIDER_X + SLIDER_W + 40.0,
                    top + SET_ROW_H,
                ),
            };
            if hit.contains(p) && row.enabled(state.pad_ready) {
                if nav.moved {
                    state.focus_row(now, i, text, choices, sfx);
                }
                if released && state.set_focus == i {
                    clicked = Some(i);
                }
            }
        }
    }
    if nav.up
        && let Some(f) = state.next_row(false)
    {
        state.focus_row(now, f, text, choices, sfx);
    }
    if nav.down
        && let Some(f) = state.next_row(true)
    {
        state.focus_row(now, f, text, choices, sfx);
    }
    let Some(&row) = rows.get(state.set_focus) else {
        return false;
    };
    // (A row that can't be used takes nothing: the Controller Layout screen's,
    // focused as it opens, with no controller there.)
    if !row.enabled(state.pad_ready) {
        return false;
    }
    let pressed = clicked.is_some() || nav.accept;
    // Brightness and Safe Frame open their screens (openBrightness,
    // openSafeFrame).
    let screen = match row {
        SetRow::Brightness => Some(ScreenKind::Brightness),
        SetRow::SafeFrame => Some(ScreenKind::SafeFrame),
        _ => None,
    };
    if let Some(kind) = screen {
        if pressed {
            sfx.play(SND_SUBMENU_ENTER);
            state.open_screen(now, kind, &settings.display);
        }
        return false;
    }
    // Sub-menus open on a press (openSubMenu).
    if let Some(menu) = row.opens() {
        if pressed {
            sfx.play(SND_SUBMENU_ENTER);
            state.show_menu(now, menu, 0, text, choices);
        }
        return false;
    }
    if let Some(stepper) = row.stepper() {
        // A stepper moves to its next value on a press or Right, back on Left (both
        // wrapping round); a click on one of its values picks that one.
        let n = stepper.values(text, choices).len();
        let current = stepper.selected(choices);
        // (Where the values are drawn now: they slide out as the row gains the
        // focus.)
        let picked = clicked.and(cursor).and_then(|p| {
            let left = SET_ANCHOR.x + SET_VALUE_X + SET_VALUE_DX;
            let steps = &state.steps[stepper.index()];
            stepper
                .spans(text, choices)
                .iter()
                .enumerate()
                .position(|(i, &(_, w))| {
                    let x = left + steps[i].x.get(now);
                    steps[i].alpha.get(now) > 0.5 && p.x >= x - 3.0 && p.x <= x + w + 3.0
                })
        });
        let next = if let Some(i) = picked {
            Some(i)
        } else if pressed || nav.right {
            Some((current + 1) % n)
        } else if nav.left {
            Some((current + n - 1) % n)
        } else {
            None
        };
        if let Some(i) = next
            && i != current
        {
            if stepper == Stepper::ViewLayout {
                // setCurrentKbdViewMode: the table shows that form's keys.
                state.kbd_view = Mode::ALL[i.min(Mode::ALL.len() - 1)];
            } else {
                stepper.select(i, settings);
            }
            state.select_step(now, stepper, settings.choices(state.kbd_view));
        }
    } else if let Some(slider) = row.slider() {
        let (track, dec, inc) = slider_rects(row_top(state, now, state.set_focus));
        let source = settings.sliders();
        let (value, range) = (slider.value(&source), slider.range(&source));
        let new = slider_input(
            state,
            now,
            keys,
            mouse,
            gamepads,
            cursor,
            nav.stick,
            (nav.left, nav.right),
            (track, dec, inc),
            value,
            range,
        );
        // (Written only when it moves: the change is what saves it.)
        if (new - value).abs() > 1e-4 {
            sfx.play(SND_SLIDER_STEP);
            slider.set(new, settings);
            if let Some(sample) = slider.sample() {
                sfx.play(sample);
            }
        }
    } else if let SetRow::Binding(i) = row
        && pressed
    {
        // A table row waits for its new key (the demo's own; Turn/Look and Pause
        // can't be changed).
        if state.kbd_view.actions().get(i).is_none_or(|a| a.fixed()) {
            sfx.play(SND_LOCKED);
        } else {
            sfx.play(SND_SUBMENU_ENTER);
            state.capture = Some(Capture { row: i, slot: 0 });
        }
    }
    false
}

/// A Display screen's input. Brightness: Left and Right (A and D, the d-pad, the
/// stick), the slider's arrows or a drag along it move the brightness (1 of 100 a
/// step, repeating while held), which the game takes up at once; Enter (A, or a
/// click on Accept) keeps it (applyBrightness) and Esc (B, a right click, or Back)
/// puts it back as it was (closeBrightnessMenu). Safe Frame: Left and Right narrow
/// and widen the frame, Up and Down heighten and lower it (0.005 a step, repeating
/// while held, from 0 to 0.18: adjustSafeFrame); Enter keeps it (applySafeFrame)
/// and Esc leaves the one set before.
#[allow(clippy::too_many_arguments)]
fn screen_input(
    state: &mut MenuState,
    now: f32,
    nav: &Nav,
    keys: &ButtonInput<KeyCode>,
    mouse: &ButtonInput<MouseButton>,
    gamepads: &Query<&Gamepad>,
    settings: &mut Settings,
    sfx: &mut Sfx,
) {
    let Some(mut screen) = state.screen else {
        return;
    };
    let cursor = nav.cursor;
    let released = !state.dragging && mouse.just_released(MouseButton::Left);
    let back = Rect::from_center_size(BACK_CENTER, BACK_SIZE);
    let accept = Rect::from_center_size(ACCEPT_CENTER, BACK_SIZE);
    state.back_over = cursor.is_some_and(|p| back.contains(p));
    state.accept_over = screen.changed && cursor.is_some_and(|p| accept.contains(p));
    let pad = |button: GamepadButton| gamepads.iter().any(|p| p.just_pressed(button));
    if nav.back || (state.back_over && released) {
        sfx.play(SND_SUBMENU_EXIT);
        if screen.kind == ScreenKind::Brightness {
            settings.display.brightness = screen.brightness;
        }
        state.screen = None;
        return;
    }
    if keys.any_just_pressed([KeyCode::Enter, KeyCode::NumpadEnter])
        || pad(GamepadButton::South)
        || (state.accept_over && released)
    {
        sfx.play(SND_APPLY);
        if screen.kind == ScreenKind::SafeFrame {
            settings.display.safe_frame = screen.frame;
        }
        state.screen = None;
        return;
    }
    match screen.kind {
        ScreenKind::Brightness => {
            let value = settings.display.brightness;
            let new = slider_input(
                state,
                now,
                keys,
                mouse,
                gamepads,
                cursor,
                nav.stick,
                (nav.left, nav.right),
                brightness_slider_rects(),
                value,
                (0.0, 100.0, 1.0),
            );
            if (new - value).abs() > 1e-4 {
                sfx.play(SND_SLIDER_STEP);
                settings.display.brightness = new;
                screen.changed = true;
            }
        }
        ScreenKind::SafeFrame => {
            // Left, Right, Up, Down: pressed this frame, and held.
            let held = |codes: [KeyCode; 2], button: GamepadButton, stick: bool| {
                keys.any_pressed(codes) || gamepads.iter().any(|p| p.pressed(button)) || stick
            };
            let dirs = [
                (
                    nav.left,
                    held(
                        [KeyCode::ArrowLeft, KeyCode::KeyA],
                        GamepadButton::DPadLeft,
                        nav.stick.x < -0.6,
                    ),
                ),
                (
                    nav.right,
                    held(
                        [KeyCode::ArrowRight, KeyCode::KeyD],
                        GamepadButton::DPadRight,
                        nav.stick.x > 0.6,
                    ),
                ),
                (
                    nav.up,
                    held(
                        [KeyCode::ArrowUp, KeyCode::KeyW],
                        GamepadButton::DPadUp,
                        nav.stick.y > 0.6,
                    ),
                ),
                (
                    nav.down,
                    held(
                        [KeyCode::ArrowDown, KeyCode::KeyS],
                        GamepadButton::DPadDown,
                        nav.stick.y < -0.6,
                    ),
                ),
            ];
            let mut step = None;
            if let Some(d) = dirs.iter().position(|(pressed, _)| *pressed) {
                step = Some(d);
                state.repeat = Some((d as f32, now + 0.4));
            } else if let Some((d, at)) = state.repeat {
                let d = d as usize;
                if !dirs.get(d).is_some_and(|(_, held)| *held) {
                    state.repeat = None;
                } else if now >= at {
                    step = Some(d);
                    state.repeat = Some((d as f32, now + 0.05));
                }
            }
            // Left narrows the frame (its share across grows), Right widens it, Up
            // heightens it and Down lowers it.
            if let Some(d) = step {
                let (axis, delta) = match d {
                    0 => (0, SAFE_FRAME_STEP),
                    1 => (0, -SAFE_FRAME_STEP),
                    2 => (1, -SAFE_FRAME_STEP),
                    _ => (1, SAFE_FRAME_STEP),
                };
                let old = screen.frame[axis];
                let new = (old + delta).clamp(0.0, MAX_SAFE_FRAME);
                if (new - old).abs() > 1e-6 {
                    sfx.play(SND_SLIDER_STEP);
                    screen.frame[axis] = new;
                    screen.steps[axis] = Some((delta < 0.0, now));
                    screen.changed = true;
                }
            }
        }
    }
    state.screen = Some(screen);
}

/// The Brightness screen's slider: its track and its arrows (outside it).
fn brightness_slider_rects() -> (Rect, Rect, Rect) {
    let t = BRIGHTNESS_TRACK;
    let dec = Rect::new(t.min.x - TRANS_ARROW, t.min.y, t.min.x, t.max.y);
    let inc = Rect::new(t.max.x, t.min.y, t.max.x + TRANS_ARROW, t.max.y);
    (t, dec, inc)
}

/// A table row waiting for its new key: the first key, mouse button or turn of
/// the wheel pressed becomes its key (Controls::assign: the row that had it takes
/// this one's old key); Move takes four keys one after another (forward, left,
/// back, right). Escape, a gamepad's B or Start, or a click on Back stops waiting
/// (keys already given stay). Keys that can't be bound, buttons or the wheel for
/// Move and the wheel for an action that is held are refused.
#[allow(clippy::too_many_arguments)]
fn capture_input(
    state: &mut MenuState,
    now: f32,
    nav: &Nav,
    keys: &ButtonInput<KeyCode>,
    mouse: &ButtonInput<MouseButton>,
    gamepads: &Query<&Gamepad>,
    text: &MenuText,
    settings: &mut Settings,
    sfx: &mut Sfx,
) {
    let Some(capture) = state.capture else {
        return;
    };
    let click = mouse.just_pressed(MouseButton::Left);
    if click {
        state.release_wait = true;
    }
    let pad_back = gamepads
        .iter()
        .any(|p| p.just_pressed(GamepadButton::East) || p.just_pressed(GamepadButton::Start));
    let on_back = click
        && nav
            .cursor
            .is_some_and(|p| Rect::from_center_size(BACK_CENTER, BACK_SIZE).contains(p));
    if keys.just_pressed(KeyCode::Escape) || pad_back || on_back {
        sfx.play(SND_SUBMENU_EXIT);
        state.capture = None;
        return;
    }
    const BUTTONS: [MouseButton; 5] = [
        MouseButton::Left,
        MouseButton::Right,
        MouseButton::Middle,
        MouseButton::Back,
        MouseButton::Forward,
    ];
    // (Some(None): only keys that can't be bound.)
    let pressed: Vec<KeyCode> = keys.get_just_pressed().copied().collect();
    let input = if !pressed.is_empty() {
        Some(
            pressed
                .iter()
                .find_map(|&k| controls::bindable(k))
                .map(Input::Key),
        )
    } else if let Some(&b) = BUTTONS.iter().find(|&&b| mouse.just_pressed(b)) {
        Some(Some(Input::Mouse(b)))
    } else if nav.wheel > 0.0 {
        Some(Some(Input::WheelUp))
    } else if nav.wheel < 0.0 {
        Some(Some(Input::WheelDown))
    } else {
        None
    };
    let Some(input) = input else {
        return;
    };
    let mode = state.kbd_view;
    let assigned = input.is_some_and(|input| {
        settings
            .controls
            .assign(mode, capture.row, capture.slot, input)
    });
    if !assigned {
        sfx.play(SND_LOCKED);
        return;
    }
    sfx.play(SND_SLIDER_STEP);
    let more = mode.actions().get(capture.row) == Some(&Action::Move) && capture.slot < 3;
    state.capture = more.then_some(Capture {
        slot: capture.slot + 1,
        ..capture
    });
    // Select Layout shows Custom now.
    let choices = settings.choices(mode);
    state.select_step(now, Stepper::SelectLayout, choices);
    state.lay_out_stepper(now, Stepper::SelectLayout, false, text, choices);
}

/// A slider (MenuSlider): the arrow keys or d-pad step it (held, they repeat), the
/// arrows step it, and the track can be dragged. Returns its new value, between
/// `min` and `max` in steps of `step`.
#[allow(clippy::too_many_arguments)]
fn slider_input(
    state: &mut MenuState,
    now: f32,
    keys: &ButtonInput<KeyCode>,
    mouse: &ButtonInput<MouseButton>,
    gamepads: &Query<&Gamepad>,
    cursor: Option<Vec2>,
    stick: Vec2,
    (left_key, right_key): (bool, bool),
    (track, dec, inc): (Rect, Rect, Rect),
    value: f32,
    (min, max, step_size): (f32, f32, f32),
) -> f32 {
    let mut value = value;
    let mut step = 0.0;
    if left_key {
        step = -1.0;
        state.repeat = Some((-1.0, now + 0.4));
    } else if right_key {
        step = 1.0;
        state.repeat = Some((1.0, now + 0.4));
    }
    // Holding a direction repeats.
    let held_left = keys.any_pressed([KeyCode::ArrowLeft, KeyCode::KeyA])
        || stick.x < -0.6
        || gamepads.iter().any(|p| p.pressed(GamepadButton::DPadLeft));
    let held_right = keys.any_pressed([KeyCode::ArrowRight, KeyCode::KeyD])
        || stick.x > 0.6
        || gamepads.iter().any(|p| p.pressed(GamepadButton::DPadRight));
    if let Some((dir, at)) = state.repeat {
        let held = if dir < 0.0 { held_left } else { held_right };
        if !held {
            state.repeat = None;
        } else if step == 0.0 && now >= at {
            step = dir;
            state.repeat = Some((dir, now + 0.08));
        }
    }
    if let Some(p) = cursor {
        if mouse.just_pressed(MouseButton::Left) {
            if dec.contains(p) {
                step = -1.0;
            } else if inc.contains(p) {
                step = 1.0;
            } else if track.contains(p) {
                state.dragging = true;
            }
        }
        if state.dragging && mouse.pressed(MouseButton::Left) {
            let v = ((p.x - track.min.x) / (track.width() - SLIDER_BAR_MIN)).clamp(0.0, 1.0);
            value = min + (v * (max - min) / step_size).round() * step_size;
        }
    }
    if !mouse.pressed(MouseButton::Left) {
        state.dragging = false;
    }
    if step != 0.0 {
        value = min + (((value - min) / step_size).round() + step) * step_size;
    }
    value.clamp(min, max)
}

// ---------------------------------------------------------------------------
// Movie and music
// ---------------------------------------------------------------------------

fn menu_movie(
    time: Res<Time>,
    state: Option<Res<MenuState>>,
    mut movie: ResMut<Movie>,
    mut images: ResMut<Assets<Image>>,
) {
    let Some(state) = state else {
        return;
    };
    let movie = &mut *movie;
    let now = time.elapsed_secs();
    // Behind the settings the front end stops the movie and darkens it; behind
    // Extras it plays on, darkened: the front end's overlay, and the Extras movie's
    // background (the same gradient, multiplied in again) fading in, put on each
    // frame as it is decoded. Under the credits it waits.
    let extras = state.page == Page::Extras;
    let again = if extras {
        state.extras_background(now)
    } else {
        -1.0
    };
    movie.darken.store(again.to_bits(), Ordering::Relaxed);
    // (The lobby covers it.)
    let paused = matches!(
        state.page,
        Page::Settings | Page::Credits | Page::Lobby | Page::Customize
    );
    if paused != movie.paused {
        movie.paused = paused;
        if paused && state.page == Page::Settings {
            if let Some(image) = movie.image.as_ref().and_then(|h| images.get_mut(h)) {
                let w = image.width() as usize;
                let h = image.height() as usize;
                if let Some(data) = image.data.as_mut() {
                    darken(data, w, h, 0.0);
                }
            }
        } else {
            movie.next_at = now;
        }
    }
    if movie.paused {
        return;
    }
    let mut latest = None;
    let mut ended = false;
    if let Some(rx) = movie.frames.as_ref().and_then(|m| m.lock().ok()) {
        while movie.next_at <= now {
            match rx.try_recv() {
                Ok(frame) => {
                    latest = Some(frame);
                    movie.next_at += 1.0 / MOVIE_FPS;
                }
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => {
                    ended = true;
                    break;
                }
            }
        }
    }
    if ended {
        movie.frames = None;
    }
    // Don't race to catch up after a stall.
    if movie.next_at < now - 0.25 {
        movie.next_at = now;
    }
    let Some(mut frame) = latest else {
        return;
    };
    if extras != frame.darkened {
        if !extras {
            // Darkened for Extras, which has just closed: wait for the next.
            return;
        }
        // Decoded before Extras opened: darkened here.
        darken(
            &mut frame.rgba,
            frame.w as usize,
            frame.h as usize,
            state.extras_background(now),
        );
    }
    show_frame(&mut images, &mut movie.image, frame.w, frame.h, frame.rgba);
}

/// Multiplies a frame by the overlay gradient (top to bottom), and by it again at
/// `again` opacity (0 to 1: the Extras menu's own background over the overlay).
fn darken(data: &mut [u8], w: usize, h: usize, again: f32) {
    for y in 0..h {
        let t = y as f32 / (h.max(2) - 1) as f32;
        let c = overlay_color(t);
        let mul = c.map(|v| {
            let k = v / 255.0;
            (k * (1.0 - again + again * k) * 256.0) as u32
        });
        let row = &mut data[y * w * 4..(y + 1) * w * 4];
        for px in row.chunks_exact_mut(4) {
            for k in 0..3 {
                px[k] = ((px[k] as u32 * mul[k]) >> 8) as u8;
            }
        }
    }
}

fn overlay_color(t: f32) -> [f32; 3] {
    for pair in OVERLAY.windows(2) {
        let ((t0, c0), (t1, c1)) = (pair[0], pair[1]);
        if t <= t1 {
            let k = ((t - t0) / (t1 - t0)).clamp(0.0, 1.0);
            return [0, 1, 2].map(|i| c0[i] + (c1[i] - c0[i]) * k);
        }
    }
    OVERLAY[OVERLAY.len() - 1].1
}

/// The front end's music cue (MP_FRONT_END): the Transformers theme, then
/// "Metroplex Revealed" fading in and looping; the cue starts over after 287 s.
fn menu_music(
    mut commands: Commands,
    time: Res<Time>,
    bank: Option<Res<SoundBank>>,
    state: Option<Res<MenuState>>,
    volumes: Res<Volumes>,
    mut music: ResMut<MenuMusic>,
    mut sinks: Query<&mut AudioSink>,
) {
    let now = time.elapsed_secs();
    if music.music.is_none() {
        let Some(cue) = bank.as_ref().and_then(|b| b.music(MUSIC)) else {
            return;
        };
        music.voices = vec![None; cue.layers.len()];
        music.music = Some(cue);
        music.started = now;
    }
    let level = state.map(|s| s.music_level.get(now)).unwrap_or(1.0);
    let MenuMusic {
        music: Some(cue),
        started,
        voices,
    } = &mut *music
    else {
        return;
    };
    let mut t = now - *started;
    if cue.loop_end > 0.0 && t >= cue.loop_end {
        for voice in voices.iter_mut() {
            if let Some(e) = voice.take() {
                commands.entity(e).try_despawn();
            }
        }
        *started = now;
        t = 0.0;
    }
    let settings_gain = volumes.gain(cue.class);
    for (i, layer) in cue.layers.iter().enumerate() {
        let gain =
            10f32.powf(layer.volume_db / 20.0) * layer.envelope_at(t) * settings_gain * level;
        let current = voices[i];
        match current {
            None if t >= layer.time => {
                let settings = if layer.looping {
                    PlaybackSettings::LOOP
                } else {
                    PlaybackSettings::DESPAWN
                };
                let e = commands
                    .spawn((
                        AudioPlayer::new(layer.wave.clone()),
                        settings.with_volume(Volume::Linear(gain)),
                    ))
                    .id();
                voices[i] = Some(e);
            }
            Some(e) => {
                if let Ok(mut sink) = sinks.get_mut(e) {
                    sink.set_volume(Volume::Linear(gain));
                }
            }
            None => {}
        }
    }
}

// ---------------------------------------------------------------------------
// Layout
// ---------------------------------------------------------------------------

type Parts<'w, 's> = Query<
    'w,
    's,
    (
        &'static mut Node,
        &'static mut Visibility,
        Option<&'static mut ImageNode>,
        Option<&'static mut TextColor>,
        Option<&'static mut TextFont>,
        Option<&'static mut Text>,
        &'static mut UiTransform,
        Option<&'static mut BackgroundColor>,
    ),
>;

/// Places an image over `r` (stage pixels) with `color`, and hides it if
/// invisible. `turn` squeezes it sideways about `pivot` (x, stage pixels).
fn put_image(
    parts: &mut Parts,
    entity: Entity,
    view: &View,
    r: Rect,
    color: Color,
    turn: Option<(f32, f32)>,
) {
    let Ok((mut node, mut vis, image, _, _, _, mut transform, _)) = parts.get_mut(entity) else {
        return;
    };
    let shown = color.alpha() > 0.003 && r.width() > 0.0 && r.height() > 0.0;
    vis.set_if_neq(if shown {
        Visibility::Inherited
    } else {
        Visibility::Hidden
    });
    if !shown {
        return;
    }
    let min = view.offset + r.min * view.scale;
    let size = r.size() * view.scale;
    node.set_if_neq(Node {
        position_type: PositionType::Absolute,
        left: Val::Px(min.x),
        top: Val::Px(min.y),
        width: Val::Px(size.x),
        height: Val::Px(size.y),
        ..default()
    });
    if let Some(mut image) = image
        && image.color != color
    {
        image.color = color;
    }
    transform.set_if_neq(squeeze(view, turn, min.x + size.x * 0.5));
}

/// A line of text with its left end at `x` and its baseline at `baseline`.
#[allow(clippy::too_many_arguments)]
fn put_text(
    parts: &mut Parts,
    entity: Entity,
    view: &View,
    x: f32,
    baseline: f32,
    size: f32,
    width: f32,
    color: Color,
    value: Option<&str>,
    turn: Option<(f32, f32)>,
) {
    let Ok((mut node, mut vis, _, text_color, font, text, mut transform, _)) =
        parts.get_mut(entity)
    else {
        return;
    };
    let shown = color.alpha() > 0.003;
    vis.set_if_neq(if shown {
        Visibility::Inherited
    } else {
        Visibility::Hidden
    });
    if !shown {
        return;
    }
    let left = view.offset.x + x * view.scale;
    node.set_if_neq(Node {
        position_type: PositionType::Absolute,
        left: Val::Px(left),
        top: Val::Px(view.offset.y + (baseline - size * ASCENT) * view.scale),
        ..default()
    });
    if let Some(mut c) = text_color {
        c.set_if_neq(TextColor(color));
    }
    if let Some(mut font) = font {
        let px = (size * view.scale).max(1.0);
        if (font.font_size - px).abs() > 0.01 {
            font.font_size = px;
        }
    }
    if let (Some(mut text), Some(value)) = (text, value)
        && text.0 != value
    {
        text.0 = value.to_string();
    }
    transform.set_if_neq(squeeze(view, turn, left + width * view.scale * 0.5));
}

/// The movie's buttons turn about their vertical axis (_yrotation) as they come
/// and go: drawn here as a squeeze about the button's origin.
fn squeeze(view: &View, turn: Option<(f32, f32)>, center_x: f32) -> UiTransform {
    let Some((degrees, pivot)) = turn else {
        return UiTransform::IDENTITY;
    };
    let k = degrees.to_radians().cos().max(0.0);
    if (k - 1.0).abs() < 1e-4 {
        return UiTransform::IDENTITY;
    }
    let pivot = view.offset.x + pivot * view.scale;
    UiTransform {
        translation: Val2::px((pivot - center_x) * (1.0 - k), 0.0),
        scale: Vec2::new(k, 1.0),
        rotation: Rot2::IDENTITY,
    }
}

/// The Controller Layout screen's controller map (see PAD_MAP_ITEM), its origin
/// where its row would be, as the menu drifts (`top`).
#[allow(clippy::too_many_arguments)]
fn layout_pad_map(
    parts: &mut Parts,
    view: &View,
    ui: &PadMapUi,
    state: &MenuState,
    text: &MenuText,
    top: f32,
    alpha: f32,
    now: f32,
) {
    let showing = state.set_menu == SetMenu::ControllerLayout && alpha > 0.0;
    let connected = state.pad_ready.connected;
    let map = &state.pad_map;
    let origin = Vec2::new(SET_ANCHOR.x, top + state.row_y(PAD_MAP_ITEM));
    let at = |r: Rect| Rect::from_corners(r.min + origin, r.max + origin);
    let picture = map
        .picture
        .as_ref()
        .filter(|_| showing && connected)
        .map_or(Rect::default(), |(_, r)| at(*r));
    put_image(
        parts,
        ui.picture,
        view,
        picture,
        Color::WHITE.with_alpha(alpha),
        None,
    );
    for (i, (label_ui, place)) in ui.labels.iter().zip(&map.labels).enumerate() {
        let label = state.pad_labels.labels.get(i).filter(|_| showing);
        // What it says, since when it came in, and how far faded.
        let (said, came, fade) = match label {
            Some(l) if !l.text.is_empty() => (l.text.as_str(), l.at, 0.0),
            Some(PadLabel {
                leaving: Some((said, since)),
                ..
            }) => {
                let e =
                    Ease::RegularOut.apply(((now - since) / PAD_LABEL_OUT_SECS).clamp(0.0, 1.0));
                (said.as_str(), f32::NEG_INFINITY, e)
            }
            _ => ("", 0.0, 1.0),
        };
        let shown = !said.is_empty() && fade < 1.0;
        let (near, whiteness) = pad_label_in(now - came);
        let seen = |p: Vec2| STAGE_CENTER + (p - STAGE_CENTER) * near;
        let r = at(place.art);
        let r = if shown {
            Rect::from_corners(seen(r.min), seen(r.max))
        } else {
            Rect::default()
        };
        let a = alpha * (1.0 - fade);
        put_image(
            parts,
            label_ui.art,
            view,
            r,
            Color::srgba(1.0 - fade, 1.0, 1.0, a),
            None,
        );
        put_image(
            parts,
            label_ui.white,
            view,
            r,
            Color::WHITE.with_alpha(a * whiteness),
            None,
        );
        let field = place.text;
        let w = text.width(DEMI_BOLD, said, field.size);
        let x = if place.right {
            field.x1 - FIELD_GUTTER - w
        } else {
            field.x0 + FIELD_GUTTER
        };
        put_pad_text(
            parts,
            view,
            &label_ui.text,
            origin + Vec2::new(x, field.y0 + FIELD_GUTTER),
            field.size,
            w,
            field.color,
            if shown { a } else { 0.0 },
            fade,
            near,
            whiteness,
            said,
        );
    }
    // With no controller, mc_controllerMapNone's line (a ControllerLabel too),
    // across its field.
    let said = text.get("MessagePrompts.ControllerErrorTitle", "Controller Error");
    let (near, whiteness) = pad_label_in(now - state.pad_labels.none_at);
    let (field, a) = match map.none {
        Some(f) if showing && !connected => (f, alpha),
        _ => (
            PadMapText {
                x0: 0.0,
                x1: 0.0,
                y0: 0.0,
                size: 16.0,
                color: Color::NONE,
            },
            0.0,
        ),
    };
    let w = text.width(DEMI_BOLD, &said, field.size);
    put_pad_text(
        parts,
        view,
        &ui.none,
        origin + Vec2::new((field.x0 + field.x1 - w) * 0.5, field.y0 + FIELD_GUTTER),
        field.size,
        w,
        field.color,
        a,
        0.0,
        near,
        whiteness,
        &said,
    );
}

/// A controller label `secs` after it came in: how near it is, seen (1: where it
/// belongs), and how white.
fn pad_label_in(secs: f32) -> (f32, f32) {
    let e = Ease::RegularOut.apply((secs / PAD_LABEL_IN_SECS).clamp(0.0, 1.0));
    let focal = STAGE_CENTER.x / (STAGE_FOV.to_radians() * 0.5).tan();
    let z = PAD_LABEL_Z * (1.0 - e);
    (focal / (focal + z), 1.0 - e)
}

/// A controller label's text, its top left at `at`: its colour turned white as
/// far as `whiteness`, and as far as `fade` without its red; its shadow (black,
/// turned white as it is); all of it seen `near` about the stage's middle (zoomed
/// at its size: the glyphs aren't drawn anew at every size).
#[allow(clippy::too_many_arguments)]
fn put_pad_text(
    parts: &mut Parts,
    view: &View,
    ui: &PadTextUi,
    at: Vec2,
    size: f32,
    width: f32,
    color: Color,
    alpha: f32,
    fade: f32,
    near: f32,
    whiteness: f32,
    said: &str,
) {
    let c = mix_color(color, Color::WHITE, whiteness).to_srgba();
    let color = Color::srgba(c.red * (1.0 - fade), c.green, c.blue, alpha);
    let shadow = Color::srgba(whiteness, whiteness, whiteness, alpha * PAD_SHADOW_ALPHA);
    let shown = alpha > 0.003;
    let copies = ui.shadow.iter().enumerate().map(|(i, &entity)| {
        let a = i as f32 * std::f32::consts::TAU / ui.shadow.len() as f32;
        (entity, Vec2::new(a.cos(), a.sin()) * 2.0, shadow)
    });
    for (entity, d, color) in copies.chain([(ui.text, Vec2::ZERO, color)]) {
        let p = at + d;
        put_text(
            parts,
            entity,
            view,
            p.x,
            p.y + size * ASCENT,
            size,
            width,
            if shown { color } else { Color::NONE },
            Some(said),
            None,
        );
        if shown
            && (near - 1.0).abs() > 1e-4
            && let Ok((_, _, _, _, _, _, mut transform, _)) = parts.get_mut(entity)
        {
            let middle = p + Vec2::new(width * 0.5, size * LINE_EMS * 0.5);
            let seen = STAGE_CENTER + (middle - STAGE_CENTER) * near;
            let shift = (seen - middle) * view.scale;
            *transform = UiTransform {
                translation: Val2::px(shift.x, shift.y),
                scale: Vec2::splat(near),
                rotation: Rot2::IDENTITY,
            };
        }
    }
}

fn with_alpha(color: Color, alpha: f32) -> Color {
    color.with_alpha(color.alpha() * alpha.clamp(0.0, 1.0))
}

fn layout_menu(
    time: Res<Time>,
    window: Single<&Window, With<PrimaryWindow>>,
    ui: Option<Res<MenuUi>>,
    state: Option<Res<MenuState>>,
    text: Option<Res<MenuText>>,
    movie: Option<Res<Movie>>,
    credits: Option<Res<Credits>>,
    settings: SettingsView,
    net: Res<Net>,
    images: Res<Assets<Image>>,
    gamepads: Query<(), With<Gamepad>>,
    mut parts: Parts,
) {
    let (Some(ui), Some(state), Some(text)) = (ui, state, text) else {
        return;
    };
    let SettingsView {
        volumes,
        fullscreen,
        cap,
        controls,
        display,
        quality,
        screen,
    } = settings;
    let display_choice = screen.choice(&fullscreen);
    let slider_source = SliderSource {
        volumes: &volumes,
        controls: &controls,
        resolution: display_choice.resolution,
        resolutions: &screen.resolutions.0,
        render_scale: screen.render_scale.0,
    };
    let now = time.elapsed_secs();
    let view = View::new(&window);

    // Black over everything while fading to and from the game.
    if let Ok((mut node, _, _, _, _, _, _, Some(mut bg))) = parts.get_mut(ui.fader) {
        let target = Color::BLACK.with_alpha(state.fade.get(now).clamp(0.0, 1.0));
        bg.set_if_neq(BackgroundColor(target));
        node.set_if_neq(Node {
            position_type: PositionType::Absolute,
            width: Val::Percent(100.0),
            height: Val::Percent(100.0),
            ..default()
        });
    }
    let main = state.page == Page::Main;
    let settings = state.page == Page::Settings;
    let extras = state.page == Page::Extras;

    // The background: the movie (or the still) covering the window.
    let cover = (view.window.x / 1280.0).max(view.window.y / 720.0);
    let bg_size = Vec2::new(1280.0, 720.0) * cover / view.scale;
    let bg_min = (view.window / view.scale - bg_size) * 0.5 - view.offset / view.scale;
    if let Some(movie) = movie.as_ref()
        && let Ok((_, _, Some(mut image), _, _, _, _, _)) = parts.get_mut(ui.background)
    {
        let handle = movie.image.clone().unwrap_or_else(|| movie.still.clone());
        if image.image != handle {
            image.image = handle;
        }
    }
    // (In the game, the game shows instead: through the pause menu, the settings.
    // Under the Customize screen, its garage.)
    let customize = state.page == Page::Customize;
    if !state.gone
        && let Ok((.., Some(mut bg))) = parts.get_mut(ui.root)
    {
        bg.set_if_neq(BackgroundColor(if customize {
            Color::NONE
        } else {
            Color::BLACK
        }));
    }
    let still_tint = if state.gone || customize {
        Color::NONE
    } else if (settings || extras) && movie.as_ref().is_some_and(|m| m.image.is_none()) {
        // No movie to darken: tint the still instead (behind Extras, by its own
        // background too).
        let again = if extras {
            state.extras_background(now)
        } else {
            0.0
        };
        let [r, g, b] = [0.1, 0.38, 0.46].map(|k: f32| k * (1.0 - again + again * k));
        Color::srgb(r, g, b)
    } else {
        Color::WHITE
    };
    put_image(
        &mut parts,
        ui.background,
        &view,
        Rect::from_corners(bg_min, bg_min + bg_size),
        still_tint,
        None,
    );

    // The logo.
    let intro = ((now - state.started) / LOGO_FADE_SECS).clamp(0.0, 1.0);
    let intro = 1.0 - (1.0 - intro) * (1.0 - intro);
    let logo_alpha = if main || state.leaving.is_some() {
        intro * state.logo_alpha.get(now)
    } else {
        0.0
    };
    let half = LOGO_SIZE * 0.5 * LOGO_SCALE * state.logo_scale.get(now);
    put_image(
        &mut parts,
        ui.logo,
        &view,
        Rect::from_corners(LOGO_POS - half, LOGO_POS + half),
        Color::WHITE.with_alpha(logo_alpha),
        None,
    );

    // The buttons.
    for (i, b) in state.buttons.iter().enumerate() {
        let origin = button_origin(&state, now, i) + Vec2::new(b.shift.get(now), 0.0);
        let alpha = if main { b.alpha.get(now) } else { 0.0 };
        let turn = Some((b.turn.get(now), origin.x));
        let focused = i == state.focus;
        let w = ui.label_widths[i];
        put_text(
            &mut parts,
            ui.labels[i],
            &view,
            origin.x + LABEL_RIGHT - w,
            origin.y + LABEL_BASELINE,
            LABEL_SIZE,
            w,
            with_alpha(if focused { LABEL_OVER } else { LABEL_UP }, alpha),
            None,
            turn,
        );
        // The brackets open from 40 pixels wider, fading in, over half a second.
        let (open, bracket_alpha) = match b.brackets_at {
            Some(at) if focused => {
                let k = Ease::RegularOut.apply(((now - at) / 0.5).clamp(0.0, 1.0));
                (1.0 - k, k)
            }
            _ => (0.0, 0.0),
        };
        let x0 = 363.0 - w - 20.0 * open;
        let x1 = x0 + w + 36.0 + 40.0 * open;
        let sx = BUTTON_SCALE.x;
        let sy = BUTTON_SCALE.y;
        let top = origin.y - 4.0 * sy;
        let height = BRACKET_IMAGE.y * sy;
        let color = Color::WHITE.with_alpha(alpha * bracket_alpha);
        let left = Rect::new(
            origin.x + x0 * sx,
            top,
            origin.x + (x0 + BRACKET_LEFT) * sx,
            top + height,
        );
        let right = Rect::new(
            origin.x + (x1 - BRACKET_RIGHT) * sx,
            top,
            origin.x + x1 * sx,
            top + height,
        );
        for (side, r) in [left, right].into_iter().enumerate() {
            let entity = ui.brackets[i][side];
            let mut loaded = false;
            if let Ok((_, _, Some(mut image), _, _, _, _, _)) = parts.get_mut(entity)
                && let Some(texture) = images.get(&image.image)
            {
                // Each side shows its corner of the image (its middle is empty).
                // The movie's 229 x 47 image was stretched to a 256 x 64 texture,
                // so the corners are cut out in the texture's own pixels.
                let k = texture.size_f32() / BRACKET_IMAGE;
                let (u0, u1) = if side == 0 {
                    (0.0, BRACKET_LEFT)
                } else {
                    (BRACKET_IMAGE.x - BRACKET_RIGHT, BRACKET_IMAGE.x)
                };
                let rect = Some(Rect::new(u0 * k.x, 0.0, u1 * k.x, BRACKET_IMAGE.y * k.y));
                if image.rect != rect {
                    image.rect = rect;
                }
                loaded = true;
            }
            let color = if loaded { color } else { Color::NONE };
            put_image(&mut parts, entity, &view, r, color, turn);
        }
    }

    // The description of the focused item, with its glow.
    let desc: &str = &ui.descriptions[state.focus];
    let desc_w = text.width(DEMI_BOLD, desc, SUMMARY_SIZE);
    let desc_x = SHELL_X + SUMMARY_RIGHT - desc_w;
    let desc_y = state.shell_y.get(now) - 0.7 + SUMMARY_BASELINE;
    let desc_alpha = if main {
        state.summary_alpha.get(now)
    } else {
        0.0
    };
    for (k, glow) in ui.summary_glow.iter().enumerate() {
        let a = k as f32 * std::f32::consts::TAU / ui.summary_glow.len() as f32;
        let d = Vec2::new(a.cos(), a.sin()) * 2.0;
        put_text(
            &mut parts,
            *glow,
            &view,
            desc_x + d.x,
            desc_y + d.y,
            SUMMARY_SIZE,
            desc_w,
            with_alpha(SUMMARY_GLOW, desc_alpha),
            Some(desc),
            None,
        );
    }
    put_text(
        &mut parts,
        ui.summary,
        &view,
        desc_x,
        desc_y,
        SUMMARY_SIZE,
        desc_w,
        Color::WHITE.with_alpha(desc_alpha),
        Some(desc),
        None,
    );

    // The button hint (the gamepad's A: Select), only while a gamepad is
    // connected; with the keyboard and mouse there's no button to show.
    let hint_alpha = if main && state.leaving.is_none() && !gamepads.is_empty() {
        intro
    } else {
        0.0
    };
    hint(
        &mut parts,
        &view,
        ui.hint_icon,
        ui.hint_text,
        ui.hint_width,
        HINT_CENTER,
        hint_alpha,
    );

    // The demo's version, at the window's bottom left, with the main menu.
    let version_alpha = if main && state.leaving.is_none() {
        intro
    } else {
        0.0
    };
    let corner = view.to_stage(Vec2::new(0.0, view.window.y));
    put_text(
        &mut parts,
        ui.version,
        &view,
        corner.x + VERSION_MARGIN.x,
        corner.y - VERSION_MARGIN.y,
        VERSION_SIZE,
        text.width(DEMI_BOLD, &ui.version_text, VERSION_SIZE),
        VERSION_COLOR.with_alpha(VERSION_COLOR.alpha() * version_alpha),
        Some(&ui.version_text),
        None,
    );
    let set_alpha = if settings {
        state.settings_alpha.get(now)
    } else {
        0.0
    };

    // The Back button (under the settings, and under Extras as its button hints
    // fade in).
    let back_alpha = if extras {
        Ease::RegularOut.apply(((now - state.extras_at) / EXTRAS_FOOTER_SECS).clamp(0.0, 1.0))
    } else {
        set_alpha
    };
    let back = Rect::from_center_size(BACK_CENTER, BACK_SIZE);
    put_image(
        &mut parts,
        ui.back_box,
        &view,
        back,
        BACK_FILL.with_alpha(back_alpha),
        None,
    );
    let back_color = if state.back_over {
        BACK_TEXT_OVER
    } else {
        BACK_TEXT
    };
    put_text(
        &mut parts,
        ui.back_text,
        &view,
        BACK_CENTER.x - ui.back_width * 0.5,
        BACK_CENTER.y + BACK_TEXT_SIZE * 0.35,
        BACK_TEXT_SIZE,
        ui.back_width,
        back_color.with_alpha(back_alpha),
        None,
        None,
    );

    // A Display screen hides the menu under it (its hideClip), all but Back. Its
    // Accept shows once something has changed, as the Graphics menu's does once
    // Fullscreen Mode, Resolution or VSync has.
    let graphics_pending = state.screen.is_none()
        && state.set_menu == SetMenu::Graphics
        && screen.pending.0.is_some_and(|p| {
            p != DisplayChoice {
                fullscreen: fullscreen.0,
                resolution: screen.resolution.0,
                vsync: screen.vsync.0,
            }
        });
    let accept_alpha = if state.screen.is_some_and(|s| s.changed) || graphics_pending {
        set_alpha
    } else {
        0.0
    };
    layout_display(
        &mut parts,
        &view,
        &ui.display,
        &state,
        &text,
        &display,
        if state.screen.is_some() {
            set_alpha
        } else {
            0.0
        },
        accept_alpha,
        now,
    );
    let set_alpha = if state.screen.is_some() {
        0.0
    } else {
        set_alpha
    };

    // The settings menu's background art.
    let k = if settings {
        Ease::RegularOut.apply(((now - state.settings_at) / SET_ART_SECS).clamp(0.0, 1.0))
    } else {
        0.0
    };
    let art_x = SET_ANCHOR.x - 36.0 + 100.0 * (1.0 - k);
    let art_w = 256.0 * (4.3 + 0.39 * k);
    let art_h = 256.0 * 2.81;
    hold_slice_scale(&mut parts, ui.set_art, &view);
    put_image(
        &mut parts,
        ui.set_art,
        &view,
        Rect::new(
            art_x,
            SET_ANCHOR.y - art_h * 0.5,
            art_x + art_w,
            SET_ANCHOR.y + art_h * 0.5,
        ),
        Color::WHITE.with_alpha(0.1016 * k),
        None,
    );

    // The menu showing: its selection bar, title and rows, all drifting with the
    // focus.
    let top = SET_ANCHOR.y + state.drift.get(now);
    let bar_cy = top + state.bar_y.get(now);
    let bar_half = 16.0 * 1.06 * state.bar_h.get(now);
    put_image(
        &mut parts,
        ui.set_bar,
        &view,
        Rect::new(
            SET_ANCHOR.x - 24.0,
            bar_cy - bar_half,
            SET_ANCHOR.x - 24.0 + 256.0 * 3.52,
            bar_cy + bar_half,
        ),
        Color::WHITE.with_alpha(set_alpha),
        None,
    );
    let title_row = top + state.title_y.get(now);
    let title = state.title(now);
    put_text(
        &mut parts,
        ui.set_title,
        &view,
        SET_ANCHOR.x + SET_TITLE_DX,
        title_row + SET_TITLE_DY + SET_TITLE_SIZE * ASCENT,
        SET_TITLE_SIZE,
        0.0,
        Color::WHITE.with_alpha(set_alpha),
        Some(&title),
        None,
    );
    let rows = state.rows();
    let choices = choices(
        (display_choice, &cap, &quality),
        &controls,
        &display,
        state.kbd_view,
    );
    // The focused row's parts darken on the bright bar (SET_FOCUS_TINT), as far
    // as it has the focus.
    let focus_tint = |f: f32| SET_FOCUS_TINT.map(|t| 1.0 + (t - 1.0) * f);
    let tinted = |c: Color, tint: [f32; 3], alpha: f32| {
        let c = c.to_srgba();
        Color::srgba(
            c.red * tint[0],
            c.green * tint[1],
            c.blue * tint[2],
            c.alpha * alpha,
        )
    };
    let mut slider_rows = Vec::new();
    // Each stepper's row (its index and top), if the menu has it.
    let mut stepper_rows = [None; STEPPERS.len()];
    for (i, row) in rows.iter().enumerate() {
        let y = top + state.row_y(i);
        if let Some(stepper) = row.stepper() {
            stepper_rows[stepper.index()] = Some((i, y));
        }
        if let Some(slider) = row.slider() {
            slider_rows.push((i, y, slider));
        }
    }
    for (i, entity) in ui.set_rows.iter().enumerate() {
        let row = rows
            .get(i)
            .copied()
            .filter(|r| !matches!(r, SetRow::Binding(_)));
        let Some(row) = row else {
            put_text(
                &mut parts,
                *entity,
                &view,
                0.0,
                0.0,
                SET_LABEL_SIZE,
                0.0,
                Color::NONE,
                None,
                None,
            );
            continue;
        };
        let y = top + state.row_y(i);
        let f = state.row_focus[i].get(now);
        let (key, fallback) = row.label();
        let label = text.get(key, fallback);
        let color = if row.enabled(state.pad_ready) {
            mix_color(SET_LABEL_UP, SET_LABEL_COLOR, f)
        } else {
            SET_LABEL_DISABLED
        };
        put_text(
            &mut parts,
            *entity,
            &view,
            SET_ANCHOR.x - 5.0 * f + SET_LABEL_DX,
            y + SET_LABEL_DY + SET_LABEL_SIZE * ASCENT,
            SET_LABEL_SIZE,
            0.0,
            with_alpha(color, set_alpha),
            Some(&label),
            None,
        );
    }

    // The steppers' values (the focused one's with their glow).
    let glowing = rows.get(state.set_focus).and_then(|r| r.stepper());
    let glow_focus = state.row_focus[state.set_focus.min(MAX_ROWS - 1)].get(now);
    for (i, copies) in ui.set_value_glow.iter().enumerate() {
        let stepper = glowing.filter(|s| stepper_rows[s.index()].is_some());
        let shown = stepper.and_then(|s| {
            let (_, y) = stepper_rows[s.index()]?;
            let value = s.values(&text, choices).get(i)?.clone();
            let step = &state.steps[s.index()][i];
            let on = step.on.get(now);
            let alpha = (0.25 + 0.75 * on) * step.alpha.get(now) * set_alpha * glow_focus;
            Some((y, value, step.x.get(now), alpha))
        });
        for (k, &copy) in copies.iter().enumerate() {
            let Some((y, value, x, alpha)) = &shown else {
                put_text(
                    &mut parts,
                    copy,
                    &view,
                    0.0,
                    0.0,
                    SET_VALUE_SIZE,
                    0.0,
                    Color::NONE,
                    None,
                    None,
                );
                continue;
            };
            let a = k as f32 * std::f32::consts::TAU / 8.0;
            let d = Vec2::new(a.cos(), a.sin()) * STEP_GLOW_REACH;
            put_text(
                &mut parts,
                copy,
                &view,
                SET_ANCHOR.x + SET_VALUE_X + x + SET_VALUE_DX + d.x,
                y + SET_VALUE_DY + SET_VALUE_SIZE * ASCENT + d.y,
                SET_VALUE_SIZE,
                0.0,
                STEP_GLOW.with_alpha(STEP_GLOW_ALPHA * alpha),
                Some(value),
                None,
            );
        }
    }
    // (Team Coloring Mode's values are drawn in their parts, each in its colour.)
    let team_parts = team_color_parts(&text);
    for stepper in STEPPERS {
        let values = stepper.values(&text, choices);
        let row = stepper_rows[stepper.index()];
        let steps = &state.steps[stepper.index()];
        let in_parts = stepper == Stepper::TeamColorMode;
        for (i, entity) in ui.set_values[stepper.index()].iter().enumerate() {
            let part_entities = ui.display.team_colors.get(i).filter(|_| in_parts);
            let shown = row.zip(values.get(i));
            let Some(((row, y), value)) = shown else {
                for &e in std::iter::once(entity).chain(part_entities.into_iter().flatten()) {
                    put_text(
                        &mut parts,
                        e,
                        &view,
                        0.0,
                        0.0,
                        SET_VALUE_SIZE,
                        0.0,
                        Color::NONE,
                        None,
                        None,
                    );
                }
                continue;
            };
            // "on": #66ffe4; "off": no red, at 25%. They keep their colours on the
            // focused row (MenuItemStepper's container in its "focused" state); as
            // the row loses the focus they start dark and come back (its "default"
            // state). (A colour transform: Team Coloring Mode's coloured words
            // take it too.)
            let step = &steps[i];
            let on = step.on.get(now);
            let focus = &state.row_focus[row];
            let losing = if focus.to < 0.5 { focus.get(now) } else { 0.0 };
            let [tr, tg, tb] = focus_tint(losing);
            let alpha = (0.25 + 0.75 * on) * step.alpha.get(now) * set_alpha;
            let transform = |c: Color| {
                let c = c.to_srgba();
                Color::srgba(c.red * on * tr, c.green * tg, c.blue * tb, c.alpha * alpha)
            };
            let x = SET_ANCHOR.x + SET_VALUE_X + step.x.get(now) + SET_VALUE_DX;
            let baseline = y + SET_VALUE_DY + SET_VALUE_SIZE * ASCENT;
            let (value_color, value_parts) = match (part_entities, team_parts.get(i)) {
                (Some(entities), Some(words)) => (Color::NONE, Some((entities, words))),
                _ => (transform(SET_LABEL_UP), None),
            };
            put_text(
                &mut parts,
                *entity,
                &view,
                x,
                baseline,
                SET_VALUE_SIZE,
                0.0,
                value_color,
                Some(value.as_str()),
                None,
            );
            if let Some((entities, words)) = value_parts {
                let mut part_x = x;
                for (&e, (word, color)) in entities.iter().zip(words) {
                    put_text(
                        &mut parts,
                        e,
                        &view,
                        part_x,
                        baseline,
                        SET_VALUE_SIZE,
                        0.0,
                        transform(color.unwrap_or(SET_LABEL_UP)),
                        Some(word.as_str()),
                        None,
                    );
                    part_x += text.width(DEMI_BOLD, word, SET_VALUE_SIZE);
                }
            }
        }
    }

    // The sliders: in their own colours with no arrows, dark with their arrows
    // as they take the focus.
    for (k, slider_ui) in ui.set_sliders.iter().enumerate() {
        let shown = slider_rows.get(k).copied();
        let alpha = if shown.is_some() { set_alpha } else { 0.0 };
        let (row, y, slider) = shown.unwrap_or((0, top, Slider::MasterVolume));
        let f = state.row_focus[row].get(now);
        let tint = focus_tint(f);
        let (track, dec, inc) = slider_rects(y);
        put_image(
            &mut parts,
            slider_ui.track,
            &view,
            track,
            Color::srgba(tint[0], tint[1], tint[2], 0.25 * alpha),
            None,
        );
        let share = slider.fill(&slider_source);
        // (Its text beside it, past its arrow: the game's resolution label.)
        let label = slider.label(&slider_source).filter(|_| shown.is_some());
        put_text(
            &mut parts,
            slider_ui.label,
            &view,
            inc.max.x + SLIDER_LABEL_GAP,
            y + SET_VALUE_DY + SET_VALUE_SIZE * ASCENT,
            SET_VALUE_SIZE,
            0.0,
            if label.is_some() {
                tinted(SET_LABEL_UP, tint, alpha)
            } else {
                Color::NONE
            },
            label.as_deref(),
            None,
        );
        let w = SLIDER_BAR_MIN + (SLIDER_W - SLIDER_BAR_MIN) * share;
        let fill_alpha = if share > 0.0 { alpha } else { 0.0 };
        let (x0, x1) = (track.min.x, track.min.x + w);
        let pieces = [
            (x0, x0 + SLIDER_END),
            (x0 + SLIDER_END, x1 - SLIDER_END),
            (x1 - SLIDER_END, x1),
        ];
        for (entity, (left, right)) in slider_ui.fill.into_iter().zip(pieces) {
            put_image(
                &mut parts,
                entity,
                &view,
                Rect::new(left, track.min.y, right, track.max.y),
                Color::srgba(tint[0], tint[1], tint[2], fill_alpha),
                None,
            );
        }
        let slide = SLIDER_ARROW_SLIDE * (1.0 - f);
        let arrows = [
            Rect::from_corners(dec.min + Vec2::X * slide, dec.max + Vec2::X * slide),
            Rect::from_corners(inc.min - Vec2::X * slide, inc.max - Vec2::X * slide),
        ];
        let [dr, dg, db] = SET_FOCUS_TINT;
        for (side, r) in arrows.into_iter().enumerate() {
            let entity = slider_ui.arrows[side];
            if let Ok((_, _, Some(mut image), _, _, _, _, _)) = parts.get_mut(entity) {
                let flip = side == 0;
                if image.flip_x != flip {
                    image.flip_x = flip;
                }
            }
            put_image(
                &mut parts,
                entity,
                &view,
                r,
                Color::srgba(dr, dg, db, f * alpha),
                None,
            );
        }
    }

    // The Keyboard/Mouse Layout screen's table (updateKbdLayout): the form's
    // actions and their keys in the layout chosen. A row waiting for its new key
    // says so, pulsing.
    let table = state.set_menu == SetMenu::KeyboardLayout;
    let actions = state.kbd_view.actions();
    let binds = controls.binds(state.kbd_view);
    for (j, &[action_entity, key_entity]) in ui.set_table.iter().enumerate() {
        let (Some(action), true) = (actions.get(j), table) else {
            for entity in [action_entity, key_entity] {
                put_text(
                    &mut parts,
                    entity,
                    &view,
                    0.0,
                    0.0,
                    TABLE_TEXT_SIZE,
                    0.0,
                    Color::NONE,
                    None,
                    None,
                );
            }
            continue;
        };
        let i = j + 2;
        let baseline = top + state.row_y(i) + SET_LABEL_DY + TABLE_TEXT_SIZE * ASCENT;
        let tint = focus_tint(state.row_focus[i].get(now));
        let (key, fallback) = action.text();
        let name = text.get(key, fallback);
        let w = text.width(LIGHT, &name, TABLE_TEXT_SIZE);
        put_text(
            &mut parts,
            action_entity,
            &view,
            TABLE_ACTION_RIGHT - w,
            baseline,
            TABLE_TEXT_SIZE,
            w,
            tinted(TABLE_ACTION, tint, set_alpha),
            Some(&name),
            None,
        );
        let (value, pulse) = match state.capture {
            Some(capture) if capture.row == j => {
                let prompt = if *action == Action::Move {
                    let direction = controls::MOVE_KEYS[capture.slot.min(3)];
                    format!("{direction}: {PRESS_A_KEY}")
                } else {
                    PRESS_A_KEY.to_string()
                };
                let pulse = 0.65 + 0.35 * (now * PRESS_PULSE_HZ * std::f32::consts::TAU).cos();
                (prompt, pulse)
            }
            _ => {
                let (key, fallback) = binds
                    .get(j)
                    .map(|b| b.text())
                    .unwrap_or(("", String::new()));
                (text.get(key, &fallback), 1.0)
            }
        };
        put_text(
            &mut parts,
            key_entity,
            &view,
            TABLE_KEY_X,
            baseline,
            TABLE_TEXT_SIZE,
            text.width(LIGHT, &value, TABLE_TEXT_SIZE),
            tinted(TABLE_KEY, tint, set_alpha * pulse),
            Some(&value),
            None,
        );
    }
    layout_pad_map(
        &mut parts,
        &view,
        &ui.pad_map,
        &state,
        &text,
        top,
        set_alpha,
        now,
    );

    layout_pause(&mut parts, &view, &ui.pause, &state, &text, now);
    layout_extras(&mut parts, &view, &ui.extras, &state, &text, now);
    layout_lobby(&mut parts, &view, &ui.lobby, &state, &text, &net, now);
    layout_prompt(&mut parts, &view, &ui.prompt, &state, &text, now);

    // The credits, fitted to the window on black.
    let showing = state.page == Page::Credits;
    if let Ok((mut node, _, _, _, _, _, _, Some(mut bg))) = parts.get_mut(ui.credits_back) {
        let target = if showing { Color::BLACK } else { Color::NONE };
        bg.set_if_neq(BackgroundColor(target));
        node.set_if_neq(Node {
            position_type: PositionType::Absolute,
            width: Val::Percent(100.0),
            height: Val::Percent(100.0),
            ..default()
        });
    }
    let frame = credits.as_ref().and_then(|c| c.image.clone());
    if let Some(handle) = &frame
        && let Ok((_, _, Some(mut image), _, _, _, _, _)) = parts.get_mut(ui.credits_image)
        && &image.image != handle
    {
        image.image = handle.clone();
    }
    let size = frame
        .as_ref()
        .and_then(|h| images.get(h))
        .map(|i| i.size_f32())
        .unwrap_or(Vec2::new(1280.0, 720.0));
    let fit = (view.window.x / size.x).min(view.window.y / size.y);
    let shown_size = size * fit;
    let shown_min = (view.window - shown_size) * 0.5;
    let stage_min = (shown_min - view.offset) / view.scale;
    put_image(
        &mut parts,
        ui.credits_image,
        &view,
        Rect::from_corners(stage_min, stage_min + shown_size / view.scale),
        if showing && frame.is_some() {
            Color::WHITE
        } else {
            Color::NONE
        },
        None,
    );
}

/// The Display screens (Brightness_GFX over CalibrationImage_GFX, and
/// SafeFrame_GFX), `alpha` as the settings show, and their Accept button.
#[allow(clippy::too_many_arguments)]
fn layout_display(
    parts: &mut Parts,
    view: &View,
    ui: &DisplayUi,
    state: &MenuState,
    text: &MenuText,
    display: &DisplaySettings,
    alpha: f32,
    accept_alpha: f32,
    now: f32,
) {
    let screen = state.screen;
    let kind = screen.map(|s| s.kind);
    let shown = |k: ScreenKind| if kind == Some(k) { alpha } else { 0.0 };
    let brightness = shown(ScreenKind::Brightness);
    let safe = shown(ScreenKind::SafeFrame);

    // Brightness: the panel, the picture fading in, and the slider (its bar from
    // the track's left, its share of the track as the brightness's of 100, never
    // shorter than its ends).
    put_image(
        parts,
        ui.panel,
        view,
        BRIGHTNESS_PANEL,
        Color::WHITE.with_alpha(brightness),
        None,
    );
    let fade = screen.map_or(0.0, |s| ((now - s.at) / SCREEN_FADE_SECS).clamp(0.0, 1.0));
    put_image(
        parts,
        ui.picture,
        view,
        CALIBRATION,
        Color::WHITE.with_alpha(brightness * fade),
        None,
    );
    let (track, dec, inc) = brightness_slider_rects();
    put_image(
        parts,
        ui.track,
        view,
        track,
        Color::WHITE.with_alpha(brightness),
        None,
    );
    let share = (display.brightness / 100.0).clamp(0.0, 1.0);
    let x0 = track.min.x;
    let x1 = x0 + (track.width() * share).max(2.0 * TRANS_BAR_END);
    let pieces = [
        (x0, x0 + TRANS_BAR_END),
        (x0 + TRANS_BAR_END, x1 - TRANS_BAR_END),
        (x1 - TRANS_BAR_END, x1),
    ];
    for (&entity, (left, right)) in ui.bar.iter().zip(pieces) {
        put_image(
            parts,
            entity,
            view,
            Rect::new(left, track.min.y, right, track.max.y),
            Color::WHITE.with_alpha(brightness),
            None,
        );
    }
    for (side, (&entity, r)) in ui.arrows.iter().zip([dec, inc]).enumerate() {
        if let Ok((_, _, Some(mut image), ..)) = parts.get_mut(entity)
            && image.flip_x != (side == 0)
        {
            image.flip_x = side == 0;
        }
        put_image(
            parts,
            entity,
            view,
            r,
            Color::WHITE.with_alpha(brightness),
            None,
        );
    }

    // Safe Frame: the safe area, the frame being set on the whole window (the
    // HUD's stage), at 30% (its 56-pixel glow drawn twice its size, as the movie
    // draws its image).
    let frame = screen.map_or(display.safe_frame, |s| s.frame);
    let center = Vec2::new(STAGE_W, STAGE_H) * 0.5;
    let half = view.window / view.scale * (Vec2::ONE - frame) * 0.5;
    if let Ok((_, _, Some(mut image), ..)) = parts.get_mut(ui.area)
        && let NodeImageMode::Sliced(slicer) = &image.image_mode
        && (slicer.max_corner_scale - 2.0 * view.scale).abs() > 1e-4
    {
        let mut slicer = slicer.clone();
        slicer.max_corner_scale = 2.0 * view.scale;
        image.image_mode = NodeImageMode::Sliced(slicer);
    }
    put_image(
        parts,
        ui.area,
        view,
        Rect::from_center_half_size(center, half),
        Color::WHITE.with_alpha(0.3 * safe),
        None,
    );
    // Its arrows, 10 pixels in from its edges (up, right, down, left), each
    // turned from the up arrow about its tip. Stepping the frame plays their
    // 'increase' (it grew: they flash white, start a quarter bigger and 3.9
    // pixels further out) or 'decrease' (white, 22% smaller and 5 pixels in),
    // easing back over 8 frames.
    let tips = [
        (Vec2::new(center.x, center.y - half.y + SAFE_ARROW_INSET), 1),
        (Vec2::new(center.x + half.x - SAFE_ARROW_INSET, center.y), 0),
        (Vec2::new(center.x, center.y + half.y - SAFE_ARROW_INSET), 1),
        (Vec2::new(center.x - half.x + SAFE_ARROW_INSET, center.y), 0),
    ];
    for (k, (tip, axis)) in tips.into_iter().enumerate() {
        let step = screen.and_then(|s| s.steps[axis]);
        let (scale, nudge, white) = match step {
            Some((grew, at)) => {
                let t = ((now - at) / SAFE_ARROW_SECS).clamp(0.0, 1.0);
                let w = (1.0 - t) * (1.0 - t);
                if grew {
                    (1.0 + 0.25 * w, -3.9 * w, w)
                } else {
                    (1.0 - 0.21875 * w, 5.0 * w, w)
                }
            }
            None => (1.0, 0.0, 0.0),
        };
        // The up arrow's image is 64 pixels square, its tip 17 pixels from its top
        // (its centre 15 below the tip); turned a quarter more for each.
        let degrees = 90.0 * k as f32;
        let along =
            Vec2::from_angle(degrees.to_radians()).rotate(Vec2::new(0.0, 15.0 * scale + nudge));
        let r = Rect::from_center_half_size(tip + along, Vec2::splat(SAFE_ARROW * 0.5 * scale));
        put_image_turned(
            parts,
            ui.area_arrows[k],
            view,
            r,
            Color::WHITE.with_alpha(safe),
            degrees,
        );
        put_image_turned(
            parts,
            ui.area_flashes[k],
            view,
            r,
            Color::WHITE.with_alpha(safe * white),
            degrees,
        );
    }
    // The band behind the directions: black in the middle, fading out to its
    // ends, at half.
    for (side, &entity) in ui.band.iter().enumerate() {
        if let Ok((_, _, Some(mut image), ..)) = parts.get_mut(entity)
            && image.flip_x != (side == 1)
        {
            image.flip_x = side == 1;
        }
        let r = if side == 0 {
            Rect::new(SAFE_BAND.min.x, SAFE_BAND.min.y, center.x, SAFE_BAND.max.y)
        } else {
            Rect::new(center.x, SAFE_BAND.min.y, SAFE_BAND.max.x, SAFE_BAND.max.y)
        };
        put_image(
            parts,
            entity,
            view,
            r,
            Color::WHITE.with_alpha(0.5 * safe),
            None,
        );
    }

    // Either's directions (one line, centred under the slider or in the band).
    let directions = match kind {
        Some(ScreenKind::Brightness) => text.get(
            "Settings.BrightnessDirections",
            "Adjust brightness so the left emblem is barely visible.",
        ),
        Some(ScreenKind::SafeFrame) => text.get(
            "Settings.SafeFrameDirections",
            "Move the arrows to adjust the safe frame to fit your display.",
        ),
        None => String::new(),
    };
    let w = text.width(DEMI_BOLD, &directions, DIRECTIONS_SIZE);
    put_text(
        parts,
        ui.directions,
        view,
        center.x - w * 0.5,
        DIRECTIONS_TOP + DIRECTIONS_SIZE * ASCENT,
        DIRECTIONS_SIZE,
        w,
        Color::WHITE.with_alpha(if kind.is_some() { alpha } else { 0.0 }),
        Some(&directions),
        None,
    );

    // Accept, beside Back, once something has changed (setButtonHint('A')).
    put_image(
        parts,
        ui.accept_box,
        view,
        Rect::from_center_size(ACCEPT_CENTER, BACK_SIZE),
        BACK_FILL.with_alpha(accept_alpha),
        None,
    );
    let accept_color = if state.accept_over {
        BACK_TEXT_OVER
    } else {
        BACK_TEXT
    };
    put_text(
        parts,
        ui.accept_text,
        view,
        ACCEPT_CENTER.x - ui.accept_width * 0.5,
        ACCEPT_CENTER.y + BACK_TEXT_SIZE * 0.35,
        BACK_TEXT_SIZE,
        ui.accept_width,
        accept_color.with_alpha(accept_alpha),
        None,
        None,
    );
}

/// Colour `a` turning into `b` (k from 0 to 1).
fn mix_color(a: Color, b: Color, k: f32) -> Color {
    let (a, b) = (a.to_srgba(), b.to_srgba());
    Color::srgba(
        a.red + (b.red - a.red) * k,
        a.green + (b.green - a.green) * k,
        a.blue + (b.blue - a.blue) * k,
        a.alpha + (b.alpha - a.alpha) * k,
    )
}

/// Keeps a sliced image's fixed borders at the stage's scale, as a movie's scale-9
/// grid keeps them at their own size however far the clip is stretched (Bevy
/// scales them by the image's least stretch, up to `max_corner_scale`).
fn hold_slice_scale(parts: &mut Parts, entity: Entity, view: &View) {
    if let Ok((_, _, Some(mut image), ..)) = parts.get_mut(entity)
        && let NodeImageMode::Sliced(slicer) = &image.image_mode
        && slicer.max_corner_scale != view.scale
    {
        let mut slicer = slicer.clone();
        slicer.max_corner_scale = view.scale;
        image.image_mode = NodeImageMode::Sliced(slicer);
    }
}

/// Shows `handle` in an image part (mirrored or not).
fn set_image(parts: &mut Parts, entity: Entity, handle: &Handle<Image>, flip_x: bool) {
    if let Ok((_, _, Some(mut image), ..)) = parts.get_mut(entity) {
        if image.image != *handle {
            image.image = handle.clone();
        }
        if image.flip_x != flip_x {
            image.flip_x = flip_x;
        }
    }
}

/// `put_image`, turned `degrees` clockwise about the image's centre.
fn put_image_turned(
    parts: &mut Parts,
    entity: Entity,
    view: &View,
    r: Rect,
    color: Color,
    degrees: f32,
) {
    put_image(parts, entity, view, r, color, None);
    if let Ok((_, _, _, _, _, _, mut transform, _)) = parts.get_mut(entity) {
        transform.set_if_neq(UiTransform::from_rotation(Rot2::degrees(degrees)));
    }
}

/// Copies of a glowing text to each ring of its glow (put_glowing_text_rings).
const GLOW_RING: usize = 8;

/// A line of text with a glow: copies of it in `glow` 2 pixels around it.
#[allow(clippy::too_many_arguments)]
fn put_glowing_text(
    parts: &mut Parts,
    glow: &[Entity],
    entity: Entity,
    view: &View,
    at: Vec2,
    size: f32,
    color: Color,
    glow_color: Color,
    value: &str,
) {
    put_glowing_text_rings(
        parts,
        glow,
        entity,
        view,
        at,
        size,
        color,
        &[(2.0, glow_color)],
        value,
    );
}

/// The same with a glow of several rings of copies (`rings`: each one's distance
/// in pixels and colour), GLOW_RING copies of `glow` to a ring (the ones left over
/// hidden), each ring turned half a step from the one before.
#[allow(clippy::too_many_arguments)]
fn put_glowing_text_rings(
    parts: &mut Parts,
    glow: &[Entity],
    entity: Entity,
    view: &View,
    at: Vec2,
    size: f32,
    color: Color,
    rings: &[(f32, Color)],
    value: &str,
) {
    let baseline = at.y + size * ASCENT;
    let per_ring = glow.len().min(GLOW_RING).max(1);
    for (k, copy) in glow.iter().enumerate() {
        let (ring, i) = (k / per_ring, k % per_ring);
        let (radius, glow_color) = rings.get(ring).copied().unwrap_or((0.0, Color::NONE));
        let a = (i as f32 + 0.5 * ring as f32) * std::f32::consts::TAU / per_ring as f32;
        let d = Vec2::new(a.cos(), a.sin()) * radius;
        put_text(
            parts,
            *copy,
            view,
            at.x + d.x,
            baseline + d.y,
            size,
            0.0,
            glow_color,
            Some(value),
            None,
        );
    }
    put_text(
        parts,
        entity,
        view,
        at.x,
        baseline,
        size,
        0.0,
        color,
        Some(value),
        None,
    );
}

/// Places a node that clips its children (a ticker's window, a text input) over
/// `r`, and returns the view its children are placed in.
fn put_clip(parts: &mut Parts, entity: Entity, view: &View, r: Rect, shown: bool) -> View {
    if let Ok((mut node, mut vis, ..)) = parts.get_mut(entity) {
        vis.set_if_neq(if shown {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        });
        node.set_if_neq(Node {
            position_type: PositionType::Absolute,
            left: Val::Px(view.offset.x + r.min.x * view.scale),
            top: Val::Px(view.offset.y + r.min.y * view.scale),
            width: Val::Px(r.width() * view.scale),
            height: Val::Px(r.height() * view.scale),
            overflow: Overflow::clip(),
            ..default()
        });
    }
    View {
        scale: view.scale,
        offset: -r.min * view.scale,
        window: view.window,
    }
}

/// The Escalation lobby (MPLobby_GFX): its clouds and panels; the title, subtitle
/// and ticker; a game's map; the menu with its selection bar and hint; the player
/// list over the game mode's icon; the status and the countdown; Back. Joining,
/// "Connecting to Host" instead of the menu and the list.
#[allow(clippy::too_many_arguments)]
fn layout_lobby(
    parts: &mut Parts,
    view: &View,
    ui: &LobbyUi,
    state: &MenuState,
    text: &MenuText,
    net: &Net,
    now: f32,
) {
    let shown = state.page == Page::Lobby;
    let under_settings = state.page == Page::Settings && state.settings_from_lobby;
    let clouds = if shown || under_settings {
        Ease::RegularOut.apply(((now - state.lobby_opened) / LOBBY_FADE_SECS).clamp(0.0, 1.0))
    } else {
        1.0 - Ease::RegularOut.apply(((now - state.lobby_closed) / LOBBY_FADE_SECS).clamp(0.0, 1.0))
    };

    // The clouds: each tile the image turned some quarter turns, and over it the
    // image turned one more, fading in.
    let turns = now.max(0.0) / CLOUDS_TURN_SECS;
    let quarter = turns.floor();
    let fade = turns - quarter;
    let angle = (quarter % 4.0) * 90.0;
    let mut tiles = ui.clouds.iter();
    for row in CLOUD_ROWS {
        for column in CLOUD_COLUMNS {
            let Some(&[base, over]) = tiles.next() else {
                continue;
            };
            let min = CLOUDS_ORIGIN + Vec2::new(column as f32, row as f32) * CLOUDS_STEP;
            let r = Rect::from_corners(min, min + Vec2::splat(CLOUDS_SIZE));
            put_image_turned(parts, base, view, r, Color::WHITE.with_alpha(clouds), angle);
            put_image_turned(
                parts,
                over,
                view,
                r,
                Color::WHITE.with_alpha(clouds * fade),
                angle + 90.0,
            );
        }
    }
    // The shade, across the window and down the stage, black below it.
    let left = -view.offset.x / view.scale;
    let right = (view.window.x - view.offset.x) / view.scale;
    let bottom = (view.window.y - view.offset.y) / view.scale;
    put_image(
        parts,
        ui.clouds_shade,
        view,
        Rect::new(left, 0.0, right, STAGE_H),
        Color::WHITE.with_alpha(clouds),
        None,
    );
    put_image(
        parts,
        ui.clouds_below,
        view,
        Rect::new(left, STAGE_H, right, bottom.max(STAGE_H)),
        Color::BLACK.with_alpha(clouds),
        None,
    );
    // The panels go while the settings are up.
    let panels = if under_settings { 0.0 } else { clouds };
    put_image(
        parts,
        ui.panels,
        view,
        LOBBY_PANELS,
        Color::WHITE.with_alpha(panels),
        None,
    );

    // The view's contents, each coming in on its own time.
    let c = if shown { clouds } else { 0.0 };
    let lobby_view = state.lobby_view;
    let game = lobby_view == LobbyView::Game;
    let connecting = lobby_view == LobbyView::Connecting;
    let t = now - state.lobby_at;
    let intro = |at: f32, secs: f32| Ease::RegularOut.apply(((t - at) / secs).clamp(0.0, 1.0));
    let content = c * intro(LOBBY_CONTENT_AT, LOBBY_CONTENT_SECS);
    let game_content = if game { content } else { 0.0 };

    // The game mode's icon, turning in behind the player list.
    let k = intro(LOBBY_CONTENT_AT, 0.5);
    put_image_turned(
        parts,
        ui.mode_icon,
        view,
        Rect::from_center_half_size(MODE_ICON_CENTER, Vec2::splat(MODE_ICON_HALF)),
        with_alpha(MODE_ICON_TINT, k * if game { c } else { 0.0 }),
        -60.0 * (1.0 - k),
    );

    // The title and subtitle.
    let title = text
        .get("Lobby.EscalationLobbyLabel", "Escalation")
        .to_uppercase();
    let subtitle = if game {
        text.get(
            "TransGame.TnOnlineGameSettingsSV.RulesFormat",
            "Survive waves of attacks",
        )
    } else {
        text.get("Lobby.PartyLobbyTitle", "Party Lobby")
    }
    .to_uppercase();
    let k = intro(LOBBY_TITLE_AT[0], LOBBY_TITLE_SECS);
    put_glowing_text(
        parts,
        &ui.title_glow,
        ui.title,
        view,
        LOBBY_TITLE - Vec2::new(LOBBY_TITLE_SLIDE * (1.0 - k), 0.0),
        LOBBY_TITLE_SIZE,
        Color::WHITE.with_alpha(k * c),
        with_alpha(LOBBY_TITLE_GLOW, k * c),
        &title,
    );
    let k = intro(LOBBY_TITLE_AT[1], LOBBY_TITLE_SECS);
    put_text(
        parts,
        ui.subtitle,
        view,
        LOBBY_SUBTITLE.x - LOBBY_TITLE_SLIDE * (1.0 - k),
        LOBBY_SUBTITLE.y + LOBBY_SUBTITLE_SIZE * ASCENT,
        LOBBY_SUBTITLE_SIZE,
        0.0,
        LOBBY_TEAL.with_alpha(k * c),
        Some(&subtitle),
        None,
    );

    // The ticker: its message comes in at the right of its window, the next one
    // behind it, again and again.
    let ticker_alpha = c * intro(TICKER_AT, LOBBY_CONTENT_SECS);
    let inside = put_clip(parts, ui.ticker_box, view, TICKER, ticker_alpha > 0.003);
    let message = &state.ticker;
    let width = text.width(DEMI_BOLD, message, TICKER_SIZE);
    // (Its text field is 4 pixels wider than its text.)
    let period = (width + 4.0 + TICKER_GAP) / TICKER_SPEED;
    let elapsed = (now - state.ticker_at).max(0.0);
    let newest = (elapsed / period).floor();
    for (j, entity) in ui.ticker.iter().enumerate() {
        let n = newest - j as f32;
        let x = TICKER.width() - (elapsed - n * period) * TICKER_SPEED;
        let on = n >= 0.0 && x > -(width + 4.0) && !message.is_empty();
        put_text(
            parts,
            *entity,
            &inside,
            TICKER.min.x + x + 2.0,
            TICKER.min.y + TICKER_TEXT_Y + TICKER_SIZE * ASCENT,
            TICKER_SIZE,
            width,
            Color::WHITE.with_alpha(if on { ticker_alpha } else { 0.0 }),
            Some(message),
            None,
        );
    }

    // A game's map: the frame opens out (black inside it), then the picture comes
    // in under its shade, with the map's name.
    let map_alpha = if game { c } else { 0.0 };
    let frames = (t - LOBBY_CONTENT_AT) * 30.0;
    let wide = Ease::RegularOut.apply((frames / 4.0).clamp(0.0, 1.0));
    let tall = Ease::RegularOut.apply(((frames - 4.0) / 4.0).clamp(0.0, 1.0));
    let open = MAP_FRAME_START + (Vec2::ONE - MAP_FRAME_START) * Vec2::new(wide, tall);
    let center = MAP_FRAME.center();
    let opening = |r: Rect| {
        Rect::from_corners(
            center + (r.min - center) * open,
            center + (r.max - center) * open,
        )
    };
    let frame_alpha = if frames >= 0.0 { map_alpha } else { 0.0 };
    put_image(
        parts,
        ui.map_back,
        view,
        opening(MAP_PICTURE),
        Color::BLACK.with_alpha(frame_alpha),
        None,
    );
    let picture = map_alpha * Ease::RegularOut.apply(((frames - 8.0) / 9.0).clamp(0.0, 1.0));
    if let Some(image) = ui.map_pictures.get(net.map()) {
        set_image(parts, ui.map_picture, image, false);
    }
    put_image(
        parts,
        ui.map_picture,
        view,
        MAP_PICTURE,
        Color::WHITE.with_alpha(picture),
        None,
    );
    put_image(
        parts,
        ui.map_shade,
        view,
        MAP_PICTURE,
        Color::WHITE.with_alpha(0.7 * picture),
        None,
    );
    put_image(
        parts,
        ui.map_frame,
        view,
        opening(MAP_FRAME),
        Color::WHITE.with_alpha(frame_alpha),
        None,
    );
    let k = Ease::RegularOut.apply(((frames - 8.0) / 15.0).clamp(0.0, 1.0));
    let name = map_name(text, net.map());
    put_glowing_text(
        parts,
        &ui.map_label_glow,
        ui.map_label,
        view,
        MAP_LABEL - Vec2::new(20.0 * (1.0 - k), 0.0),
        MAP_LABEL_SIZE,
        MAP_LABEL_COLOR.with_alpha(map_alpha * k),
        Color::BLACK.with_alpha(0.3 * map_alpha * k),
        &name,
    );

    // The menu: the selection bar under the focused row, the rows, a selector's
    // value and arrows.
    let rows = lobby_rows(lobby_view, net.is_host(), state.prime);
    let menu_alpha = if rows.is_empty() { 0.0 } else { content };
    let focus = state.lobby_focus.min(rows.len().saturating_sub(1));
    let on_selector = rows.get(focus).is_some_and(|row| row.selector());
    set_image(
        parts,
        ui.bar,
        &ui.bar_images[usize::from(on_selector)],
        false,
    );
    let bar_y = lobby_row_top(lobby_view, 0) + state.lobby_bar_y.get(now);
    let bar_x = LOBBY_MENU_X + LOBBY_BAR_X;
    put_image(
        parts,
        ui.bar,
        view,
        Rect::new(
            bar_x,
            bar_y - LOBBY_BAR_SIZE.y * 0.5,
            bar_x + LOBBY_BAR_SIZE.x,
            bar_y + LOBBY_BAR_SIZE.y * 0.5,
        ),
        Color::WHITE.with_alpha(menu_alpha),
        None,
    );
    for i in 0..LOBBY_MAX_ROWS {
        let (label_part, value_part, arrow_parts) = (ui.rows[i], ui.values[i], ui.arrows[i]);
        let Some(&row) = rows.get(i) else {
            for part in [label_part, value_part] {
                put_text(
                    parts,
                    part,
                    view,
                    0.0,
                    0.0,
                    LOBBY_LABEL_SIZE,
                    0.0,
                    Color::NONE,
                    None,
                    None,
                );
            }
            for part in arrow_parts {
                put_image(
                    parts,
                    part,
                    view,
                    Rect::new(0.0, 0.0, 1.0, 1.0),
                    Color::NONE,
                    None,
                );
            }
            continue;
        };
        let top = lobby_row_top(lobby_view, i);
        let baseline = top + LOBBY_LABEL_Y + LOBBY_LABEL_SIZE * ASCENT;
        let f = state.lobby_row_focus[i].get(now);
        let row_state = row.state(net);
        let (key, fallback, _, _) = row.text();
        let label = text.get(key, fallback);
        let (x, color) = match (row.selector(), row_state) {
            (false, RowState::Disabled) => (LOBBY_LABEL_X, LOBBY_TEAL.with_alpha(0.5)),
            // (mc_lockPanelButton locked: white at half, #0f3d4d over.)
            (false, RowState::Locked) => (
                LOBBY_LABEL_X * (1.0 - f),
                mix_color(Color::WHITE.with_alpha(0.5), LOBBY_LOCKED_OVER, f),
            ),
            (false, _) => (
                LOBBY_LABEL_X * (1.0 - f),
                mix_color(LOBBY_TEAL, LOBBY_LABEL_OVER, f),
            ),
            (true, RowState::Disabled) => (LOBBY_LABEL_X, LOBBY_BLUE.with_alpha(0.25)),
            (true, _) => (LOBBY_LABEL_X, mix_color(LOBBY_TEAL, LOBBY_SELECTOR_OVER, f)),
        };
        put_text(
            parts,
            label_part,
            view,
            LOBBY_MENU_X + x,
            baseline,
            LOBBY_LABEL_SIZE,
            0.0,
            with_alpha(color, menu_alpha),
            Some(&label),
            None,
        );
        let value = match row {
            LobbyRow::SelectMap => Some(map_name(text, net.map())),
            LobbyRow::Difficulty => Some(difficulty_name(text, net.difficulty())),
            _ => None,
        };
        let value_color = if row_state == RowState::Disabled {
            LOBBY_BLUE.with_alpha(0.25)
        } else {
            mix_color(LOBBY_VALUE, LOBBY_SELECTOR_OVER, f)
        };
        put_text(
            parts,
            value_part,
            view,
            LOBBY_MENU_X + LOBBY_VALUE_X,
            baseline,
            LOBBY_LABEL_SIZE,
            0.0,
            with_alpha(value_color, if value.is_some() { menu_alpha } else { 0.0 }),
            value.as_deref(),
            None,
        );
        let arrows = row.selector() && row_state == RowState::Enabled;
        let (at, last) = if row == LobbyRow::SelectMap {
            map_place(net)
        } else {
            (net.difficulty(), 2)
        };
        for (side, part) in arrow_parts.into_iter().enumerate() {
            let can_move = if side == 0 { at > 0 } else { at < last };
            let color = if i != focus {
                LOBBY_BLUE.with_alpha(0.5)
            } else if can_move {
                Color::WHITE
            } else {
                LOBBY_BLUE.with_alpha(0.25)
            };
            // (Scaled to the row's height: k pixels for each of the image's.)
            let k = LOBBY_PANEL_H / LOBBY_ARROW.y;
            let w = LOBBY_ARROW.x * k;
            let x = LOBBY_MENU_X
                + if side == 0 {
                    LOBBY_ARROW_INSET - w
                } else {
                    LOBBY_SELECTOR_W - LOBBY_ARROW_INSET
                };
            set_image(parts, part, &ui.arrow_image, side == 0);
            put_image(
                parts,
                part,
                view,
                Rect::new(x - k, top - k, x + w + k, top + LOBBY_PANEL_H + k),
                with_alpha(color, if arrows { menu_alpha } else { 0.0 }),
                None,
            );
        }
    }

    // The Prime Mode row's lock (mc_lock, 24 pixels at 83%, 22 pixels in from the
    // row's right end): white at half, whole while focused.
    let locked = rows.iter().position(|r| *r == LobbyRow::PrimeModeLocked);
    let (lock_at, lock_alpha) = match locked {
        Some(i) => (
            Vec2::new(
                LOBBY_MENU_X + LOBBY_LOCK_X,
                lobby_row_top(lobby_view, i) + LOBBY_PANEL_H * 0.5,
            ),
            menu_alpha * (0.5 + 0.5 * state.lobby_row_focus[i].get(now)),
        ),
        None => (Vec2::ZERO, 0.0),
    };
    let half = LOBBY_LOCK_SIZE * 0.5;
    put_image(
        parts,
        ui.lock,
        view,
        Rect::new(
            lock_at.x - half,
            lock_at.y - half,
            lock_at.x + half,
            lock_at.y + half,
        ),
        Color::WHITE.with_alpha(lock_alpha),
        None,
    );

    // The focused row's hint.
    let k = intro(10.0 / 30.0, 10.0 / 30.0);
    let hint = rows
        .get(focus)
        .map(|row| {
            let (_, _, key, fallback) = row.text();
            let hint = text.get(key, fallback);
            // (Split screen: how the second player comes in, or goes.)
            if *row == LobbyRow::StartGame && !state.guest_hint.is_empty() {
                format!("{hint} {}", state.guest_hint)
            } else {
                hint
            }
        })
        .unwrap_or_default();
    let hint = wrap_text(text, DEMI_BOLD, &hint, LOBBY_HINT_SIZE, LOBBY_HINT_WIDTH);
    put_text(
        parts,
        ui.hint,
        view,
        LOBBY_HINT.x + 10.0 * (1.0 - k),
        LOBBY_HINT.y - 4.0 * (1.0 - k) + LOBBY_HINT_SIZE * ASCENT,
        LOBBY_HINT_SIZE,
        0.0,
        LOBBY_ORANGE.with_alpha(if rows.is_empty() { 0.0 } else { c * k }),
        Some(&hint),
        None,
    );

    // The player list: the team (or the party), the columns, the players. The
    // team is the one the map's Escalation is played by: the Decepticons on their
    // maps, else the Autobots.
    let list_alpha = if connecting { 0.0 } else { content };
    let decepticons = crate::MAPS.get(net.map()).is_some_and(|m| m.decepticons);
    let team_color = if decepticons {
        LIST_DECEPTICON
    } else {
        LIST_TEAM
    };
    let (header, header_color, glow) = if game && decepticons {
        (
            text.get("TransContent.TnTeamInfoDecepticons.TeamName", "Decepticons"),
            LIST_DECEPTICON,
            LIST_DECEPTICON_GLOW
                .map(|(r, c)| (r, with_alpha(c, list_alpha)))
                .to_vec(),
        )
    } else if game {
        (
            text.get("TransContent.TnTeamInfoAutobots.TeamName", "Autobots"),
            LIST_TEAM,
            vec![(2.0, with_alpha(LIST_TEAM_GLOW, list_alpha))],
        )
    } else {
        (
            text.get("Lobby.PartyMembersHeader", "Party Members"),
            LIST_PARTY,
            Vec::new(),
        )
    };
    put_glowing_text_rings(
        parts,
        &ui.header_glow,
        ui.header,
        view,
        LIST_HEADER,
        LIST_HEADER_SIZE,
        header_color.with_alpha(list_alpha),
        &glow,
        &header.to_uppercase(),
    );
    let columns = [
        (
            LIST_NAME_COLUMN,
            "TransGame.TnPlayerReplicationInfo.PlayerName.FriendlyName",
            "Name",
            list_alpha,
        ),
        (
            LIST_SCORE_COLUMN,
            "TransGame.TnPlayerReplicationInfo.Score.FriendlyName",
            "Score",
            game_content,
        ),
    ];
    for (part, (x, key, fallback, alpha)) in ui.columns.into_iter().zip(columns) {
        put_text(
            parts,
            part,
            view,
            x,
            LIST_COLUMNS_Y + LIST_COLUMN_SIZE * ASCENT,
            LIST_COLUMN_SIZE,
            0.0,
            LIST_COLUMN.with_alpha(alpha),
            Some(&text.get(key, fallback).to_uppercase()),
            None,
        );
    }
    // (In the party lobby, this PC's players: this player, and split screen's
    // Player 2 under him once he has joined.)
    let players: Vec<(usize, String, u16, u16, bool)> = if game {
        net.players()
            .map(|(slot, p)| (slot, p.name.clone(), p.score, p.ping_ms, p.prime))
            .collect()
    } else {
        let mut party = vec![(0, net.name().to_string(), 0, 0, net.prime())];
        if state.party_guest {
            party.push((1, GUEST_NAME.to_string(), 0, 0, false));
        }
        party
    };
    let me = if game { net.my_slot() } else { Some(0) };
    // (Split screen's "Press START to join" under this player's name while Player
    // 2 can join the party: in the lobby's orange, pulsing as its status does.)
    let pulse_frame = (now * 30.0).rem_euclid(25.0);
    let pulse = if pulse_frame < 14.0 {
        0.6 + 0.4 * pulse_frame / 14.0
    } else {
        1.0 - 0.4 * (pulse_frame - 14.0) / 11.0
    };
    let join_prompt = !game && state.party_hint && !state.party_guest;
    for (i, ((name_part, score_part), ping_part)) in
        ui.names.iter().zip(&ui.scores).zip(&ui.pings).enumerate()
    {
        let y = LIST_ROWS_Y + i as f32 * LIST_ROW_H;
        let entry = players.get(i);
        let alpha = if entry.is_some() { list_alpha } else { 0.0 };
        let (slot, name, score, ping, prime) = entry.cloned().unwrap_or_default();
        // A Prime's medal (mc_awardPrimeMode at 40%) left of his name: its star
        // turning (once every 194 frames) over its ring flipping (in its first 40
        // frames of 180).
        let medal_alpha = if prime { alpha } else { 0.0 };
        let center = Vec2::new(LIST_NAME_X + MEDAL_X, y + MEDAL_Y);
        let frame = now * 30.0;
        let flip = (frame % 180.0 / 40.0).min(1.0);
        let ring_k = (flip * std::f32::consts::PI).cos().abs().max(0.03);
        let [ring_part, star_part] = ui.medals[i];
        let ring = MEDAL_RING * 0.5;
        put_image(
            parts,
            ring_part,
            view,
            Rect::new(
                center.x - ring * ring_k,
                center.y - ring,
                center.x + ring * ring_k,
                center.y + ring,
            ),
            Color::WHITE.with_alpha(medal_alpha),
            None,
        );
        let star = MEDAL_STAR * 0.5;
        put_image_turned(
            parts,
            star_part,
            view,
            Rect::new(
                center.x - star,
                center.y - star,
                center.x + star,
                center.y + star,
            ),
            Color::WHITE.with_alpha(medal_alpha),
            frame % 194.0 / 194.0 * 360.0,
        );
        // (This PC's players: split screen's guest too, and Player 2 in the party.)
        let color = if Some(slot) == me
            || (game && Some(slot) == net.guest_slot())
            || (!game && entry.is_some() && slot == 1)
        {
            Color::WHITE
        } else {
            team_color
        };
        let baseline = y + LIST_ENTRY_Y + LIST_ENTRY_SIZE * ASCENT;
        let (name, color) = if join_prompt && i == 1 {
            (
                PARTY_JOIN_PROMPT.to_string(),
                LOBBY_ORANGE.with_alpha(list_alpha * pulse),
            )
        } else {
            (name, color.with_alpha(alpha))
        };
        put_text(
            parts,
            *name_part,
            view,
            LIST_NAME_X,
            baseline,
            LIST_ENTRY_SIZE,
            0.0,
            color,
            Some(&name),
            None,
        );
        let game_alpha = if game { alpha } else { 0.0 };
        put_text(
            parts,
            *score_part,
            view,
            LIST_SCORE_X,
            baseline,
            LIST_ENTRY_SIZE,
            0.0,
            Color::WHITE.with_alpha(game_alpha),
            Some(&score.to_string()),
            None,
        );
        let bars = match ping {
            0..80 => 2,
            80..160 => 1,
            _ => 0,
        };
        set_image(parts, *ping_part, &ui.ping_images[bars], false);
        put_image(
            parts,
            *ping_part,
            view,
            Rect::new(LIST_PING_X, y - 5.0, LIST_PING_X + 32.0, y + 27.0),
            Color::WHITE.with_alpha(game_alpha),
            None,
        );
    }

    // The status, pulsing, until the countdown starts.
    let waiting = net.countdown().is_none() && !net.in_match();
    let frame = (now * 30.0).rem_euclid(25.0);
    let pulse = if frame < 14.0 {
        0.6 + 0.4 * frame / 14.0
    } else {
        1.0 - 0.4 * (frame - 14.0) / 11.0
    };
    let status = text.get(
        "TransGame.TnGameReplicationInfoLobby.WaitingForHostToStartStatus",
        "Waiting for host to start game",
    );
    let w = text.width(DEMI_BOLD, &status, LOBBY_STATUS_SIZE);
    put_text(
        parts,
        ui.status,
        view,
        LOBBY_STATUS.x - w,
        LOBBY_STATUS.y + LOBBY_STATUS_SIZE * ASCENT,
        LOBBY_STATUS_SIZE,
        w,
        LOBBY_ORANGE.with_alpha(if waiting { game_content * pulse } else { 0.0 }),
        Some(&status),
        None,
    );

    // The countdown: its number popping each second, the label left of it.
    let counting = net.countdown().filter(|_| game);
    let left = counting.unwrap_or(0.0);
    let k = Ease::RegularOut.apply(((COUNTDOWN_SECS - left) / 0.5).clamp(0.0, 1.0));
    let alpha = if counting.is_some() { c * k } else { 0.0 };
    let rise = 10.0 * (1.0 - k);
    let number = format!(" {}", left.ceil() as u32);
    let pop = 2.0 - Ease::RegularOut.apply(((now - state.countdown_at) / 0.5).clamp(0.0, 1.0));
    let w = text.width(DEMI_BOLD, &number, COUNTDOWN_SIZE);
    let size = COUNTDOWN_SIZE * pop;
    put_text(
        parts,
        ui.countdown_number,
        view,
        COUNTDOWN_ORIGIN.x + pop * (1.0 - w),
        COUNTDOWN_ORIGIN.y + rise - 14.0 * pop + size * ASCENT,
        size,
        w * pop,
        COUNTDOWN_NUMBER.with_alpha(alpha),
        Some(&number),
        None,
    );
    let label = text
        .get("Lobby.MultiplayerCountdownLabel", "Match starts in")
        .to_uppercase();
    let lw = text.width(DEMI_BOLD, &label, COUNTDOWN_SIZE);
    put_text(
        parts,
        ui.countdown_label,
        view,
        COUNTDOWN_ORIGIN.x + 1.0 - (w + 4.0) - lw,
        COUNTDOWN_ORIGIN.y + rise - 14.0 + COUNTDOWN_SIZE * ASCENT,
        COUNTDOWN_SIZE,
        lw,
        LOBBY_TEAL.with_alpha(alpha),
        Some(&label),
        None,
    );

    // Connecting to Host: the band, its words, the spinning icon by them.
    let alpha = if connecting {
        c * intro(0.0, LOBBY_CONTENT_SECS)
    } else {
        0.0
    };
    put_image(
        parts,
        ui.band,
        view,
        Rect::from_center_size(PROMPT_CENTER, CONNECTING_BAND),
        Color::srgba(0.8, 1.0, 1.0, 0.3 * alpha),
        None,
    );
    let label = text
        .get("Lobby.ConnectingToHost", "Connecting to Host")
        .to_uppercase();
    let w = text.width(LIGHT, &label, CONNECTING_SIZE);
    put_text(
        parts,
        ui.connecting,
        view,
        PROMPT_CENTER.x - w * 0.5,
        CONNECTING_TOP + CONNECTING_SIZE * ASCENT,
        CONNECTING_SIZE,
        w,
        CONNECTING_COLOR.with_alpha(alpha),
        Some(&label),
        None,
    );
    put_image_turned(
        parts,
        ui.spinner,
        view,
        Rect::from_center_size(
            Vec2::new(PROMPT_CENTER.x - w * 0.5 - 26.0, PROMPT_CENTER.y),
            Vec2::splat(32.0),
        ),
        Color::WHITE.with_alpha(alpha),
        (now * 360.0).rem_euclid(360.0),
    );

    // Back.
    put_image(
        parts,
        ui.back_box,
        view,
        LOBBY_BACK,
        BACK_FILL.with_alpha(c),
        None,
    );
    let back_color = if state.lobby_back_over {
        BACK_TEXT_OVER
    } else {
        BACK_TEXT
    };
    put_text(
        parts,
        ui.back_text,
        view,
        LOBBY_BACK.center().x - ui.back_width * 0.5,
        LOBBY_BACK.center().y + BACK_TEXT_SIZE * 0.35,
        BACK_TEXT_SIZE,
        ui.back_width,
        back_color.with_alpha(c),
        None,
        None,
    );
}

/// A prompt over the menus: the screen darkens and the box opens out between its
/// bars; its icon, title, text, input and buttons fade in.
fn layout_prompt(
    parts: &mut Parts,
    view: &View,
    ui: &PromptUi,
    state: &MenuState,
    text: &MenuText,
    now: f32,
) {
    let shown = if state.prompt.is_some() { 1.0 } else { 0.0 };
    let prompt = state.prompt.clone().unwrap_or(Prompt::Address);
    let entry = prompt.typed();
    let layout = prompt_layout(&prompt, text);
    let t = now - state.prompt_at;
    let open = Ease::RegularOut.apply((t / PROMPT_OPEN_SECS).clamp(0.0, 1.0));
    let content = shown * Ease::RegularOut.apply((t / PROMPT_CONTENT_SECS).clamp(0.0, 1.0));

    // The screen, darkened.
    let screen = Rect::from_corners(
        -view.offset / view.scale,
        (view.window - view.offset) / view.scale,
    );
    put_image(
        parts,
        ui.dim,
        view,
        screen,
        Color::WHITE.with_alpha(shown * open),
        None,
    );

    // The box, and its bars (the top one from 8 pixels over the box's top, the
    // bottom one from its bottom).
    let half = Vec2::new(
        PROMPT_W * 0.5 * (0.036 + 0.964 * open),
        10.0 + (layout.box_rect.half_size().y - 10.0) * open,
    );
    let r = Rect::from_center_half_size(PROMPT_CENTER, half);
    put_image(
        parts,
        ui.box_fill,
        view,
        r,
        Color::WHITE.with_alpha(shown),
        None,
    );
    let bars = [
        Rect::new(r.min.x, r.min.y - 8.0, r.max.x, r.min.y + 2.0),
        Rect::new(r.min.x, r.max.y, r.max.x, r.max.y + 10.0),
    ];
    for (part, bar) in ui.bars.into_iter().zip(bars) {
        put_image(parts, part, view, bar, LOBBY_BLUE.with_alpha(shown), None);
    }

    // The icon, title and text.
    let (icon, tint, icon_half) = if entry {
        (0, PROMPT_ICON_TINT, PROMPT_ICON_HALF)
    } else {
        (1, PROMPT_ALERT_TINT, PROMPT_ALERT_HALF)
    };
    set_image(parts, ui.icon, &ui.icon_images[icon], false);
    put_image(
        parts,
        ui.icon,
        view,
        Rect::from_center_half_size(layout.icon, icon_half),
        tint.with_alpha(content),
        None,
    );
    let (join_title, message_title) = match &prompt {
        Prompt::Address => (
            text.get("OnlineFriends.JoinGame", "Join Game"),
            String::new(),
        ),
        Prompt::DisplayName => ("Display Name".to_string(), String::new()),
        Prompt::HostPassword | Prompt::JoinPassword { .. } => (
            text.get("Engine.AccessControl.GamePassword", "Game Password"),
            String::new(),
        ),
        Prompt::Message { title, .. } | Prompt::Confirm { title, .. } => {
            (String::new(), title.clone())
        }
    };
    let title_baseline = layout.title.y + PROMPT_TITLE_SIZE * ASCENT;
    put_text(
        parts,
        ui.join_title,
        view,
        layout.title.x,
        title_baseline,
        PROMPT_TITLE_SIZE,
        0.0,
        JOIN_TITLE.with_alpha(if entry { content } else { 0.0 }),
        Some(&join_title),
        None,
    );
    put_text(
        parts,
        ui.message_title,
        view,
        layout.title.x,
        title_baseline,
        PROMPT_TITLE_SIZE,
        0.0,
        Color::WHITE.with_alpha(if entry { 0.0 } else { content }),
        Some(&message_title),
        None,
    );
    let (size, color) = if entry {
        // (A wrong password's warning in orange, as a message's text.)
        let wrong = prompt == Prompt::JoinPassword { wrong: true };
        (
            JOIN_TEXT_SIZE,
            if wrong { LOBBY_ORANGE } else { LOBBY_BLUE },
        )
    } else {
        (MESSAGE_TEXT_SIZE, LOBBY_ORANGE)
    };
    put_text(
        parts,
        ui.body,
        view,
        layout.body.x,
        layout.body.y + size * ASCENT,
        size,
        0.0,
        color.with_alpha(content),
        Some(&layout.lines),
        None,
    );

    // The text input (Join Game's, Display Name's and Game Password's): the box,
    // what is typed (its end kept in view; a password as asterisks) and the caret,
    // blinking.
    let input_alpha = if entry { content } else { 0.0 };
    let input = layout.input.unwrap_or(Rect::new(0.0, 0.0, 1.0, 1.0));
    let inner = Rect::new(
        input.min.x + 1.0,
        input.min.y + 1.0,
        input.max.x - 1.0,
        input.max.y - 1.0,
    );
    put_image(
        parts,
        ui.input_border,
        view,
        input,
        JOIN_INPUT_BORDER.with_alpha(input_alpha),
        None,
    );
    put_image(
        parts,
        ui.input_fill,
        view,
        inner,
        Color::WHITE.with_alpha(input_alpha),
        None,
    );
    put_image(
        parts,
        ui.input_strip,
        view,
        Rect::new(inner.min.x, inner.min.y, inner.max.x, inner.min.y + 4.0),
        Color::srgb(0.92, 0.92, 0.92).with_alpha(input_alpha),
        None,
    );
    let inside = put_clip(parts, ui.input_clip, view, inner, input_alpha > 0.003);
    let typed = if prompt.password() {
        "*".repeat(state.password_entry.len())
    } else if prompt == Prompt::DisplayName {
        state.name_entry.clone()
    } else {
        state.address.clone()
    };
    let w = text.width(DEMI_BOLD, &typed, JOIN_TEXT_SIZE);
    let room = inner.width() - 10.0;
    let x = input.min.x + 6.0 - (w - room).max(0.0);
    put_text(
        parts,
        ui.input_text,
        &inside,
        x,
        input.min.y + 1.8 + JOIN_TEXT_SIZE * ASCENT,
        JOIN_TEXT_SIZE,
        w,
        JOIN_INPUT_TEXT.with_alpha(input_alpha),
        Some(&typed),
        None,
    );
    let blink = if t.rem_euclid(1.0) < 0.5 { 1.0 } else { 0.0 };
    let caret = x + w + 1.0;
    put_image(
        parts,
        ui.caret,
        &inside,
        Rect::new(caret, input.min.y + 3.0, caret + 1.0, input.max.y - 3.0),
        JOIN_INPUT_TEXT.with_alpha(input_alpha * blink),
        None,
    );

    // The buttons: the focused one orange with the bar under it, the others dark
    // (PROMPT_BUTTON_ON).
    let focus = state
        .prompt_focus
        .min(layout.buttons.len().saturating_sub(1));
    for (i, [button, label_part, bar]) in ui.buttons.iter().copied().enumerate() {
        let (r, label) = layout
            .buttons
            .get(i)
            .cloned()
            .unwrap_or((Rect::new(0.0, 0.0, 1.0, 1.0), String::new()));
        let alpha = if i < layout.buttons.len() {
            content
        } else {
            0.0
        };
        let on = i == focus;
        let fill = Rect::new(r.min.x, r.min.y, r.max.x, r.min.y + PROMPT_BUTTON_FILL_H);
        if on {
            set_image(parts, button, &ui.button_on, false);
            put_image(
                parts,
                button,
                view,
                fill,
                Color::WHITE.with_alpha(alpha),
                None,
            );
        } else {
            set_image(parts, button, &Handle::default(), false);
            put_image(parts, button, view, fill, BACK_FILL.with_alpha(alpha), None);
        }
        let under = Rect::new(
            r.min.x,
            r.min.y + PROMPT_BAR.0,
            r.max.x,
            r.min.y + PROMPT_BAR.1,
        );
        put_image(
            parts,
            bar,
            view,
            under,
            Color::WHITE.with_alpha(if on { alpha } else { 0.0 }),
            None,
        );
        let w = text.width(MEDIUM, &label, PROMPT_LABEL_SIZE);
        let color = if on { PROMPT_LABEL_ON } else { BACK_TEXT };
        put_text(
            parts,
            label_part,
            view,
            fill.center().x - w * 0.5,
            fill.center().y + PROMPT_LABEL_SIZE * 0.35,
            PROMPT_LABEL_SIZE,
            w,
            color.with_alpha(alpha),
            Some(&label),
            None,
        );
    }
}

/// The Extras menu (ExtrasMenu_GFX's mc_rootMenu): its title sliding in with the
/// focused button's hint under it, the selection bar, and the buttons.
fn layout_extras(
    parts: &mut Parts,
    view: &View,
    ui: &ExtrasUi,
    state: &MenuState,
    text: &MenuText,
    now: f32,
) {
    let shown = state.page == Page::Extras;
    let alpha = if shown { 1.0 } else { 0.0 };

    // The title and the hint (title_mc).
    let k = Ease::RegularOut.apply(((now - state.extras_at) / EXTRAS_TITLE_SECS).clamp(0.0, 1.0));
    let x = EXTRAS_MENU.x + EXTRAS_TITLE.x - EXTRAS_TITLE_SLIDE * (1.0 - k);
    let top = EXTRAS_MENU.y + EXTRAS_TITLE.y;
    put_text(
        parts,
        ui.title,
        view,
        x,
        top + EXTRAS_TITLE_SIZE * ASCENT,
        EXTRAS_TITLE_SIZE,
        0.0,
        Color::WHITE.with_alpha(k * alpha),
        None,
        None,
    );
    let (_, _, hint_key, hint_fallback) = EXTRAS_ITEMS[state.extras_focus].text();
    let hint = text.get(hint_key, hint_fallback);
    put_text(
        parts,
        ui.hint,
        view,
        x,
        top + EXTRAS_HINT_Y + EXTRAS_HINT_SIZE * ASCENT,
        EXTRAS_HINT_SIZE,
        0.0,
        EXTRAS_HINT.with_alpha(k * alpha),
        Some(&hint),
        None,
    );

    // The selection bar (changeHover's animate: from a quarter of its width and
    // half opacity).
    let b = Ease::RegularOut.apply(((now - state.extras_bar_at) / EXTRAS_BAR_SECS).clamp(0.0, 1.0));
    let width = 256.0 * EXTRAS_BAR_SCALE.x * (0.25 + 0.75 * b);
    let half_h = 16.0 * EXTRAS_BAR_SCALE.y;
    let cy = EXTRAS_MENU.y + state.extras_bar_y.get(now);
    let left = EXTRAS_MENU.x + EXTRAS_BAR_X;
    put_image(
        parts,
        ui.bar,
        view,
        Rect::new(left, cy - half_h, left + width, cy + half_h),
        Color::WHITE.with_alpha((0.5 + 0.5 * b) * alpha),
        None,
    );

    // The buttons' labels.
    for (i, entity) in ui.rows.iter().enumerate() {
        let f = state.extras_row_focus[i].get(now);
        let (u, o) = (EXTRAS_LABEL_UP.to_srgba(), EXTRAS_LABEL_OVER.to_srgba());
        let color = Color::srgba(
            u.red + (o.red - u.red) * f,
            u.green + (o.green - u.green) * f,
            u.blue + (o.blue - u.blue) * f,
            alpha,
        );
        let top = extras_row_top(i) + EXTRAS_LABEL_Y;
        put_text(
            parts,
            *entity,
            view,
            EXTRAS_MENU.x + EXTRAS_LABEL_X * (1.0 - f),
            top + EXTRAS_LABEL_SIZE * ASCENT,
            EXTRAS_LABEL_SIZE,
            0.0,
            color,
            None,
            None,
        );
    }
}

/// The pause menu's shell (mc_multiplayerShell): the Teletraan logo with its
/// hexagons, glow and pips on the left, the menu on the right, all animating in as
/// the menu opens. Hidden while the settings are open from it.
fn layout_pause(
    parts: &mut Parts,
    view: &View,
    ui: &PauseUi,
    state: &MenuState,
    text: &MenuText,
    now: f32,
) {
    let shown = state.page == Page::Pause;
    let t = now - state.pause_at;
    let alpha = if shown { 1.0 } else { 0.0 };
    let logo_origin = PAUSE_SHELL + Vec2::new(PAUSE_LOGO_X, 0.0);
    let out = |secs: f32| Ease::RegularOut.apply((t / secs).clamp(0.0, 1.0));

    // Behind the logo: the warped hexagons and the emblem's glow (teal).
    let k = out(PAUSE_BACK_SECS);
    let half = 300.0 * (1.133 + (1.0 - 1.133) * k);
    for (entity, (side, _, _)) in ui.hexagons.iter().zip(PAUSE_HEX_QUARTERS) {
        put_image(
            parts,
            *entity,
            view,
            Rect::from_corners(logo_origin, logo_origin + side * half),
            Color::srgba(0.0, 1.0, 1.0, 0.1016 * k * alpha),
            None,
        );
    }
    let frame = (t * 30.0).clamp(0.0, 24.0);
    let mut key = PAUSE_GLOW_KEYS[PAUSE_GLOW_KEYS.len() - 1];
    for pair in PAUSE_GLOW_KEYS.windows(2) {
        let (a, b) = (pair[0], pair[1]);
        if frame <= b.0 {
            let u = (frame - a.0) / (b.0 - a.0);
            let mix = |x: f32, y: f32| x + (y - x) * u;
            key = (
                frame,
                mix(a.1, b.1),
                mix(a.2, b.2),
                mix(a.3, b.3),
                mix(a.4, b.4),
            );
            break;
        }
    }
    let (_, gx, gy, ga, gb) = key;
    put_image(
        parts,
        ui.glow,
        view,
        Rect::from_corners(
            logo_origin + PAUSE_GLOW.min * Vec2::new(gx, gy),
            logo_origin + PAUSE_GLOW.max * Vec2::new(gx, gy),
        ),
        Color::srgba(0.0, 1.0, gb, ga * alpha),
        None,
    );

    // The emblem slides away and fades; the logo slides into place.
    let k = out(PAUSE_SHELL_SECS);
    let emblem_at = logo_origin + Vec2::new(-40.0 * k, 0.0);
    put_image(
        parts,
        ui.emblem,
        view,
        Rect::from_corners(emblem_at + PAUSE_EMBLEM.min, emblem_at + PAUSE_EMBLEM.max),
        Color::WHITE.with_alpha((1.0 - k) * alpha),
        None,
    );
    let logo_at = logo_origin + Vec2::new(145.0 + (20.0 - 145.0) * k, 15.0);
    put_image(
        parts,
        ui.logo,
        view,
        Rect::from_center_size(logo_at, PAUSE_LOGO_SIZE),
        Color::WHITE.with_alpha(alpha),
        None,
    );
    put_text(
        parts,
        ui.caption,
        view,
        logo_at.x - 89.0 + 2.0,
        logo_at.y + 13.0 + 2.0 + PAUSE_CAPTION_SIZE * ASCENT,
        PAUSE_CAPTION_SIZE,
        0.0,
        PAUSE_CAPTION.with_alpha(alpha),
        None,
        None,
    );

    // The pips close in from four times their width.
    let sx = 4.0 + (1.0 - 4.0) * k;
    for (entity, (from, to, fade)) in ui.pips.iter().zip(PAUSE_PIPS) {
        let at = logo_origin + from + (to - from) * k;
        let a = match fade {
            1 => k,
            -1 => 1.0 - k,
            _ => 1.0,
        };
        put_image(
            parts,
            *entity,
            view,
            Rect::from_center_size(at, Vec2::new(16.0 * sx, 4.0)),
            Color::srgba(0.0, 1.0, 211.0 / 255.0, a * alpha),
            None,
        );
    }

    // The menu: its background art, selection bar, title and rows, drifting with
    // the focus.
    let anchor = PAUSE_SHELL + Vec2::new(PAUSE_MENU_X, state.pause_drift.get(now));
    let k = out(PAUSE_ART_SECS);
    let art_x = anchor.x - 36.0 + 100.0 * (1.0 - k);
    let art_w = 256.0 * (2.734 + (3.125 - 2.734) * k);
    let art_h = 256.0 * 2.812;
    hold_slice_scale(parts, ui.art, view);
    put_image(
        parts,
        ui.art,
        view,
        Rect::new(
            art_x,
            PAUSE_SHELL.y - art_h * 0.5,
            art_x + art_w,
            PAUSE_SHELL.y + art_h * 0.5,
        ),
        Color::WHITE.with_alpha(0.1016 * k * alpha),
        None,
    );
    let bar_cy = anchor.y + state.pause_bar_y.get(now);
    let bar_half = Vec2::new(0.0, 16.0 * PAUSE_BAR_SCALE.y);
    put_image(
        parts,
        ui.bar,
        view,
        Rect::new(
            anchor.x - 24.0,
            bar_cy - bar_half.y,
            anchor.x - 24.0 + 256.0 * PAUSE_BAR_SCALE.x,
            bar_cy + bar_half.y,
        ),
        Color::WHITE.with_alpha(alpha),
        None,
    );
    let title = state.pause_title(now);
    put_text(
        parts,
        ui.title,
        view,
        anchor.x - 5.0 + 2.0 + 2.0,
        anchor.y + state.pause_title_y.get(now) - 37.0 + 2.0 + 2.0 + SET_TITLE_SIZE * ASCENT,
        SET_TITLE_SIZE,
        0.0,
        Color::WHITE.with_alpha(alpha),
        Some(&title),
        None,
    );
    // (Each item's row where this menu has it: Return to Lobby only in the host's.)
    let items = state.pause_items();
    for (item, entity) in HOST_PAUSE_ITEMS.iter().zip(&ui.rows) {
        let row = items.iter().position(|i| i == item);
        let i = row.unwrap_or(0);
        let f = state.pause_row_focus[i].get(now);
        let y = anchor.y + state.pause_row_offset(i);
        let (key, fallback) = item.label();
        let label = text.get(key, fallback);
        let color = Color::srgba(
            0.4 + (0.4 * 0.0586 - 0.4) * f,
            1.0 + (0.281 - 1.0) * f,
            228.0 / 255.0 * (1.0 + (0.281 - 1.0) * f),
            if row.is_some() { alpha } else { 0.0 },
        );
        put_text(
            parts,
            *entity,
            view,
            anchor.x - 5.0 * f + 2.0 + 2.0,
            y + 2.0 + 2.0 + SET_LABEL_SIZE * ASCENT,
            SET_LABEL_SIZE,
            0.0,
            color,
            Some(&label),
            None,
        );
    }
}

/// A button hint: the gamepad button's icon, then its action, centred on `center`.
fn hint(
    parts: &mut Parts,
    view: &View,
    icon: Entity,
    label: Entity,
    label_width: f32,
    center: Vec2,
    alpha: f32,
) {
    let gap = 5.0;
    let total = HINT_ICON + gap + label_width;
    let x = center.x - total * 0.5;
    put_image(
        parts,
        icon,
        view,
        Rect::new(
            x,
            center.y - HINT_ICON * 0.5,
            x + HINT_ICON,
            center.y + HINT_ICON * 0.5,
        ),
        Color::WHITE.with_alpha(alpha),
        None,
    );
    put_text(
        parts,
        label,
        view,
        x + HINT_ICON + gap,
        center.y + HINT_SIZE * 0.25,
        HINT_SIZE,
        label_width,
        HINT_COLOR.with_alpha(alpha),
        None,
        None,
    );
}

// ---------------------------------------------------------------------------
// Into the game
// ---------------------------------------------------------------------------

/// The announcer's line naming the game type as a match starts: "Escalation"
/// (TnGameTypeMessageSV's GameTypeDialog, MP_GameTypeEscalationDialog, played as
/// this cue by the announcer's sound event set; a 2D dialog line). The cue holds a
/// line for each side and the game plays the player's side's: Optimus Prime's for
/// the Autobots, and Megatron's on the Decepticon maps (MapInfo::decepticons:
/// Downfall and Oblivion), which build_sounds.py writes as a cue of its own.
const GAME_TYPE_DIALOG: &str = "BL_DX_SWITCHBOARD.SC009019";
const GAME_TYPE_DIALOG_DECEPTICONS: &str = "BL_DX_SWITCHBOARD.SC009019_MGTRON";

/// The music a match starts with (TnGameTypeMessageSV's GameTypeMusic): one of 13
/// stingers from the campaign's score, picked at random, starting a second in.
const GAME_TYPE_MUSIC: &str = "BL_LVL_MP.MX_MATCH_BEGIN";

/// The game type's name, announced on the HUD as a match starts (TnGameTypeMessage's
/// GameTypeMessage, shown with TnHUD.LocalizedGameAnnouncement). The other game types
/// name themselves there ("Team Deathmatch", "Conquest"...); the game leaves
/// Escalation's empty, so the demo uses the mode's own name from TransGame.int.
const GAME_TYPE_MESSAGE: &str = "Escalation";

/// The match starts as the game starts it (TnGameRules.HandleStartGame,
/// TnGameTypeMessage.ClientReceive): the announcer names the game type (Megatron
/// on a Decepticon map, `map` being the one in play), the HUD announces it and the
/// match-start music plays. (That is as the countdown to it runs out, once the
/// player has chosen his character: pregame.rs.)
pub(crate) fn announce_match(
    sfx: &mut Sfx,
    huds: [&mut crate::hud::HudState; crate::split::SEATS],
    map: Option<usize>,
) {
    let decepticons = map
        .and_then(|m| crate::MAPS.get(m))
        .is_some_and(|m| m.decepticons);
    sfx.play(if decepticons {
        GAME_TYPE_DIALOG_DECEPTICONS
    } else {
        GAME_TYPE_DIALOG
    });
    sfx.play(GAME_TYPE_MUSIC);
    // (On every HUD of this PC's: split screen's guest's too.)
    for hud in huds {
        hud.events
            .push(crate::hud::HudEvent::GameAnnouncement(GAME_TYPE_MESSAGE));
    }
}

/// The game starts: lock the mouse for it, stop the menu's movie and music, and
/// leave only the black fader, which fades out over the game. (The match itself
/// starts after the Choose Character screen and the countdown: pregame.rs.)
#[allow(clippy::too_many_arguments)]
fn enter_game(
    mut commands: Commands,
    time: Res<Time>,
    mut cursor: Single<&mut CursorOptions, With<PrimaryWindow>>,
    ui: Option<Res<MenuUi>>,
    mut state: Option<ResMut<MenuState>>,
    mut music: ResMut<MenuMusic>,
    mut movie: ResMut<Movie>,
    mut visibility: Query<&mut Visibility>,
) {
    cursor.grab_mode = CursorGrabMode::Locked;
    cursor.visible = false;
    for voice in music.voices.drain(..).flatten() {
        commands.entity(voice).try_despawn();
    }
    music.music = None;
    movie.frames = None; // ends the decoding thread
    let now = time.elapsed_secs();
    if let Some(state) = state.as_mut() {
        state
            .fade
            .from_to(now, 1.0, 0.0, FADE_SECS, Ease::RegularOut);
        state.page = Page::Game;
        state.leaving = None;
        state.game_at = None;
        state.gone = true;
        state.settings_from_lobby = false;
        state.prompt = None;
    }
    if let Some(ui) = ui {
        let all = [
            ui.background,
            ui.logo,
            ui.summary,
            ui.hint_icon,
            ui.hint_text,
        ]
        .into_iter()
        .chain(ui.labels.iter().copied())
        .chain(ui.brackets.iter().flatten().copied())
        .chain(ui.summary_glow.iter().copied());
        for e in all {
            if let Ok(mut v) = visibility.get_mut(e) {
                *v = Visibility::Hidden;
            }
        }
        // The root's black backing goes too, so the game shows through the fade.
        commands
            .entity(ui.root)
            .insert(BackgroundColor(Color::NONE));
    }
}

/// Fullscreen Mode, Resolution and VSync on the window, as they are accepted and
/// as the demo starts (from the saved settings): fullscreen in the Resolution's
/// video mode (exclusive, as the game's is: the primary monitor's mode that size
/// with the fastest refresh) or, at the desktop's, borderless; a window that
/// size, or as it is; each frame shown with vsync or as soon as it is done.
fn apply_display(
    (fullscreen, resolution, vsync): (Res<Fullscreen>, Res<Resolution>, Res<VSync>),
    monitors: Query<(&Monitor, Has<PrimaryMonitor>)>,
    mut window: Single<&mut Window, With<PrimaryWindow>>,
    mut applied: Local<Option<(bool, Option<UVec2>, bool, bool)>>,
) {
    let modes = monitors
        .iter()
        .max_by_key(|(_, primary)| *primary)
        .map_or(&[][..], |(m, _)| m.video_modes.as_slice());
    let mode = resolution.0.and_then(|size| {
        modes
            .iter()
            .filter(|m| m.physical_size == size)
            .max_by_key(|m| (m.refresh_rate_millihertz, m.bit_depth))
            .cloned()
    });
    // (A fullscreen resolution waits for the monitor's modes to be known.)
    if fullscreen.0 && resolution.0.is_some() && modes.is_empty() {
        return;
    }
    let key = (fullscreen.0, resolution.0, vsync.0, mode.is_some());
    if *applied == Some(key) {
        return;
    }
    *applied = Some(key);
    let want = match (fullscreen.0, mode) {
        (true, Some(m)) => {
            WindowMode::Fullscreen(MonitorSelection::Current, VideoModeSelection::Specific(m))
        }
        (true, None) => WindowMode::BorderlessFullscreen(MonitorSelection::Current),
        (false, _) => WindowMode::Windowed,
    };
    if window.mode != want {
        window.mode = want;
    }
    if !fullscreen.0
        && let Some(size) = resolution.0
        && window.physical_size() != size
    {
        window.resolution.set_physical_resolution(size.x, size.y);
    }
    let present = if vsync.0 {
        PresentMode::AutoVsync
    } else {
        PresentMode::Mailbox
    };
    if window.present_mode != present {
        window.present_mode = present;
    }
}

/// The primary monitor's resolutions, for the Resolution slider (its video modes'
/// sizes, each once, smallest first).
fn list_resolutions(
    monitors: Query<(&Monitor, Has<PrimaryMonitor>)>,
    mut list: ResMut<Resolutions>,
) {
    let Some((monitor, _)) = monitors.iter().max_by_key(|(_, primary)| *primary) else {
        return;
    };
    let sizes = resolution_list(&monitor.video_modes);
    if list.0 != sizes {
        list.0 = sizes;
    }
}

/// Video modes' sizes, each once, smallest first, none under MIN_RESOLUTION.
fn resolution_list(modes: &[VideoMode]) -> Vec<UVec2> {
    let mut sizes: Vec<UVec2> = modes
        .iter()
        .map(|m| m.physical_size)
        .filter(|s| s.x >= MIN_RESOLUTION.x && s.y >= MIN_RESOLUTION.y)
        .collect();
    sizes.sort_by_key(|s| (s.x * s.y, s.x));
    sizes.dedup();
    sizes
}

/// Any setting changed in the menus is saved to settings.txt (not as the demo
/// starts, when they are read from it).
#[allow(clippy::too_many_arguments)]
fn save_changed_settings(
    volumes: Res<Volumes>,
    fullscreen: Res<Fullscreen>,
    cap: Res<FramerateCap>,
    controls: Res<Controls>,
    display: Res<DisplaySettings>,
    (quality, resolution, vsync, render_scale): (
        Res<GraphicsQuality>,
        Res<Resolution>,
        Res<VSync>,
        Res<RenderScale>,
    ),
    mut saved: Local<Option<Controls>>,
    mut saved_display: Local<Option<DisplaySettings>>,
) {
    // (The Graphics level, Resolution, VSync and Render Scale on lines of their
    // own.)
    let mut graphics = Vec::new();
    if quality.is_changed() && !quality.is_added() {
        graphics.push(quality.setting());
    }
    if resolution.is_changed() && !resolution.is_added() {
        graphics.push(resolution.setting());
    }
    if vsync.is_changed() && !vsync.is_added() {
        graphics.push(("vsync".to_string(), u8::from(vsync.0).to_string()));
    }
    if render_scale.is_changed() && !render_scale.is_added() {
        graphics.push(render_scale.setting());
    }
    if !graphics.is_empty() {
        write_settings(&graphics, None);
    }
    let changed = |added: bool, changed: bool| changed && !added;
    if changed(volumes.is_added(), volumes.is_changed())
        || changed(fullscreen.is_added(), fullscreen.is_changed())
        || changed(cap.is_added(), cap.is_changed())
    {
        save_settings(*volumes, *fullscreen, *cap, None);
    }
    // The Display settings, when they differ from those last saved or read.
    match *saved_display {
        Some(last) if last != *display => {
            write_settings(&display.settings(), None);
            *saved_display = Some(*display);
        }
        Some(_) => {}
        None => *saved_display = Some(*display),
    }
    // The controls (all their lines, the player's own keys in place of any saved
    // before), when they differ from those last saved or read.
    match saved.as_ref() {
        None => *saved = Some(controls.clone()),
        Some(last) if *last != *controls => {
            write_settings(&controls.settings(), Some("bind_"));
            *saved = Some(controls.clone());
        }
        Some(_) => {}
    }
}

/// The Framerate Cap: each frame ends no sooner than 1/cap seconds after the one
/// before (sleeping, then spinning out the last moment, which sleep can't time
/// finely). The frames keep to that schedule, so a frame a little late is made up
/// by a shorter wait for the next; one more than a whole frame late starts the
/// schedule again from now rather than rushing several to catch up.
fn limit_framerate(cap: Res<FramerateCap>, mut last: Local<Option<Instant>>) {
    let Some(fps) = cap.fps() else {
        *last = None;
        return;
    };
    const SPIN: Duration = Duration::from_micros(1500);
    let frame = Duration::from_secs_f64(1.0 / f64::from(fps));
    let now = Instant::now();
    let Some(target) = last.map(|t| t + frame) else {
        *last = Some(now);
        return;
    };
    if target > now {
        let wait = target - now;
        if wait > SPIN {
            std::thread::sleep(wait - SPIN);
        }
        while Instant::now() < target {
            std::hint::spin_loop();
        }
        *last = Some(target);
    } else if now - target > frame {
        *last = Some(now);
    } else {
        *last = Some(target);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const PAD_MAP: &str = "# made by tools/build_menu.py
picture pad_controller.png 184 72 512 256
label XboxTypeS_LeftShoulder pad_lb.png 256 33 27 27 right -143.3 256.7 32.15 16 81ffe7
label XboxTypeS_LeftThumbstick pad_ls.png 256 132 27 27 right -143.3 256.7 132 16 81ffe7
label XboxTypeS_RightX pad_right_stick.png 482 170 141 55 left 623.9 1073.9 208.85 16 ffffff
none 0 600 70 18 66ffe4
";

    #[test]
    fn the_lobbies_have_offline_game_and_customize() {
        // Offline Game heads the party lobby whatever it offers of Prime Mode, with
        // Join Game (Find Game's place) under it.
        for prime in [PrimeOffer::Locked, PrimeOffer::Open, PrimeOffer::Gone] {
            let rows = lobby_rows(LobbyView::Party, false, prime);
            assert_eq!(&rows[..2], &[LobbyRow::OfflineGame, LobbyRow::JoinGame]);
            assert!(rows.len() <= LOBBY_MAX_ROWS);
        }
        // A game's lobby has Customize just above the difficulties, for the host and
        // for those who join.
        for host in [true, false] {
            let rows = lobby_rows(LobbyView::Game, host, PrimeOffer::Gone);
            let at = rows
                .iter()
                .position(|r| *r == LobbyRow::Customize)
                .expect("Customize");
            assert_eq!(rows[at + 1], LobbyRow::Difficulty);
            assert!(rows.len() <= LOBBY_MAX_ROWS);
        }
        // Offline Game and Customize can be used; Customize not once the countdown
        // has started.
        let mut net = Net::default();
        assert_eq!(LobbyRow::OfflineGame.state(&net), RowState::Enabled);
        net.host_offline();
        assert_eq!(LobbyRow::Customize.state(&net), RowState::Enabled);
        net.start_countdown();
        assert_eq!(LobbyRow::Customize.state(&net), RowState::Disabled);
    }

    /// The main menu's version: the installer's when it was built with one
    /// (FOC_ESC_VERSION), otherwise the package's, as a dev build.
    #[test]
    fn the_main_menu_names_its_version() {
        let label = version_label();
        match option_env!("FOC_ESC_VERSION").map(str::trim) {
            Some(v) if !v.is_empty() => assert_eq!(label, format!("Version {v}")),
            _ => assert_eq!(
                label,
                format!("Version {} (dev build)", env!("CARGO_PKG_VERSION"))
            ),
        }
    }

    /// Split screen in the party lobby: "Press START to join" while a controller
    /// other than the first player's is connected, Start on it bringing Player 2
    /// into the party, and the game the first player then hosts having him in as
    /// its lobby opens; he stays in the party back in the party lobby, and leaves it
    /// with B, his controller unplugged or the party lobby left for the main menu.
    #[test]
    fn player_2_joins_the_party_and_comes_into_a_hosted_game() {
        let mut world = bevy::ecs::world::World::new();
        let (first, second) = (world.spawn_empty().id(), world.spawn_empty().id());
        let (mut pad1, mut pad2) = (Gamepad::default(), Gamepad::default());
        let mut controls = Controls {
            controller: true,
            ..Controls::default()
        };
        let mut state = MenuState::new(0.0);
        state.page = Page::Lobby;
        state.lobby_view = LobbyView::Party;
        let mut split = crate::split::Split::default();
        let mut net = Net::default();
        let mut sfx = Sfx::default();
        let press = |g: &mut Gamepad, b: GamepadButton| {
            g.digital_mut().clear();
            g.digital_mut().press(b);
        };
        let calm = |g: &mut Gamepad| {
            g.digital_mut().release_all();
            g.digital_mut().clear();
        };

        // Only the first player's controller (Active Game Controls on Controller):
        // nothing to join on.
        party_guest(
            &mut state,
            &mut split,
            &[(first, &pad1)],
            &controls,
            &mut sfx,
        );
        assert!(!state.party_hint && !state.party_guest);
        // A second one: "Press START to join" under his name; Start on the first
        // player's own does nothing.
        press(&mut pad1, GamepadButton::Start);
        party_guest(
            &mut state,
            &mut split,
            &[(first, &pad1), (second, &pad2)],
            &controls,
            &mut sfx,
        );
        assert!(state.party_hint && split.party.is_none());
        calm(&mut pad1);
        // Start on the second: Player 2 is in the party, listed in its place.
        press(&mut pad2, GamepadButton::Start);
        party_guest(
            &mut state,
            &mut split,
            &[(first, &pad1), (second, &pad2)],
            &controls,
            &mut sfx,
        );
        assert_eq!(split.party, Some(second));
        assert!(state.party_guest && !state.party_hint);
        calm(&mut pad2);

        // Hosting (Offline Game here, Host Game the same): its lobby has him in as it
        // opens, on his controller.
        net.host_offline();
        state.lobby_view = LobbyView::Game;
        let pads = [(first, &pad1), (second, &pad2)];
        party_guest(&mut state, &mut split, &pads, &controls, &mut sfx);
        lobby_guest(&mut state, &mut net, &mut split, &pads, &controls, &mut sfx);
        assert_eq!(net.guest_slot(), Some(1));
        assert_eq!(net.player_count(), 2);
        assert_eq!(
            split.guest,
            Some(crate::split::Guest {
                gamepad: Some(second)
            })
        );
        assert_eq!(state.guest_hint, "");

        // Back to the party (the game left): he is still with the first player.
        net.leave();
        split.guest = None;
        state.lobby_view = LobbyView::Party;
        party_guest(&mut state, &mut split, &pads, &controls, &mut sfx);
        assert!(state.party_guest);

        // B on his controller takes him out of the party, and the prompt is back.
        press(&mut pad2, GamepadButton::East);
        party_guest(
            &mut state,
            &mut split,
            &[(first, &pad1), (second, &pad2)],
            &controls,
            &mut sfx,
        );
        assert!(split.party.is_none() && !state.party_guest);
        calm(&mut pad2);
        party_guest(
            &mut state,
            &mut split,
            &[(first, &pad1), (second, &pad2)],
            &controls,
            &mut sfx,
        );
        assert!(state.party_hint);

        // On the keyboard and mouse, one controller is enough to join on; its going
        // (unplugged) takes him out, as leaving the party lobby for the main menu does.
        controls.controller = false;
        press(&mut pad1, GamepadButton::Start);
        party_guest(
            &mut state,
            &mut split,
            &[(first, &pad1)],
            &controls,
            &mut sfx,
        );
        assert_eq!(split.party, Some(first));
        calm(&mut pad1);
        party_guest(&mut state, &mut split, &[], &controls, &mut sfx);
        assert!(split.party.is_none() && !state.party_hint);
        split.party = Some(first);
        state.page = Page::Main;
        party_guest(
            &mut state,
            &mut split,
            &[(first, &pad1)],
            &controls,
            &mut sfx,
        );
        assert!(split.party.is_none());
    }

    /// In a game's lobby, Start on a controller (the first player on the keyboard
    /// and mouse) brings Player 2 into the game and into the party too, and B on it
    /// takes him out of both.
    #[test]
    fn a_game_lobbys_guest_is_in_the_party_too() {
        let mut world = bevy::ecs::world::World::new();
        let pad_e = world.spawn_empty().id();
        let mut pad = Gamepad::default();
        let controls = Controls::default();
        let mut state = MenuState::new(0.0);
        state.page = Page::Lobby;
        state.lobby_view = LobbyView::Game;
        let mut split = crate::split::Split::default();
        let mut net = Net::default();
        let mut sfx = Sfx::default();
        net.host_offline();
        lobby_guest(
            &mut state,
            &mut net,
            &mut split,
            &[(pad_e, &pad)],
            &controls,
            &mut sfx,
        );
        assert_eq!(state.guest_hint, GUEST_JOIN_HINT);
        pad.digital_mut().press(GamepadButton::Start);
        lobby_guest(
            &mut state,
            &mut net,
            &mut split,
            &[(pad_e, &pad)],
            &controls,
            &mut sfx,
        );
        assert_eq!(net.guest_slot(), Some(1));
        assert_eq!(split.party, Some(pad_e));
        pad.digital_mut().release_all();
        pad.digital_mut().clear();
        pad.digital_mut().press(GamepadButton::East);
        lobby_guest(
            &mut state,
            &mut net,
            &mut split,
            &[(pad_e, &pad)],
            &controls,
            &mut sfx,
        );
        assert_eq!(net.guest_slot(), None);
        assert!(split.guest.is_none() && split.party.is_none());
    }

    #[test]
    fn exit_game_asks_with_continue_and_cancel() {
        let text = MenuText::default();
        let ask = |action| Prompt::Confirm {
            title: EXIT_GAME_TITLE.to_string(),
            text: EXIT_GAME_TEXT.to_string(),
            action,
        };
        let labels = |p: &Prompt| -> Vec<String> {
            prompt_layout(p, &text)
                .buttons
                .into_iter()
                .map(|(_, l)| l)
                .collect()
        };
        assert_eq!(
            labels(&ask(ConfirmAction::ExitGame)),
            ["Continue", "Cancel"]
        );
        assert_eq!(labels(&ask(ConfirmAction::QuitGame)), ["Yes", "No"]);
        assert_eq!(
            labels(&ask(ConfirmAction::PrimeMode)),
            ["Confirm", "Cancel"]
        );
        // (Left to right, the first left of the second, as the game sets them.)
        let layout = prompt_layout(&ask(ConfirmAction::ExitGame), &text);
        assert!(layout.buttons[0].0.max.x <= layout.buttons[1].0.min.x);
    }

    #[test]
    fn the_controller_map_reads_as_the_tool_writes_it() {
        let map = PadMap::parse(PAD_MAP);
        assert_eq!(
            map.picture,
            Some((
                "pad_controller.png".to_string(),
                Rect::new(184.0, 72.0, 696.0, 328.0)
            ))
        );
        assert_eq!(map.labels.len(), 3);
        let lb = &map.labels[0];
        assert_eq!(
            (lb.key.as_str(), lb.image.as_str(), lb.right),
            ("XboxTypeS_LeftShoulder", "pad_lb.png", true)
        );
        assert_eq!(lb.art, Rect::new(256.0, 33.0, 283.0, 60.0));
        assert_eq!((lb.text.x0, lb.text.x1, lb.text.y0), (-143.3, 256.7, 32.15));
        assert_eq!(lb.text.color, Color::srgb_u8(0x81, 0xff, 0xe7));
        assert!(!map.labels[2].right);
        let none = map.none.expect("the line said with no controller");
        assert_eq!((none.x1, none.y0, none.size), (600.0, 70.0, 18.0));
        // (The demo's own file, as the tool made it.)
        let path = crate::assets_dir().join("menu").join("padmap.txt");
        if path.exists() {
            let map = PadMap::load(&path);
            assert_eq!(map.labels.len(), 18);
            assert!(map.picture.is_some() && map.none.is_some());
        }
    }

    #[test]
    fn the_controller_labels_change_with_the_schemes_and_the_focus() {
        let mut state = MenuState::new(0.0);
        state.pad_map = PadMap::parse(PAD_MAP);
        state.pad_ready = PadReady {
            chosen: true,
            connected: true,
        };
        let said = |state: &MenuState| -> Vec<String> {
            state
                .pad_labels
                .labels
                .iter()
                .map(|l| l.text.clone())
                .collect()
        };
        // (Not showing, none.)
        state.update_pad_labels(0.5, [0; 3]);
        assert!(state.pad_labels.labels.is_empty());
        state.set_menu = SetMenu::ControllerLayout;
        state.set_focus = 0;
        state.update_pad_labels(1.0, [0; 3]);
        assert_eq!(
            said(&state),
            ["Dash(Press)/Sprint(Hold)", "Change Form", "Camera"]
        );
        assert!(state.pad_labels.labels.iter().all(|l| l.at == 1.0));
        // Charger: LB and LS trade; the camera stays as it was.
        state.update_pad_labels(2.0, [3, 0, 0]);
        assert_eq!(
            said(&state),
            ["Change Form", "Dash(Press)/Sprint(Hold)", "Camera"]
        );
        let at: Vec<f32> = state.pad_labels.labels.iter().map(|l| l.at).collect();
        assert_eq!(at, [2.0, 2.0, 1.0]);
        // Striker: LS has nothing left to say, and fades out.
        state.update_pad_labels(3.0, [2, 0, 0]);
        let ls = &state.pad_labels.labels[1];
        assert_eq!(
            (ls.text.as_str(), ls.leaving.clone()),
            ("", Some(("Dash(Press)/Sprint(Hold)".to_string(), 3.0)))
        );
        // The Thumbsticks row, Southpaw: only the sticks, swapped.
        state.set_focus = 2;
        state.update_pad_labels(4.0, [2, 0, 1]);
        assert_eq!(said(&state), ["", "", "Move"]);
        // No controller: the labels go, and the line says so.
        state.pad_ready.connected = false;
        state.update_pad_labels(5.0, [2, 0, 1]);
        assert_eq!(said(&state), ["", "", ""]);
        assert_eq!(state.pad_labels.none_at, 5.0);
        // Left and opened again, it starts afresh.
        state.set_menu = SetMenu::Controls;
        state.update_pad_labels(6.0, [0; 3]);
        state.set_menu = SetMenu::ControllerLayout;
        state.set_focus = 0;
        state.pad_ready.connected = true;
        state.update_pad_labels(7.0, [0; 3]);
        assert_eq!(
            said(&state),
            ["Dash(Press)/Sprint(Hold)", "Change Form", "Camera"]
        );
        assert!(
            state
                .pad_labels
                .labels
                .iter()
                .all(|l| l.at == 7.0 && l.leaving.is_none())
        );
    }

    #[test]
    fn a_controller_label_comes_in_a_little_larger_and_white() {
        let (near, white) = pad_label_in(0.0);
        assert!((near - 1.049).abs() < 0.002, "{near}");
        assert_eq!(white, 1.0);
        let (near, white) = pad_label_in(0.1);
        assert!(
            near > 1.0 && near < 1.035 && white > 0.5 && white < 0.7,
            "{near} {white}"
        );
        assert_eq!(pad_label_in(PAD_LABEL_IN_SECS), (1.0, 0.0));
    }

    #[test]
    fn resolutions_are_the_monitors_sizes_once_smallest_first() {
        let mode = |w: u32, h: u32, hz: u32| VideoMode {
            physical_size: UVec2::new(w, h),
            bit_depth: 32,
            refresh_rate_millihertz: hz * 1000,
        };
        let modes = [
            mode(1920, 1080, 144),
            mode(640, 480, 60),
            mode(1280, 720, 60),
            mode(1920, 1080, 60),
            mode(2560, 1440, 144),
            mode(1600, 900, 60),
        ];
        let sizes: Vec<(u32, u32)> = resolution_list(&modes).iter().map(|s| (s.x, s.y)).collect();
        assert_eq!(
            sizes,
            [(1280, 720), (1600, 900), (1920, 1080), (2560, 1440)]
        );
        // The slider: the sizes, then the desktop's.
        let list = resolution_list(&modes);
        let (volumes, controls) = (Volumes::default(), Controls::default());
        let source = |resolution| SliderSource {
            volumes: &volumes,
            controls: &controls,
            resolution,
            resolutions: &list,
            render_scale: 0.75,
        };
        let s = source(Some(UVec2::new(1920, 1080)));
        assert_eq!(Slider::Resolution.range(&s), (0.0, 4.0, 1.0));
        assert_eq!(Slider::Resolution.value(&s), 2.0);
        assert_eq!(Slider::Resolution.label(&s).as_deref(), Some("1920x1080"));
        let desktop = source(None);
        assert_eq!(Slider::Resolution.value(&desktop), 4.0);
        assert_eq!(
            Slider::Resolution.label(&desktop).as_deref(),
            Some("Desktop")
        );
        assert_eq!(Slider::RenderScale.label(&desktop).as_deref(), Some("75%"));
        // Kept in settings.txt as its size, or "desktop".
        assert_eq!(
            Resolution(Some(UVec2::new(1280, 720))).setting().1,
            "1280x720"
        );
        assert_eq!(Resolution(None).setting().1, "desktop");
    }

    #[test]
    fn brightness_sets_the_display_gamma() {
        let gamma = |brightness| {
            DisplaySettings {
                brightness,
                safe_frame: Vec2::ZERO,
                faction_colors: false,
                split_side_by_side: false,
            }
            .gamma()
        };
        // GetGammaSetting: 2.2 at the middle, 0.95 lower or higher at the ends.
        assert!((gamma(50.0) - 2.2).abs() < 1e-6);
        assert!((gamma(0.0) - 1.25).abs() < 1e-5);
        assert!((gamma(100.0) - 3.15).abs() < 1e-5);
    }

    #[test]
    fn team_coloring_mode_shows_both_ways() {
        let text = MenuText::load(&assets_dir().join("menu").join("menu.txt"));
        let values = Stepper::TeamColorMode.values(&text, choices_for_test(false));
        assert_eq!(values, ["Ally vs Enemy", "Autobot vs Decepticon"]);
        // Ally vs Enemy first, as the game's default (MpColoringMode 0).
        assert_eq!(Stepper::TeamColorMode.selected(choices_for_test(false)), 0);
        assert_eq!(Stepper::TeamColorMode.selected(choices_for_test(true)), 1);
    }

    fn choices_for_test(faction_colors: bool) -> Choices {
        Choices {
            fullscreen: true,
            vsync: false,
            cap: 1,
            quality: crate::graphics::HIGH,
            faction_colors,
            split_side: false,
            vibration: true,
            hold_boost: true,
            stick_delay: true,
            invert: [false; 3],
            controller: false,
            layout: 0,
            layouts: 1,
            view: Mode::ALL[0],
            pad: [0; 3],
        }
    }
}
