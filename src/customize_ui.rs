//! Draws the Customize screen (customize.rs) the way its movie lays it out, through
//! the Scaleform-style layer of flash.rs: the class select (menu_SelectCharacter),
//! the editor's page stack, lists, colour grid and info panels, the button hints,
//! the Randomize pulse, the rename prompt and the fades. Positions, sizes, colours
//! and timings are the movie's (CustomCharacter_GFX, stage pixels). As it draws it
//! records where the mouse can point (customize.rs's `Hit`s).

use crate::customize::{
    CustData, Customize, EditorUi, Field, Hint, Hit, InfoPanel, Phase, SLOTS, Swatch, Target,
};
use crate::flash::{
    Align, Canvas, Clip, Cx, Ease, FlashFont, FlashFrame, Glow, M3, STAGE_H, STAGE_W, TextSpec,
    key, text_width,
};
use crate::menu::MenuText;
use bevy::prelude::*;
use std::collections::HashMap;

/// The images (assets/customize/img, from tools/build_customize.py).
const IMAGES: [&str; 55] = [
    "slot_up",
    "slot_over",
    "slot_disabled",
    "class_plate",
    "xp_marker",
    "level_hex",
    "icon_scout",
    "icon_leader",
    "icon_soldier",
    "icon_scientist",
    "panel",
    "title_plate",
    "wheel_a",
    "wheel_b",
    "info_panel",
    "info_panel_large",
    "info_title",
    "sub_title",
    "upgrade_bar",
    "upgrade_arrow",
    "ruler",
    "stat_damage",
    "stat_range",
    "stat_accuracy",
    "stat_fire_rate",
    "stat_robot_speed",
    "stat_vehicle_speed",
    "stat_health",
    "bar_up",
    "bar_over",
    "lock",
    "scroll_track",
    "scroll_thumb",
    "scroll_thumb_over",
    "slider_arrow",
    "slider_bar",
    "slider_track",
    "swatch_frame",
    "swatch_frame_over",
    "xp_bar",
    "stat_bar",
    "chip_primary",
    "chip_secondary",
    "swatch_primary",
    "swatch_secondary",
    "spinner",
    "currency",
    "notify_bg",
    "notify_frame",
    // The hint bar's controller buttons (Hint::pad_button).
    "Gamepad_X",
    "Gamepad_Y",
    "Gamepad_B",
    "Gamepad_L3",
    "Gamepad_R3",
    "Gamepad_DRIGHT",
];

#[derive(Resource)]
pub(crate) struct CustImages {
    images: HashMap<&'static str, Handle<Image>>,
    /// Weapon and ability pictures by id (wire_<id>.png).
    wires: HashMap<String, Handle<Image>>,
}

impl CustImages {
    fn get(&self, name: &str) -> Handle<Image> {
        self.images.get(name).cloned().unwrap_or_default()
    }

    /// A weapon's or ability's picture (its ItemWireframe), if there is one.
    pub(crate) fn wire(&self, id: &str) -> Option<Handle<Image>> {
        self.wires.get(id).cloned()
    }
}

pub(crate) fn load_images(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    data: Option<Res<CustData>>,
) {
    let Some(data) = data else {
        return;
    };
    let images = IMAGES
        .iter()
        .map(|n| (*n, asset_server.load(format!("customize/img/{n}.png"))))
        .collect();
    let dir = crate::assets_dir().join("customize").join("img");
    let wires = data
        .weapons
        .iter()
        .chain(data.abilities.iter())
        .filter(|i| dir.join(format!("wire_{}.png", i.id)).exists())
        .map(|i| {
            (
                i.id.clone(),
                asset_server.load(format!("customize/img/wire_{}.png", i.id)),
            )
        })
        .collect();
    commands.insert_resource(CustImages { images, wires });
}

// ---------------------------------------------------------------------------
// Layout (the movie's numbers)
// ---------------------------------------------------------------------------

/// The class clips' x in menu_SelectCharacter (scout, leader, soldier, scientist),
/// and their drawing order (bottom first).
const CLASS_X: [f32; 4] = [-505.0, -266.0, -27.0, 212.0];
const CLASS_ORDER: [usize; 4] = [1, 2, 3, 0];
/// The slots' places when out, and where they fold to.
const SLOT_OUT: [(f32, f32); 3] = [(30.0, 73.0), (23.0, 99.0), (16.0, 125.0)];
const SLOT_IN: (f32, f32) = (30.0, 99.0);
const SLOT_XSCALE: f32 = 274.0 / 256.0;
const ORANGE: [f32; 3] = [255.0, 142.0, 77.0];
const BLUE: [f32; 3] = [32.0, 118.0, 145.0];
const LIGHT_ORANGE: [f32; 3] = [246.0, 223.0, 161.0];
const DARK_BLUE: [f32; 3] = [0.0, 24.0, 35.0];
const CREAM: [u8; 3] = [255, 235, 179];
const NAVY: [u8; 3] = [0, 24, 35];
const CYAN_TEXT: [u8; 3] = [139, 255, 255];
const TEAL_TEXT: [u8; 3] = [156, 239, 237];
const STAT_LABEL: [u8; 3] = [248, 160, 95];
const LOCK_TEXT: [u8; 3] = [165, 246, 255];
const TRACK: [f32; 3] = [97.0, 219.0, 219.0];
/// The hint bar (SharedComponents' mc_buttonHintBar on PC): 132 x 36 buttons, 136
/// apart, centred on x 560, their tops at 651.
const HINT_W: f32 = 132.0;
const HINT_H: f32 = 36.0;
const HINT_STEP: f32 = 136.0;
const HINT_TOP: f32 = 651.0;
const HINT_FILL: [f32; 3] = [15.0, 62.0, 77.0];
const HINT_FILL_OVER: [f32; 3] = [255.0, 147.0, 51.0];
const HINT_TEXT: [u8; 3] = [138, 240, 255];
/// Each button's label sits a little right of the button's middle, as it does in
/// the game.
const HINT_TEXT_SHIFT: f32 = 4.0;
/// While a controller plays, a hint's button picture goes before its label, as the
/// movie's hint renderer puts it (ButtonHintRenderer: the icon's markup, then the
/// label): this tall (the icons are square), this far from the label, the two
/// centred on the label's place; a label too long for the button with it
/// shrinks to fit, inside HINT_ICON_MARGIN of its ends.
const HINT_ICON: f32 = 22.0;
const HINT_ICON_GAP: f32 = 4.0;
const HINT_ICON_MARGIN: f32 = 4.0;
/// The 3DButton bar's scale-9 borders (left, top, right, bottom) in its 220 x 55
/// art, and as parts of its 256 x 64 texture.
const BAR_BORDERS: [f32; 4] = [42.0, 22.0, 58.0, 22.0];
const BAR_TEX: [f32; 4] = [42.0 / 220.0, 22.0 / 55.0, 58.0 / 220.0, 22.0 / 55.0];
/// The option lists' rows (3DListItemRenderer): CLIK sizes each to the list's
/// width less its scroll bar (275 - 16 = 259) by setting its `_width`, which scales
/// the row's whole 242 px bounds (the 220 px bar, and its hidden lock overlay
/// reaching 22 px to the left) about its left edge: the bar ends up this long.
const LIST_ROW_W: f32 = 220.0 * 259.0 / 242.0;
/// A frame of the currency sign's warning: scale, x, y, colour multiplier, alpha,
/// red and green offsets (0-255), shown.
type SignFrame = (f32, f32, f32, f32, f32, f32, f32, bool);

/// The currency sign's warning (currencySign_mc from its "animate" frame): frame by
/// frame (30 a second, from frame 1) its scale, x and y, its colour multiplier and
/// alpha, its red and green offsets, and whether it shows (it blinks at its
/// biggest).
const FUNDS_WARNING: [SignFrame; 45] = [
    (1.0, 0.0, 0.0, 1.0, 1.0, 0.0, 0.0, true),
    (1.0, 0.0, 0.0, 1.0, 1.0, 0.0, 0.0, true),
    (1.31, -2.0, -2.95, 0.765625, 0.953125, 60.0, 12.0, true),
    (1.58, -3.75, -5.5, 0.5625, 0.9140625, 112.0, 22.0, true),
    (1.81, -5.2, -7.65, 0.390625, 0.87890625, 155.0, 31.0, true),
    (2.0, -6.4, -9.4, 0.25, 0.8515625, 191.0, 38.0, true),
    (2.15, -7.35, -10.8, 0.140625, 0.828125, 219.0, 44.0, true),
    (2.25, -8.0, -11.75, 0.0625, 0.8125, 239.0, 48.0, true),
    (2.31, -8.4, -12.35, 0.015625, 0.8046875, 251.0, 50.0, true),
    (2.34, -8.55, -12.55, 0.0, 0.80078125, 255.0, 51.0, true),
    (2.34, -8.55, -12.55, 0.0, 0.80078125, 255.0, 51.0, true),
    (2.34, -8.55, -12.55, 0.0, 0.80078125, 255.0, 51.0, true),
    (2.34, -8.55, -12.55, 0.0, 0.80078125, 255.0, 51.0, true),
    (2.34, -8.55, -12.55, 0.0, 0.80078125, 255.0, 51.0, false),
    (2.34, -8.55, -12.55, 0.0, 0.80078125, 255.0, 51.0, true),
    (2.34, -8.55, -12.55, 0.0, 0.80078125, 255.0, 51.0, false),
    (2.34, -8.55, -12.55, 0.0, 0.80078125, 255.0, 51.0, true),
    (2.34, -8.55, -12.55, 0.0, 0.80078125, 255.0, 51.0, false),
    (2.34, -8.55, -12.55, 0.0, 0.80078125, 255.0, 51.0, true),
    (2.34, -8.55, -12.55, 0.0, 0.80078125, 255.0, 51.0, false),
    (2.34, -8.55, -12.55, 0.0, 0.80078125, 255.0, 51.0, true),
    (2.34, -8.55, -12.55, 0.0, 0.80078125, 255.0, 51.0, false),
    (2.34, -8.55, -12.55, 0.0, 0.80078125, 255.0, 51.0, true),
    (2.34, -8.55, -12.55, 0.0, 0.80078125, 255.0, 51.0, false),
    (2.34, -8.55, -12.55, 0.0, 0.80078125, 255.0, 51.0, true),
    (2.34, -8.55, -12.55, 0.0, 0.80078125, 255.0, 51.0, false),
    (2.34, -8.55, -12.55, 0.0, 0.80078125, 255.0, 51.0, true),
    (2.34, -8.55, -12.55, 0.0, 0.80078125, 255.0, 51.0, false),
    (2.34, -8.55, -12.55, 0.0, 0.80078125, 255.0, 51.0, true),
    (
        2.17, -7.55, -11.1, 0.12109375, 0.82421875, 224.0, 45.0, true,
    ),
    (2.02, -6.55, -9.65, 0.234375, 0.84765625, 195.0, 39.0, true),
    (1.88, -5.65, -8.3, 0.33984375, 0.8671875, 168.0, 34.0, true),
    (1.75, -4.8, -7.1, 0.4375, 0.88671875, 143.0, 29.0, true),
    (1.63, -4.05, -5.95, 0.52734375, 0.90625, 121.0, 24.0, true),
    (1.52, -3.35, -4.95, 0.609375, 0.921875, 100.0, 20.0, true),
    (1.42, -2.7, -4.0, 0.68359375, 0.9375, 81.0, 16.0, true),
    (1.33, -2.15, -3.15, 0.75, 0.94921875, 64.0, 13.0, true),
    (1.26, -1.65, -2.45, 0.80859375, 0.9609375, 49.0, 10.0, true),
    (1.19, -1.2, -1.8, 0.859375, 0.97265625, 36.0, 7.0, true),
    (1.13, -0.85, -1.3, 0.90234375, 0.98046875, 25.0, 5.0, true),
    (1.08, -0.55, -0.8, 0.9375, 0.98828125, 16.0, 3.0, true),
    (1.05, -0.35, -0.5, 0.96484375, 0.9921875, 9.0, 2.0, true),
    (1.02, -0.15, -0.25, 0.984375, 0.99609375, 4.0, 1.0, true),
    (1.01, -0.05, -0.05, 0.99609375, 1.0, 1.0, 0.0, true),
    (1.0, 0.0, 0.0, 1.0, 1.0, 0.0, 0.0, true),
];
/// A list row's lock (lock_mc's mc) and price (price_mc's inner_mc) over their
/// ten frames as the list opens: x, y and alpha (their last frame is where they
/// stay).
const LOCK_SLIDE: [(f32, f32, f32); 10] = [
    (170.0, 2.0, 0.0),
    (130.55, -2.0, 0.210_937_5),
    (95.75, -5.5, 0.394_531_25),
    (65.55, -8.55, 0.554_687_5),
    (40.0, -11.15, 0.691_406_25),
    (19.15, -13.25, 0.800_781_25),
    (2.9, -14.9, 0.890_625),
    (-8.7, -16.05, 0.949_218_75),
    (-15.7, -16.75, 0.988_281_25),
    (-18.0, -17.0, 1.0),
];
const PRICE_SLIDE: [(f32, f32, f32); 10] = [
    (200.6, 13.6, 0.0),
    (161.2, 10.3, 0.210_937_5),
    (126.45, 7.4, 0.394_531_25),
    (96.35, 4.9, 0.554_687_5),
    (70.85, 2.8, 0.691_406_25),
    (50.0, 1.05, 0.800_781_25),
    (33.8, -0.3, 0.890_625),
    (22.2, -1.3, 0.949_218_75),
    (15.25, -1.85, 0.988_281_25),
    (12.95, -2.05, 1.0),
];

fn lerp(a: f32, b: f32, k: f32) -> f32 {
    a + (b - a) * k
}

fn lerp3(a: [f32; 3], b: [f32; 3], k: f32) -> [f32; 3] {
    [
        lerp(a[0], b[0], k),
        lerp(a[1], b[1], k),
        lerp(a[2], b[2], k),
    ]
}

fn rgb(c: [f32; 3]) -> [u8; 3] {
    c.map(|v| v.round().clamp(0.0, 255.0) as u8)
}

/// A line of text.
fn spec(
    text: impl Into<String>,
    font: FlashFont,
    size: f32,
    color: [u8; 3],
    align: Align,
    width: f32,
) -> TextSpec {
    TextSpec {
        text: text.into(),
        font,
        size,
        color,
        align,
        width,
        shadow: None,
        glow: None,
        zoom: 1.0,
    }
}

/// The movie's soft drop shadows and glows, as a small dark offset.
fn shadowed(mut s: TextSpec, rgba: [f32; 4]) -> TextSpec {
    s.shadow = Some(([1.0, 1.0], rgba));
    s
}

/// Greedy word wrap with the font's advances (as Flash wraps a field).
fn wrap(text: &MenuText, font: FlashFont, size: f32, width: f32, s: &str) -> Vec<String> {
    let mut lines = Vec::new();
    for para in s.split('\n') {
        let mut line = String::new();
        for word in para.split_whitespace() {
            let candidate = if line.is_empty() {
                word.to_string()
            } else {
                format!("{line} {word}")
            };
            if !line.is_empty() && text_width(text, font, size, &candidate) > width {
                lines.push(std::mem::take(&mut line));
                line = word.to_string();
            } else {
                line = candidate;
            }
        }
        lines.push(line);
    }
    lines
}

/// Draws and records the frame's elements.
struct Painter<'a> {
    cmds: Vec<crate::flash::DrawCmd>,
    hits: &'a mut Vec<Hit>,
    img: &'a CustImages,
    text: &'a MenuText,
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

    fn nine(
        &mut self,
        k: u64,
        name: &str,
        rect: [f32; 4],
        borders: [f32; 4],
        tex: [f32; 4],
        m: &M3,
        cx: Cx,
    ) {
        let h = self.img.get(name);
        self.canvas().nine(k, &h, rect, borders, tex, m, cx);
    }

    fn fill(&mut self, k: u64, rect: [f32; 4], m: &M3, color: [f32; 3], alpha: f32) {
        self.canvas().fill(k, rect, m, color, alpha);
    }

    fn text(&mut self, k: u64, s: TextSpec, x: f32, y: f32, m: &M3, cx: Cx) {
        self.canvas().text(k, s, x, y, m, cx);
    }

    fn hit(&mut self, target: Target, rect: [f32; 4], m: &M3) {
        self.hits.push(Hit {
            target,
            matrix: *m,
            rect,
        });
    }

    fn width(&self, font: FlashFont, size: f32, s: &str) -> f32 {
        text_width(self.text, font, size, s)
    }
}

// ---------------------------------------------------------------------------
// The frame
// ---------------------------------------------------------------------------

pub(crate) fn draw_customize(
    time: Res<Time>,
    data: Option<Res<CustData>>,
    images: Option<Res<CustImages>>,
    menu_text: Option<Res<MenuText>>,
    mut cust: ResMut<Customize>,
    mut frame: ResMut<FlashFrame>,
) {
    let cust = &mut *cust;
    cust.hits.clear();
    if cust.phase == Phase::Closed {
        return;
    }
    let (Some(data), Some(img), Some(menu_text)) = (data, images, menu_text) else {
        return;
    };
    let now = time.elapsed_secs();
    let mut hits = std::mem::take(&mut cust.hits);
    let mut p = Painter {
        cmds: Vec::new(),
        hits: &mut hits,
        img: &img,
        text: &menu_text,
    };

    draw_hints(&mut p, cust, &data, now);
    if let Some(at) = cust.editor.as_ref().and_then(|e| e.spinner) {
        draw_spinner(&mut p, now - at);
    }
    draw_select(&mut p, cust, &data, now);
    if let Some(ed) = cust.editor.as_mut() {
        draw_editor(&mut p, ed, &data, now);
        let ed = cust.editor.as_mut().unwrap();
        for i in 0..ed.info.len() {
            draw_info(&mut p, ed, i, &data, now);
        }
        if ed.rename.is_some() {
            draw_rename(&mut p, ed, &data, now);
        }
    }
    draw_notices(&mut p, cust, now);
    // The fade, over everything (and beyond the stage's sides).
    let fade = cust.fade.get(now);
    if fade > 0.003 {
        p.fill(
            key("fade", 0),
            [-2000.0, -2000.0, STAGE_W + 2000.0, STAGE_H + 2000.0],
            &M3::IDENTITY,
            [0.0; 3],
            fade,
        );
    }
    frame.cmds.extend(p.cmds);
    cust.hits = hits;
}

// ---------------------------------------------------------------------------
// The shop's notices
// ---------------------------------------------------------------------------

/// purchaseAlert_mc's place (the stage's middle; its challengeAnchor_mc at 0, 0).
const NOTICE_AT: (f32, f32) = (560.0, 360.0);
/// mc_notifyChallenge's frames 1-5: background_mc's x scale, x and alpha, and the
/// frames' x scale, x and alpha (frameBottom_mc's and frameTop_mc's alike).
const NOTICE_BG_KEYS: [(f32, f32, f32); 5] = [
    (0.2772, -37.5, 0.0),
    (0.6649, -89.05, 0.218_75),
    (0.9418, -125.85, 0.375),
    (1.1079, -147.95, 0.468_75),
    (1.1633, -155.3, 0.5),
];
const NOTICE_FRAME_KEYS: [(f32, f32, f32); 5] = [
    (0.0588, 0.0, 0.0),
    (0.3852, 5.1, 0.4375),
    (0.6183, 8.8, 0.75),
    (0.7582, 10.95, 0.9375),
    (0.8048, 11.7, 1.0),
];
/// background_mc's height as placed (its art's 128 x 0.8047).
const NOTICE_BG_H: f32 = 128.0 * 0.8047;
/// content_mc's text: the title's line, the description's (one line of 18 px
/// Register Demi Bold, autoSize), then the price's.
const NOTICE_DESC_Y: f32 = 31.0;
const NOTICE_DESC_H: f32 = 29.3;
const NOTICE_COST_H: f32 = 22.95;
/// The corner strokes' clip (sprite 16) turns cyan and back every 64 frames
/// (frames 28 to 63: choose.rs's NOTICE_PULSE).
const NOTICE_PULSE_FIRST: usize = 27;
const NOTICE_PULSE_FRAMES: usize = 64;

/// The shop's notices (mc_notifyChallenge): the one up and those fading out.
fn draw_notices(p: &mut Painter, cust: &Customize, now: f32) {
    let notices = &cust.notices;
    for (j, n) in notices
        .fading
        .iter()
        .chain(notices.shown.iter())
        .enumerate()
    {
        draw_notice(p, n, j, now);
    }
}

/// One notice: it opens over four frames; at the fifth its text comes in
/// (content_mc, 0.2 s) and the panel and its frames grow round it (0.2 s);
/// fadeOut drops it 20 px as it goes (0.5 s).
fn draw_notice(p: &mut Painter, n: &crate::customize::Notice, j: usize, now: f32) {
    let t = now - n.at;
    if t < 0.0 {
        return;
    }
    let out = n.fade_at.map_or(0.0, |at| {
        Ease::RegularOut.apply((now - at) / crate::customize::NOTICE_FADE_SECS)
    });
    let alpha = 1.0 - out;
    if alpha <= 0.003 {
        return;
    }
    let base = M3::translate(NOTICE_AT.0, NOTICE_AT.1 + 20.0 * out, 0.0);
    let f = (t * 30.0).min(4.0);
    let i = (f.floor() as usize).min(3);
    let k = f - i as f32;
    let lerp = |a: f32, b: f32| a + (b - a) * k;
    let (ba, bb) = (NOTICE_BG_KEYS[i], NOTICE_BG_KEYS[i + 1]);
    let (fa, fb) = (NOTICE_FRAME_KEYS[i], NOTICE_FRAME_KEYS[i + 1]);
    // From frame 5: the content's height (ChallengeHeight, plus 16) sets the
    // panel's (plus 50) and the frames' places.
    let content_h = NOTICE_DESC_Y + NOTICE_DESC_H + NOTICE_COST_H;
    let height = content_h + 16.0;
    let grow = Ease::RegularOut.apply((t - 4.0 / 30.0) / 0.2);
    let bg_h = NOTICE_BG_H + (height + 50.0 - NOTICE_BG_H) * grow;
    let panel = base.then(&M3::flat(
        lerp(ba.0, bb.0),
        0.0,
        0.0,
        bg_h / 128.0,
        lerp(ba.1, bb.1),
        2.0,
    ));
    p.image(
        key("cust_notice", j * 8),
        "notify_bg",
        [0.0, -64.0, 256.0, 64.0],
        &panel,
        Cx::alpha(lerp(ba.2, bb.2) * alpha),
    );
    let pulse = ((t * 30.0) as usize % NOTICE_PULSE_FRAMES)
        .checked_sub(NOTICE_PULSE_FIRST)
        .and_then(|i| crate::choose::NOTICE_PULSE.get(i))
        .copied()
        .unwrap_or((1.0, 0.0));
    let cx = Cx {
        mul: [pulse.0, pulse.0, pulse.0, lerp(fa.2, fb.2) * alpha],
        add: [0.0, pulse.1 / 255.0, pulse.1 / 255.0, 0.0],
    };
    let (sx, x) = (lerp(fa.0, fb.0), lerp(fa.1, fb.1));
    let bottom = base.then(&M3::flat(
        sx,
        0.0,
        0.0,
        1.0,
        x,
        -2.0 + (4.0 - height * 0.5) * grow,
    ));
    let top = base.then(&M3::flat(sx, 0.0, 0.0, 1.0, x, 2.0 + height * 0.5 * grow));
    let strokes = [
        (bottom, M3::flat(-1.0, 0.0, 0.0, 1.0, -99.05, -35.0)),
        (bottom, M3::flat(1.0312, 0.0, 0.0, 1.0, 55.95, -35.0)),
        (top, M3::flat(-1.0, 0.0, 0.0, -1.0, -99.05, 37.0)),
        (top, M3::flat(1.0, 0.0, 0.0, -1.0, 57.95, 37.0)),
    ];
    // (Shape 14 shows its art mirrored.)
    let mirror = M3::flat(-1.0, 0.0, 0.0, 1.0, 128.0, 0.0);
    for (s, (clip, child)) in strokes.iter().enumerate() {
        p.image(
            key("cust_notice", j * 8 + 1 + s),
            "notify_frame",
            [0.0, 0.0, 128.0, 64.0],
            &clip.then(child).then(&mirror),
            cx,
        );
    }
    // content_mc from frame 5, fading in (0.2 s), centred on its height: the
    // title, what was bought, and its price sliding in (0.4 s).
    let since = t - 4.0 / 30.0;
    if since < 0.0 {
        return;
    }
    let content_alpha = Ease::RegularOut.apply(since / 0.2) * alpha;
    let content = base.then(&M3::translate(-114.0, -content_h * 0.5, 0.0));
    let mut title = spec(
        n.title.clone(),
        FlashFont::Header,
        24.0,
        [204, 255, 255],
        Align::Left,
        216.0,
    );
    title.glow = Some(Glow {
        color: [0, 204, 153],
        strength: 1.0,
        blur: 5.0,
    });
    p.text(
        key("cust_notice", j * 8 + 5),
        title,
        2.0,
        2.0,
        &content,
        Cx::alpha(content_alpha),
    );
    p.text(
        key("cust_notice", j * 8 + 6),
        spec(
            n.description.clone(),
            FlashFont::Normal,
            18.0,
            [255, 255, 204],
            Align::Left,
            600.0,
        ),
        2.0,
        NOTICE_DESC_Y,
        &content,
        Cx::alpha(content_alpha),
    );
    if n.cost > 0 {
        let slide = Ease::RegularOut.apply(since / 0.4);
        let mut cost = spec(
            format!("*{}", n.cost),
            FlashFont::NumberWide,
            15.0,
            [255, 255, 255],
            Align::Left,
            216.0,
        );
        cost.glow = Some(Glow {
            color: [204, 255, 204],
            strength: 0.6,
            blur: 5.0,
        });
        p.text(
            key("cust_notice", j * 8 + 7),
            cost,
            2.0 - 10.0 * (1.0 - slide),
            NOTICE_DESC_Y + NOTICE_DESC_H,
            &content,
            Cx::alpha(content_alpha * slide),
        );
    }
}

// ---------------------------------------------------------------------------
// Button hints
// ---------------------------------------------------------------------------

fn hint_label(data: &CustData, cust: &Customize, hint: Hint) -> String {
    let s = |k: &str, f: &str| data.string(k, f);
    match hint {
        Hint::ToggleFaction => s("CustomCharacter.ToggleFaction", "Toggle Faction"),
        Hint::Close => s("ButtonHints.Close", "Close"),
        Hint::CameraLock => {
            if cust.editor.as_ref().is_some_and(|e| e.camera_locked) {
                s("CustomCharacter.UnlockCamera", "Unlock Camera")
            } else {
                s("CustomCharacter.LockCamera", "Lock Camera")
            }
        }
        Hint::Randomize => s("CustomCharacter.ResetCharacter", "Randomize"),
        Hint::ChangeForm => s("CustomCharacter.ChangeForm", "Change Form"),
        Hint::CyclePage => s("CustomCharacter.CyclePage", "Cycle Page"),
        Hint::Back => s("ButtonHints.Back", "Back"),
    }
}

/// The PC hint bar: a row of slim panel buttons (HmPanelButton) at the bottom;
/// while a controller plays, each with the picture of its button before its label.
fn draw_hints(p: &mut Painter, cust: &Customize, data: &CustData, now: f32) {
    let hints = cust.hints();
    let n = hints.len() as f32;
    let pad = cust.pad_hints == Some(true);
    for (i, hint) in hints.iter().enumerate() {
        let cx = STAGE_W * 0.5 + i as f32 * HINT_STEP - (n - 1.0) * HINT_STEP * 0.5;
        let rect = [
            cx - HINT_W * 0.5,
            HINT_TOP,
            cx + HINT_W * 0.5,
            HINT_TOP + HINT_H,
        ];
        let over = cust.hint_hover.get(hint).map(|t| t.get(now)).unwrap_or(0.0);
        let k = *hint as usize;
        p.fill(
            key("hint_box", k),
            rect,
            &M3::IDENTITY,
            lerp3(HINT_FILL, HINT_FILL_OVER, over),
            lerp(0.5, 0.8, over),
        );
        let label = hint_label(data, cust, *hint);
        let color = rgb(lerp3(HINT_TEXT.map(f32::from), HINT_FILL, over));
        if pad {
            let words = p.width(FlashFont::Title, 14.0, &label);
            let lay = hint_with_icon(cx + HINT_TEXT_SHIFT, words);
            p.image(
                key("hint_icon", k),
                hint.pad_button(),
                [
                    lay.icon_x,
                    HINT_TOP + (HINT_H - HINT_ICON) * 0.5,
                    lay.icon_x + HINT_ICON,
                    HINT_TOP + (HINT_H + HINT_ICON) * 0.5,
                ],
                &M3::IDENTITY,
                Cx::IDENTITY,
            );
            p.text(
                key("hint_text", k),
                spec(
                    label,
                    FlashFont::Title,
                    lay.size,
                    color,
                    Align::Left,
                    words * lay.size / 14.0 + 2.0,
                ),
                lay.text_x,
                HINT_TOP + 17.5 - lay.size * 0.6,
                &M3::IDENTITY,
                Cx::IDENTITY,
            );
        } else {
            p.text(
                key("hint_text", k),
                spec(label, FlashFont::Title, 14.0, color, Align::Center, HINT_W),
                rect[0] + HINT_TEXT_SHIFT,
                HINT_TOP + 17.5 - 14.0 * 0.6,
                &M3::IDENTITY,
                Cx::IDENTITY,
            );
        }
        p.hit(Target::Hint(*hint), rect, &M3::IDENTITY);
    }
}

/// Where a hint's button picture and label go (the picture's and the label's left
/// edges) and the label's size.
#[derive(Debug, PartialEq)]
struct IconHint {
    icon_x: f32,
    text_x: f32,
    size: f32,
}

/// A hint's picture and label (`words` wide at 14 px) centred on `centre`, the
/// label shrunk to fit the button if it must.
fn hint_with_icon(centre: f32, words: f32) -> IconHint {
    let room = HINT_W - 2.0 * HINT_ICON_MARGIN - HINT_ICON - HINT_ICON_GAP;
    let scale = if words > room { room / words } else { 1.0 };
    let words = words * scale;
    let whole = HINT_ICON + HINT_ICON_GAP + words;
    // (Kept inside the button: the label's shift right can't push it out.)
    let left = (centre - whole * 0.5)
        .min(centre - HINT_TEXT_SHIFT + HINT_W * 0.5 - HINT_ICON_MARGIN - whole);
    IconHint {
        icon_x: left,
        text_x: left + HINT_ICON + HINT_ICON_GAP,
        size: 14.0 * scale,
    }
}

/// The Randomize pulse (spinner_mc at (560, 260)): a ring of arcs that swells,
/// turns and fades in 20 frames.
fn draw_spinner(p: &mut Painter, t: f32) {
    let f = t * 30.0;
    if !(0.0..19.0).contains(&f) {
        return;
    }
    let (scale, alpha) = if f < 3.0 {
        (lerp(0.0685, 1.0, f / 3.0), lerp(0.0, 0.40, f / 3.0))
    } else {
        let k = (f - 3.0) / 16.0;
        (lerp(1.0, 0.657, k), lerp(0.40, 0.0, k))
    };
    let m = Clip {
        rot: f * 360.0 / 11.0,
        xscale: scale,
        yscale: scale,
        ..Clip::at(560.0, 260.0)
    }
    .matrix();
    let cx = Cx {
        mul: [1.0, 1.0, 1.0, alpha],
        add: [0.2, 1.0, 1.0, 0.0],
    };
    p.image(
        key("spinner", 0),
        "spinner",
        [-96.0, -96.0, 96.0, 96.0],
        &m,
        cx,
    );
}

// ---------------------------------------------------------------------------
// Class select
// ---------------------------------------------------------------------------

fn draw_select(p: &mut Painter, cust: &Customize, data: &CustData, now: f32) {
    let sel = &cust.select;
    let intro = sel.intro.get(now);
    let menu = Clip {
        z: 4000.0 * (1.0 - intro),
        xrot: -40.0 * (1.0 - intro),
        ..Clip::at(560.0, 455.0)
    }
    .matrix();
    let menu_alpha = intro;
    let in_editor = cust.editor.is_some();
    for class in CLASS_ORDER {
        let c = &sel.classes[class];
        if c.hidden {
            continue;
        }
        let fly = c.fly.get(now);
        let x = CLASS_X[class] + c.x_offset;
        let clip = menu.then(
            &Clip {
                z: 0.0,
                xscale: lerp(c.scale, 1.2, fly),
                yscale: lerp(c.scale, 1.2, fly),
                yrot: lerp(0.0, -40.0, fly),
                ..Clip::at(lerp(x, -510.0, fly), lerp(0.0, -370.0, fly))
            }
            .matrix(),
        );
        // The three slots (drawn under the tag; slot 3 lowest).
        let k = c.slots.get(now);
        if !c.locked && k > 0.003 {
            for slot in (0..SLOTS).rev() {
                let (ox, oy) = SLOT_OUT[slot];
                let at = M3::translate(lerp(SLOT_IN.0, ox, k), lerp(SLOT_IN.1, oy, k), 0.0);
                let m = clip.then(&at);
                let focused = sel.focus == Some(class) && sel.slot[class] == slot && !in_editor;
                let pressed = sel.pressed == Some((class, slot));
                draw_slot(
                    p,
                    cust,
                    data,
                    class,
                    slot,
                    focused,
                    pressed,
                    &m,
                    menu_alpha * k,
                );
                if k > 0.5 && !in_editor {
                    p.hit(
                        Target::Slot(class, slot),
                        [0.0, 0.0, 256.0 * SLOT_XSCALE, 32.0],
                        &m,
                    );
                }
            }
        }
        // The tag (classDisplay_mc).
        let d = c.display.get(now);
        let disp = clip.then(&M3::translate(lerp(30.0, 0.0, d), lerp(78.0, 0.0, d), 0.0));
        draw_tag(
            p,
            cust,
            data,
            class,
            &disp,
            c.parts.get(now),
            menu_alpha,
            c.locked,
            now,
        );
        if !in_editor {
            p.hit(Target::Class(class), [0.0, 0.0, 256.0, 64.0], &disp);
        }
    }
}

/// A build slot (SelectBuildItemRenderer): 274 x 32 bar, unscaled text.
#[allow(clippy::too_many_arguments)]
fn draw_slot(
    p: &mut Painter,
    cust: &Customize,
    data: &CustData,
    class: usize,
    slot: usize,
    focused: bool,
    pressed: bool,
    m: &M3,
    alpha: f32,
) {
    let k = class * 10 + slot;
    let bar = M3::scale(SLOT_XSCALE, 1.0);
    // A slot the class's level hasn't opened (refreshBuilds' isDisabled): the
    // disabled bar, brighter while it is selected (disabledHover_mc's "on"), and
    // the level it opens at in faint white.
    if !cust.slot_open(class, slot) {
        p.image(
            key("slot_bar", k),
            "slot_disabled",
            [0.0, 0.0, 256.0, 32.0],
            &m.then(&bar),
            Cx::alpha(alpha),
        );
        if focused {
            let mut hover = Cx::new([0.762, 0.762, 0.762, 1.0], [61.0, 61.0, 61.0, 0.0]);
            hover.mul[3] *= alpha;
            p.image(
                key("slot_hover", k),
                "slot_disabled",
                [0.0, 0.0, 256.0, 32.0],
                &m.then(&bar),
                hover,
            );
        }
        let text = data
            .string("CustomCharacter.UnlocksAtLevel", "Unlocks at Level `n")
            .replace("`n", &crate::xp::build_unlock_level(slot).to_string());
        let s = spec(
            text,
            FlashFont::Normal,
            16.0,
            [255, 255, 255],
            Align::Left,
            230.0,
        );
        p.text(
            key("slot_text", k),
            s,
            28.0,
            9.0,
            m,
            Cx::alpha(alpha * 63.0 / 255.0),
        );
        return;
    }
    let art = if focused && !pressed {
        "slot_over"
    } else {
        "slot_up"
    };
    p.image(
        key("slot_bar", k),
        art,
        [0.0, 0.0, 256.0, 32.0],
        &m.then(&bar),
        Cx::alpha(alpha),
    );
    let name = cust.builds[class][slot].name.clone();
    let name = if name.is_empty() {
        format!("{} {}", data.class_name(class), slot + 1)
    } else {
        name
    };
    let s = if focused && !pressed {
        spec(name, FlashFont::Normal, 16.0, NAVY, Align::Left, 230.0)
    } else {
        spec(
            name,
            FlashFont::Normal,
            if pressed { 16.0 } else { 15.0 },
            CREAM,
            Align::Left,
            230.0,
        )
    };
    p.text(key("slot_text", k), s, 28.0, 9.0, m, Cx::alpha(alpha));
}

/// The class tag (sprite 123): `parts` 0 collapsed .. 1 expanded.
#[allow(clippy::too_many_arguments)]
fn draw_tag(
    p: &mut Painter,
    cust: &Customize,
    data: &CustData,
    class: usize,
    m: &M3,
    parts: f32,
    alpha: f32,
    in_editor: bool,
    now: f32,
) {
    let k = class;
    // The plate: orange collapsed, blue expanded, at 50%.
    let tint = lerp3(ORANGE, BLUE, parts);
    p.image(
        key("tag_plate", k),
        "class_plate",
        [0.0, 0.0, 256.0, 64.0],
        m,
        Cx::fill(tint, 0.5 * alpha),
    );
    // The currency (in the editor's header): shards, its sign warning of too few
    // (showInsufficientFunds plays it from frame 2).
    if in_editor {
        let cur = m.then(&M3::translate(75.1, 61.05, 0.0));
        let frame = cust
            .funds_at
            .map(|at| 1 + ((now - at).max(0.0) * 30.0) as usize)
            .filter(|f| *f < FUNDS_WARNING.len())
            .unwrap_or(0);
        let (sc, tx, ty, mul, a, r, g, shown) = FUNDS_WARNING[frame];
        if shown {
            let sign = cur
                .then(&M3::translate(0.0, 2.5, 0.0))
                .then(&M3::flat(sc, 0.0, 0.0, sc, tx, ty));
            p.image(
                key("tag_shard", k),
                "currency",
                [-1.0, -1.0, 13.8, 19.8],
                &sign,
                Cx {
                    mul: [mul, mul, mul, a * alpha],
                    add: [r / 255.0, g / 255.0, 0.0, 0.0],
                },
            );
        }
        let s = shadowed(
            spec(
                cust.progress.shards.to_string(),
                FlashFont::Normal,
                16.0,
                [255, 255, 255],
                Align::Left,
                200.0,
            ),
            [0.0, 0.0, 0.0, 0.8],
        );
        p.text(key("tag_currency", k), s, 16.9, 2.0, &cur, Cx::alpha(alpha));
    }
    // level_mc: the class's level (GetLevelForSpecialty) and its experience
    // (GetXpEarnedForSpecialty) from this level's start to the next's
    // (GetXpNeededForLevel): the bar that far along, the marker at its end, and the
    // experience still to earn with the next level's number; at the top level
    // "MAX", the bar full and no marker.
    let class_level = cust.progress.level(class);
    let xp = cust.progress.xp(class);
    let last_xp = crate::xp::xp_for(class_level).unwrap_or(0);
    let next_xp = crate::xp::xp_for(class_level + 1).filter(|&n| n > last_xp);
    let ratio = next_xp.map_or(1.0, |n| {
        (xp.saturating_sub(last_xp) as f32 / (n - last_xp) as f32).clamp(0.0, 1.0)
    });
    let level = m.then(&M3::translate(44.0, lerp(50.0, 54.0, parts), 0.0));
    // The XP bar (StatBar, 157 px: its bar as wide as the value says, hidden at
    // the minimum).
    let bar = level
        .then(&M3::translate(15.0, 0.0, 0.0))
        .then(&M3::scale(0.785, 1.0));
    let h = lerp(8.0, 16.0, parts);
    p.fill(
        key("tag_track", k),
        [0.0, -h * 0.5, 200.0, h * 0.5],
        &bar,
        TRACK,
        0.25 * alpha,
    );
    if ratio > 0.0 {
        p.image(
            key("tag_xp", k),
            "xp_bar",
            [0.0, -h * 0.5, 200.0 * ratio, h * 0.5],
            &bar,
            Cx::alpha(alpha),
        );
    }
    // xpIndicator_mc, at the bar's end (16 px tall collapsed, 32 expanded, shown
    // with the expanded tag).
    if next_xp.is_some() {
        let marker = level
            .then(&M3::translate(15.0 + ratio * 200.0 * 0.785, 0.0, 0.0))
            .then(&M3::scale(1.0, lerp(1.0, 2.0, parts)));
        p.image(
            key("tag_marker", k),
            "xp_marker",
            [-4.0, -8.0, 4.0, 8.0],
            &marker,
            Cx::alpha(alpha * parts),
        );
    }
    // nextLevel_mc: "`n XP TO" and the next level, or "MAX", with the expanded tag.
    let next = level
        .then(&M3::translate(15.0, 0.0, 0.0))
        .then(&M3::scale(1.0, lerp(0.2, 1.0, parts)));
    let needed = match next_xp {
        Some(n) => data
            .string("CustomCharacter.XPToNextLevel", "`n XP to")
            .replace("`n", &n.saturating_sub(xp).to_string()),
        None => data.string("CustomCharacter.MaxXP", "Max"),
    }
    .to_uppercase();
    let s = shadowed(
        spec(
            needed,
            FlashFont::Normal,
            14.0,
            [255, 255, 255],
            Align::Right,
            146.0,
        ),
        [0.0, 0.0, 0.0, 0.8],
    );
    p.text(
        key("tag_max", k),
        s,
        9.0,
        -10.0,
        &next,
        Cx::alpha(alpha * parts),
    );
    if next_xp.is_some() {
        let s = shadowed(
            spec(
                (class_level + 1).to_string(),
                FlashFont::Normal,
                24.0,
                [255, 255, 255],
                Align::Left,
                40.0,
            ),
            [0.0, 0.0, 0.0, 0.8],
        );
        p.text(
            key("tag_next", k),
            s,
            161.0,
            -17.0,
            &next,
            Cx::alpha(alpha * parts),
        );
    }
    // The level's hexagon (stretched 140% across when expanded, scale-9).
    let hex = level.then(&M3::scale(lerp(0.8, 1.4, parts), lerp(0.8, 1.0, parts)));
    let w = 32.0 * lerp(0.8, 1.4, parts) / lerp(0.8, 1.0, parts);
    let hexm = level.then(&M3::scale(lerp(0.8, 1.0, parts), lerp(0.8, 1.0, parts)));
    let _ = hex;
    p.nine(
        key("tag_hex", k),
        "level_hex",
        [-w * 0.5, -16.0, w * 0.5, 16.0],
        [10.0, 1.0, 10.0, 1.0],
        [10.0 / 32.0, 1.0 / 32.0, 10.0 / 32.0, 1.0 / 32.0],
        &hexm,
        Cx::alpha(alpha * parts),
    );
    // The level number: light orange collapsed, dark blue on the hexagon.
    let num = level.then(&M3::scale(lerp(0.8, 1.0, parts), lerp(0.8, 1.0, parts)));
    let color = rgb(lerp3(LIGHT_ORANGE, DARK_BLUE, parts));
    p.text(
        key("tag_level", k),
        spec(
            class_level.to_string(),
            FlashFont::Normal,
            24.0,
            color,
            Align::Center,
            36.0,
        ),
        -18.0,
        -17.0,
        &num,
        Cx::alpha(alpha),
    );
    // title_mc: the class icon and the name (the build's, in the editor).
    let title = m.then(&M3::translate(43.0, lerp(28.0, 24.0, parts), 0.0));
    let icon = [
        "icon_scout",
        "icon_leader",
        "icon_soldier",
        "icon_scientist",
    ][class];
    let ih = if class == 0 { 26.0 } else { 28.0 };
    p.image(
        key("tag_icon", k),
        icon,
        [-28.0, -13.0, 28.0, -13.0 + ih],
        &title,
        Cx::alpha(alpha),
    );
    let name = match &cust.editor {
        Some(ed) if in_editor && ed.class == class => ed.build.name.clone(),
        _ => data.class_name(class).to_uppercase(),
    };
    let w = p.width(FlashFont::Header, 24.0, &name);
    let squeeze = if w > 193.0 { 193.0 / w } else { 1.0 };
    let tm = title
        .then(&M3::translate(29.0, -19.0, 0.0))
        .then(&M3::scale(squeeze, 1.0));
    let s = shadowed(
        spec(
            name,
            FlashFont::Header,
            24.0,
            [255, 255, 255],
            Align::Left,
            w.max(193.0) + 8.0,
        ),
        [0.0, 0.0, 0.0, 0.85],
    );
    p.text(key("tag_title", k), s, 2.0, 2.0, &tm, Cx::alpha(alpha));
}

// ---------------------------------------------------------------------------
// Editor
// ---------------------------------------------------------------------------

fn editor_matrix(ed: &EditorUi, now: f32) -> M3 {
    let k = ed.intro.get(now);
    Clip {
        z: lerp(1000.0, 0.0, k),
        yrot: lerp(0.0, 40.0, k),
        ..Clip::at(820.0, lerp(330.0, 230.0, k))
    }
    .matrix()
}

fn page_title(data: &CustData, page: usize) -> String {
    let (k, f) = [
        ("CustomCharacter.LoadoutTitle", "Loadouts"),
        ("CustomCharacter.PartTitle", "Armor"),
        ("CustomCharacter.CosmeticTitle", "Personality"),
    ][page];
    data.string(k, f).to_uppercase()
}

fn draw_editor(p: &mut Painter, ed: &mut EditorUi, data: &CustData, now: f32) {
    let editor = editor_matrix(ed, now);
    // Back pages first; the front page is drawn last.
    for slot in (0..3).rev() {
        let page = ed.ring[slot];
        let m = editor.then(
            &Clip {
                z: ed.page_z[page].get(now),
                xscale: 0.7,
                yscale: 0.7,
                ..Clip::at(ed.page_x[page].get(now), ed.page_y[page].get(now))
            }
            .matrix(),
        );
        let alpha = ed.page_alpha[page].get(now);
        let focused = slot == 0 && ed.sub.is_none();
        draw_page(p, ed, data, page, focused, &m, alpha, now);
    }
    if ed.sub.is_some() {
        draw_sub(p, ed, data, &editor, now);
    }
}

/// A page (menu_PanelBase): its rings, panel, title and (focused) its rows.
#[allow(clippy::too_many_arguments)]
fn draw_page(
    p: &mut Painter,
    ed: &EditorUi,
    data: &CustData,
    page: usize,
    focused: bool,
    m: &M3,
    alpha: f32,
    now: f32,
) {
    if alpha <= 0.003 {
        return;
    }
    // The rings, 30 px behind (shown while it has focus, or before it ever had it).
    if focused || !ed.page_seen[page] {
        let w1 = m.then(
            &Clip {
                z: 600.0,
                ..Clip::at(190.0, 40.0)
            }
            .matrix(),
        );
        p.image(
            key("wheel1", page),
            "wheel_a",
            [-152.0, -161.0, 148.0, 139.0],
            &w1,
            Cx::alpha(0.75 * alpha),
        );
        let w2 = m.then(
            &Clip {
                z: 600.0,
                ..Clip::at(40.0, 210.0)
            }
            .matrix(),
        );
        p.image(
            key("wheel2", page),
            "wheel_b",
            [0.0, 0.0, 300.0, 300.0],
            &w2,
            Cx::alpha(0.75 * alpha),
        );
    }
    p.image(
        key("panel", page),
        "panel",
        [-2.0, -74.0, 278.0, 486.0],
        m,
        Cx::alpha(alpha),
    );
    // The title plate, 40 px in front: orange with white text while focused, teal
    // otherwise.
    let t = m.then(
        &Clip {
            z: -800.0,
            ..Clip::at(-20.0, -85.0)
        }
        .matrix(),
    );
    let (plate, text) = if focused {
        (Cx::alpha(alpha), [255, 255, 255])
    } else {
        (
            Cx::new([0.1016, 0.3984, 0.3008, alpha], [0.0, 100.0, 140.0, 0.0]),
            TEAL_TEXT,
        )
    };
    p.image(
        key("title_plate", page),
        "title_plate",
        [0.0, 0.0, 300.0, 75.0],
        &t,
        plate,
    );
    let mut s = spec(
        page_title(data, page),
        FlashFont::Normal,
        24.0,
        text,
        Align::Left,
        210.0,
    );
    s.zoom = 0.75;
    if focused {
        s = shadowed(s, [99.0 / 255.0, 37.0 / 255.0, 31.0 / 255.0, 0.9]);
    }
    p.text(key("title_text", page), s, 67.0, 4.0, &t, Cx::alpha(alpha));
    if !focused {
        return;
    }
    // The page's rows, 40 px in front.
    let menu = m.then(
        &Clip {
            z: -800.0,
            ..Clip::at(0.0, 0.0)
        }
        .matrix(),
    );
    match page {
        0 => draw_loadouts(p, ed, data, &menu, alpha, now),
        1 => draw_armor(p, ed, data, &menu, alpha, now),
        _ => draw_personality(p, ed, data, &menu, alpha, now),
    }
}

/// A row button (3DButton, or 3DButtonShort when `short`): 275 x 55 (or 44) at
/// (-35, y), lit and popped out toward the camera when focused, with its ghosts.
#[allow(clippy::too_many_arguments)]
fn draw_button(
    p: &mut Painter,
    k: usize,
    label: &str,
    y: f32,
    short: bool,
    focused: bool,
    pop: f32,
    m: &M3,
    alpha: f32,
    text_x: f32,
) -> M3 {
    let h = if short { 44.0 } else { 55.0 };
    let base = m.then(&M3::translate(-35.0, y, 0.0));
    let w = 275.0;
    // Ghosts: white silhouettes left behind as it comes forward.
    if pop > 0.01 {
        for (g, (a, z)) in [(0.2, 1000.0), (0.5, 500.0)].iter().enumerate() {
            let gm = base.then(&M3::translate(
                0.0,
                0.0,
                -1000.0 * pop / 20.0 + z * pop / 20.0,
            ));
            p.nine(
                key("ghost", k * 4 + g),
                "bar_up",
                [0.0, 0.0, w, h],
                BAR_BORDERS,
                BAR_TEX,
                &gm,
                Cx::fill([255.0; 3], a * pop * alpha),
            );
        }
    }
    let bm = base.then(&M3::translate(0.0, 0.0, -1000.0 * pop / 20.0));
    let art = if focused { "bar_over" } else { "bar_up" };
    p.nine(
        key("button", k),
        art,
        [0.0, 0.0, w, h],
        BAR_BORDERS,
        BAR_TEX,
        &bm,
        Cx::alpha(alpha),
    );
    let size = if short { 16.0 } else { 18.0 };
    let ty = if short { 11.0 } else { 16.0 };
    let s = if focused {
        spec(label, FlashFont::Normal, size, NAVY, Align::Left, 240.0)
    } else {
        shadowed(
            spec(label, FlashFont::Normal, size, CREAM, Align::Left, 240.0),
            [143.0 / 255.0, 62.0 / 255.0, 1.0 / 255.0, 0.8],
        )
    };
    let mut s = s;
    s.zoom = 0.75;
    p.text(key("button_text", k), s, text_x, ty, &bm, Cx::alpha(alpha));
    bm
}

fn draw_loadouts(p: &mut Painter, ed: &EditorUi, data: &CustData, menu: &M3, alpha: f32, now: f32) {
    let b = &ed.build;
    let none = data.string("Shared.None", "None");
    let name = |list: &[crate::customize::Item], id: &str| {
        CustData::item(list, id)
            .map(|i| i.name.clone())
            .unwrap_or_else(|| none.clone())
    };
    let labels = [
        name(&data.weapons, &b.weapons[0]).to_uppercase(),
        name(&data.weapon_upgrades, &b.weapon_upgrades[0]),
        name(&data.weapons, &b.weapons[1]).to_uppercase(),
        name(&data.weapon_upgrades, &b.weapon_upgrades[1]),
        name(&data.abilities, &b.ability).to_uppercase(),
        name(&data.skills, &b.ability_upgrade),
    ];
    for (i, label) in labels.iter().enumerate() {
        let focused = ed.row[0] == i;
        let pop = ed.pops[0][i].get(now);
        let bm = draw_button(
            p,
            i,
            label,
            i as f32 * 55.0,
            i % 2 == 1,
            focused,
            pop,
            menu,
            alpha,
            12.0,
        );
        p.hit(Target::Row(i), [0.0, 0.0, 275.0, 55.0], &bm);
    }
}

fn draw_armor(p: &mut Painter, ed: &EditorUi, data: &CustData, menu: &M3, alpha: f32, now: f32) {
    let labels = crate::customize::armor_labels(data, ed.class);
    for (i, label) in labels.iter().enumerate() {
        let focused = ed.row[1] == i;
        let pop = ed.pops[1][i].get(now);
        let bm = draw_button(
            p,
            100 + i,
            label,
            i as f32 * 55.0,
            false,
            focused,
            pop,
            menu,
            alpha,
            12.0,
        );
        p.hit(Target::Row(i), [0.0, 0.0, 275.0, 55.0], &bm);
    }
}

fn draw_personality(
    p: &mut Painter,
    ed: &EditorUi,
    data: &CustData,
    menu: &M3,
    alpha: f32,
    now: f32,
) {
    let s = |k: &str, f: &str| data.string(&format!("CustomCharacter.{k}"), f);
    // The colour rows (CustomizeColorSwatchButton): the label after two chips.
    for f in 0..2 {
        let label = if f == 0 {
            s("CosmeticColorAutobot", "Autobot Colors")
        } else {
            s("CosmeticColorDecepticon", "Decepticon Colors")
        };
        let focused = ed.row[2] == f;
        let pop = ed.pops[2][f].get(now);
        let bm = draw_button(
            p,
            200 + f,
            &label,
            f as f32 * 55.0,
            false,
            focused,
            pop,
            menu,
            alpha,
            42.0,
        );
        if let Some(sw) = data.swatches[f]
            .iter()
            .find(|w| w.id == ed.build.swatches[f])
        {
            // The chips are inside the stretched button.
            let chips = bm.then(&M3::scale(1.25, 1.0));
            p.image(
                key("chip_a", f),
                "chip_primary",
                [4.5, 11.25, 24.5, 51.25],
                &chips,
                Cx::fill(Swatch::shown(sw.primary), alpha),
            );
            p.image(
                key("chip_b", f),
                "chip_secondary",
                [18.0, 11.25, 40.0, 51.25],
                &chips,
                Cx::fill(Swatch::shown(sw.secondary), alpha),
            );
        }
        p.hit(Target::Row(f), [0.0, 0.0, 275.0, 55.0], &bm);
    }
    // "Metallic" and its slider.
    let metallic = s("Metallic", "Metallic");
    let label = spec(
        metallic,
        FlashFont::Normal,
        18.0,
        [0, 255, 211],
        Align::Left,
        250.0,
    );
    p.text(
        key("metal_label", 0),
        label,
        -23.0,
        117.0,
        menu,
        Cx::alpha(alpha),
    );
    draw_slider(p, ed.build.metallic, ed.row[2] == 2, menu, alpha);
    p.hit(Target::Row(2), [-45.0, 117.0, 250.0, 165.0], menu);
    p.hit(Target::Slider, [-45.0, 133.0, 250.0, 165.0], menu);
    for (i, (label, y)) in [
        (s("CosmeticVoice", "Voice"), 165.0),
        (s("CosmeticRenameBuild", "Rename Build"), 220.0),
    ]
    .iter()
    .enumerate()
    {
        let row = 3 + i;
        let focused = ed.row[2] == row;
        let pop = ed.pops[2][row].get(now);
        let bm = draw_button(
            p,
            203 + i,
            label,
            *y,
            false,
            focused,
            pop,
            menu,
            alpha,
            12.0,
        );
        p.hit(Target::Row(row), [0.0, 0.0, 275.0, 55.0], &bm);
    }
}

/// The metallic slider (TransSlider, 295 x 32 at (-45, 133)): track, bar, arrows;
/// bright while focused.
fn draw_slider(p: &mut Painter, value: f32, focused: bool, menu: &M3, alpha: f32) {
    let m = menu.then(&M3::translate(-45.0, 133.0, 0.0));
    let w = 295.0;
    let track_a = if focused { 0.5 } else { 0.25 };
    p.nine(
        key("slider_track", 0),
        "slider_track",
        [12.0, 0.0, w - 12.0, 32.0],
        [14.0, 14.0, 14.0, 14.0],
        [14.0 / 64.0, 14.0 / 32.0, 14.0 / 64.0, 14.0 / 32.0],
        &m,
        Cx::alpha(track_a * alpha),
    );
    let bar_w = 20.0 + value.clamp(0.0, 1.0) * (w - 24.0 - 20.0);
    let bar_cx = if focused {
        Cx::alpha(alpha)
    } else {
        Cx::new([0.1992, 1.0, 1.0, 0.5 * alpha], [0.0; 4])
    };
    if value > 0.0 {
        p.nine(
            key("slider_bar", 0),
            "slider_bar",
            [12.0, 0.0, 12.0 + bar_w, 32.0],
            [12.0, 14.0, 12.0, 14.0],
            [12.0 / 128.0, 14.0 / 32.0, 12.0 / 128.0, 14.0 / 32.0],
            &m,
            bar_cx,
        );
    }
    let arrow_cx = if focused {
        Cx::alpha(alpha)
    } else {
        Cx::new([0.0, 1.0, 1.0, 0.5 * alpha], [0.0; 4])
    };
    // decArrow is the arrow mirrored, at the left end.
    p.image(
        key("slider_dec", 0),
        "slider_arrow",
        [0.0, 0.0, 32.0, 32.0],
        &m.then(&M3::flat(-1.0, 0.0, 0.0, 1.0, 32.0, 0.0)),
        arrow_cx,
    );
    p.image(
        key("slider_inc", 0),
        "slider_arrow",
        [w - 32.0, 0.0, w, 32.0],
        &m,
        arrow_cx,
    );
}

// ---------------------------------------------------------------------------
// Option list and colour grid
// ---------------------------------------------------------------------------

fn draw_sub(p: &mut Painter, ed: &EditorUi, data: &CustData, editor: &M3, now: f32) {
    let Some(sub) = &ed.sub else {
        return;
    };
    let m = editor.then(
        &Clip {
            z: sub.z.get(now),
            xscale: 0.7,
            yscale: 0.7,
            ..Clip::at(0.0, 0.0)
        }
        .matrix(),
    );
    // The title: white, with a dark (list) or cream (colours) glow.
    let glow = if sub.swatches {
        [1.0, 235.0 / 255.0, 179.0 / 255.0, 0.6]
    } else {
        [0.0, 0.0, 0.2, 0.9]
    };
    let mut title = shadowed(
        spec(
            sub.title.clone(),
            FlashFont::Normal,
            18.0,
            [255, 255, 255],
            Align::Left,
            400.0,
        ),
        glow,
    );
    title.zoom = 0.75;
    p.text(key("sub_title", 0), title, 2.0, -32.0, &m, Cx::IDENTITY);
    if sub.swatches {
        draw_swatches(p, ed, data, &m, now);
    } else {
        draw_options(p, ed, &m, now);
    }
}

fn draw_options(p: &mut Painter, ed: &EditorUi, m: &M3, now: f32) {
    let sub = ed.sub.as_ref().unwrap();
    let n = sub.rows.len();
    let first = sub.scroll.min(n.saturating_sub(1));
    let last = (first + 8).min(n);
    let w = LIST_ROW_W;
    for i in first..last {
        let row = &sub.rows[i];
        let y = (i - first) as f32 * 55.0;
        let selected = i == sub.selected;
        let pop = sub.pops.get(i).map(|t| t.get(now)).unwrap_or(0.0);
        let base = m.then(&M3::translate(0.0, y, 0.0));
        if pop > 0.01 {
            for (g, (a, z)) in [(0.2, 1000.0), (0.5, 500.0)].iter().enumerate() {
                let gm = base.then(&M3::translate(0.0, 0.0, (-1000.0 + z) * pop / 20.0));
                p.nine(
                    key("opt_ghost", i * 4 + g),
                    "bar_up",
                    [0.0, 0.0, w, 55.0],
                    BAR_BORDERS,
                    BAR_TEX,
                    &gm,
                    Cx::fill([255.0; 3], a * pop),
                );
            }
        }
        let bm = base.then(&M3::translate(0.0, 0.0, -1000.0 * pop / 20.0));
        let art = if selected { "bar_over" } else { "bar_up" };
        p.nine(
            key("opt_bar", i),
            art,
            [0.0, 0.0, w, 55.0],
            BAR_BORDERS,
            BAR_TEX,
            &bm,
            Cx::IDENTITY,
        );
        let mut s = if selected {
            spec(
                row.label.clone(),
                FlashFont::Normal,
                18.0,
                NAVY,
                Align::Left,
                240.0,
            )
        } else {
            shadowed(
                spec(
                    row.label.clone(),
                    FlashFont::Normal,
                    18.0,
                    CREAM,
                    Align::Left,
                    240.0,
                ),
                [143.0 / 255.0, 62.0 / 255.0, 1.0 / 255.0, 0.8],
            )
        };
        s.zoom = 0.75;
        p.text(key("opt_text", i), s, 12.0, 16.0, &bm, Cx::IDENTITY);
        // The lock and the price slide in from the right as the list opens
        // (lock_mc's and price_mc's ten frames).
        let slide = ((now - sub.opened) * 30.0).max(0.0) as usize;
        // A row not bought yet: its price, left of the row (price_mc).
        if row.price > 0 && row.locked.is_none() {
            let (x, y, a) = PRICE_SLIDE[slide.min(PRICE_SLIDE.len() - 1)];
            let mut s = spec(
                format!("*{}", row.price),
                FlashFont::NumberWide,
                20.0,
                LOCK_TEXT,
                Align::Right,
                157.9,
            );
            s.glow = Some(Glow {
                color: [0, 0, 0],
                strength: 1.5,
                blur: 4.0,
            });
            s.zoom = 0.75;
            p.text(
                key("opt_price", i),
                s,
                x - 160.0,
                y + 2.0,
                &bm,
                Cx::alpha(a),
            );
        }
        // A locked row: the lock and its reason, left of the row (blinking red
        // when chosen).
        if let Some(why) = &row.locked {
            let (lx, ly, la) = LOCK_SLIDE[slide.min(LOCK_SLIDE.len() - 1)];
            let bm = bm.then(&M3::translate(lx + 18.0, ly + 17.0, 0.0));
            let blink = sub
                .blink
                .filter(|(r, _)| *r == i)
                .map(|(_, at)| now - at)
                .filter(|t| *t < 0.8);
            let (scale, red) = match blink {
                Some(t) => {
                    let f = t * 30.0;
                    let s = if f < 6.0 {
                        lerp(1.0, 1.489, f / 6.0)
                    } else {
                        lerp(1.489, 1.0, ((f - 6.0) / 18.0).min(1.0))
                    };
                    (
                        s,
                        if (f as i32) % 2 == 1 || f < 6.0 {
                            1.0
                        } else {
                            0.0
                        },
                    )
                }
                None => (1.0, 0.0),
            };
            let lock = bm
                .then(&M3::translate(2.5, 5.5, 0.0))
                .then(&M3::scale(scale, scale));
            let cx = Cx {
                mul: [1.0 - red, 1.0 - red * 0.8, 1.0 - red * 0.8, la],
                add: [red, 0.2 * red, 0.2 * red, 0.0],
            };
            p.image(
                key("opt_lock", i),
                "lock",
                [-20.5, -22.5, 20.5, 22.5],
                &lock,
                cx,
            );
            let s = shadowed(
                spec(
                    why.clone(),
                    FlashFont::Normal,
                    20.0,
                    LOCK_TEXT,
                    Align::Right,
                    196.0,
                ),
                [0.0, 0.0, 0.2, 0.9],
            );
            p.text(key("opt_lock_text", i), s, -208.0, -7.0, &bm, Cx::alpha(la));
        }
        p.hit(Target::Option(i), [0.0, 0.0, w, 55.0], &bm);
    }
    // The scroll bar, with more than 8 options.
    if n > 8 {
        let track = m.then(&M3::translate(259.0, 0.0, 0.0));
        p.nine(
            key("scroll_track", 0),
            "scroll_track",
            [0.0, 0.0, 16.0, 440.0],
            [0.0, 7.0, 0.0, 7.0],
            [0.0, 7.0 / 64.0, 0.0, 7.0 / 64.0],
            &track,
            Cx::IDENTITY,
        );
        let th = (8.0 / n as f32 * 440.0).clamp(10.0, 440.0);
        let max = (n - 8) as f32;
        let ty = sub.scroll as f32 / max * (440.0 - th);
        p.nine(
            key("scroll_thumb", 0),
            "scroll_thumb",
            [0.0, ty, 16.0, ty + th],
            [0.0, 7.0, 0.0, 7.0],
            [0.0, 7.0 / 32.0, 0.0, 7.0 / 32.0],
            &track,
            Cx::IDENTITY,
        );
    }
}

/// The colour grid (TransparentTileList at (74, 10)): 3 columns, 10 rows on a 41 px
/// pitch; each tile split between the swatch's two colours, the chosen one bigger
/// and in front.
fn draw_swatches(p: &mut Painter, ed: &EditorUi, data: &CustData, m: &M3, now: f32) {
    let sub = ed.sub.as_ref().unwrap();
    let Field::Swatch(faction) = sub.field else {
        return;
    };
    let list = &data.swatches[faction];
    let first = sub.scroll * 3;
    let last = (first + 30).min(list.len());
    let mut tiles: Vec<usize> = (first..last).collect();
    // The selected tile moves to the front.
    if let Some(pos) = tiles.iter().position(|&i| i == sub.selected) {
        let s = tiles.remove(pos);
        tiles.push(s);
    }
    for i in tiles {
        let sw = &list[i];
        let v = i - first;
        let cx = 74.0 + 20.5 + (v % 3) as f32 * 41.0;
        let cy = 10.0 + 20.5 + (v / 3) as f32 * 41.0;
        let pop = sub.pops.get(i).map(|t| t.get(now)).unwrap_or(0.0);
        let s = lerp(1.0, 1.25, pop);
        let tm = m.then(
            &Clip {
                z: -400.0 * pop,
                xscale: s,
                yscale: s,
                ..Clip::at(cx, cy)
            }
            .matrix(),
        );
        let tint = |c: [f32; 3]| {
            let c = Swatch::shown(c);
            Cx::new([c[0] / 255.0, c[1] / 255.0, c[2] / 255.0, 1.0], [0.0; 4])
        };
        p.image(
            key("tile_b", i),
            "swatch_secondary",
            [-17.5, -17.5, 17.5, 17.5],
            &tm,
            tint(sw.secondary),
        );
        p.image(
            key("tile_a", i),
            "swatch_primary",
            [-17.5, -17.5, 17.5, 17.5],
            &tm,
            tint(sw.primary),
        );
        let frame = if i == sub.selected {
            "swatch_frame_over"
        } else {
            "swatch_frame"
        };
        p.image(
            key("tile_frame", i),
            frame,
            [-26.0, -26.0, 26.0, 26.0],
            &tm,
            Cx::IDENTITY,
        );
        p.hit(Target::Option(i), [-20.5, -20.5, 20.5, 20.5], &tm);
    }
}

// ---------------------------------------------------------------------------
// Info panels
// ---------------------------------------------------------------------------

/// A compare bar (CustomizeCompareStatBar) at `m`: the ruler, the teal track and
/// the orange bar `value / max` of 200 px, slanted back.
fn draw_stat_bar(p: &mut Painter, k: usize, value: f32, max: f32, m: &M3, alpha: f32) {
    let slant = m.then(&M3::flat(1.0, 0.0, -0.599, 1.0, 0.0, 0.0));
    p.image(
        key("stat_ruler", k),
        "ruler",
        [-2.0, 10.0, 202.0, 16.35],
        &slant,
        Cx::alpha(alpha),
    );
    p.fill(
        key("stat_track", k),
        [0.0, 0.0, 200.0, 8.0],
        &slant,
        TRACK,
        0.25 * alpha,
    );
    let w = (value / max).clamp(0.0, 1.0) * 200.0;
    if w > 0.5 {
        p.image(
            key("stat_bar", k),
            "stat_bar",
            [0.0, 0.0, w, 8.0],
            &slant,
            Cx::alpha(alpha),
        );
    }
}

/// A stat's label (16 px orange, upper case).
fn stat_label(p: &mut Painter, k: usize, label: &str, x: f32, y: f32, m: &M3, alpha: f32) {
    let s = shadowed(
        spec(
            label.to_uppercase(),
            FlashFont::Normal,
            16.0,
            STAT_LABEL,
            Align::Left,
            200.0,
        ),
        [0.2, 0.0, 0.0, 0.8],
    );
    p.text(key("stat_label", k), s, x, y, m, Cx::alpha(alpha));
}

fn draw_info(p: &mut Painter, ed: &mut EditorUi, index: usize, data: &CustData, now: f32) {
    let panel: &InfoPanel = &ed.info[index];
    let page = panel.page;
    let born = panel.born;
    let leaving = panel.leaving;
    let fade = panel.alpha.get(now);
    // In: 1.2 s from 100 px up, turned -60 degrees, transparent. Out: 1 s back and down.
    let rest = if page == 0 { -30.0 } else { -40.0 };
    let k = crate::flash::Ease::StrongOut.apply(((now - born) / 1.2).clamp(0.0, 1.0));
    let (mut y, mut yrot, mut z, mut alpha) = (lerp(220.0, 320.0, k), lerp(-60.0, rest, k), 0.0, k);
    if let Some(at) = leaving {
        let o = crate::flash::Ease::StrongOut.apply(((now - at) / 1.0).clamp(0.0, 1.0));
        y = lerp(y, 420.0, o);
        yrot = lerp(yrot, -20.0, o);
        z = lerp(0.0, 2000.0, o);
        alpha *= 1.0 - o;
    }
    alpha *= fade;
    if alpha <= 0.003 {
        return;
    }
    let m = Clip {
        z,
        yrot,
        xscale: 0.8,
        yscale: 0.8,
        ..Clip::at(100.0, y)
    }
    .matrix();
    let kk = index * 1000;
    let layer = |z: f32| {
        Clip {
            z,
            ..Clip::at(0.0, 0.0)
        }
        .matrix()
    };
    if page == 1 {
        // mc_partInfo: "ANALYSIS" and the class's speeds and health (out of 10).
        let wheel = m.then(
            &Clip {
                z: 800.0,
                ..Clip::at(50.0, 76.0)
            }
            .matrix(),
        );
        p.image(
            key("info_wheel", kk),
            "wheel_a",
            [-152.0, -161.0, 148.0, 139.0],
            &wheel,
            Cx::alpha(0.75 * alpha),
        );
        p.image(
            key("info_panel", kk),
            "info_panel",
            [-56.0, -54.0, 284.0, 286.0],
            &m.then(&layer(0.0)),
            Cx::alpha(alpha),
        );
        let title = m
            .then(&layer(-400.0))
            .then(&M3::translate(-74.0, -40.0, 0.0));
        draw_info_title(
            p,
            kk,
            &data.string("CustomCharacter.ContextPartTitle", "Analysis"),
            &title,
            alpha,
        );
        let stats = m.then(&layer(-800.0)).then(&M3::translate(24.0, 20.0, 0.0));
        let c = &data.classes[ed.class];
        let rows = [
            (
                "StatRobotSpeed",
                "Robot Speed",
                c.robot_speed,
                "stat_robot_speed",
            ),
            (
                "StatVehicleSpeed",
                "Vehicle Speed",
                c.vehicle_speed,
                "stat_vehicle_speed",
            ),
            ("StatHealth", "Health", c.health, "stat_health"),
        ];
        for (i, (sk, fallback, value, icon)) in rows.iter().enumerate() {
            let y = i as f32 * 50.0;
            let label = data.string(&format!("CustomCharacter.{sk}"), fallback);
            stat_label(p, kk + i, &label, 2.0, 3.0 + y, &stats, alpha);
            draw_stat_bar(
                p,
                kk + i,
                *value,
                10.0,
                &stats.then(&M3::translate(2.0, 24.0 + y, 0.0)),
                alpha,
            );
            p.image(
                key("stat_icon", kk + i),
                icon,
                [-32.0, 5.0 + y, -2.0, 35.0 + y],
                &stats,
                Cx::alpha(alpha),
            );
        }
        return;
    }

    // mc_weaponInfo: the described weapon (or the ability) and its ratings.
    let wheel = m.then(
        &Clip {
            z: 800.0,
            ..Clip::at(50.0, -14.0)
        }
        .matrix(),
    );
    p.image(
        key("info_wheel", kk),
        "wheel_a",
        [-152.0, -161.0, 148.0, 139.0],
        &wheel,
        Cx::alpha(0.75 * alpha),
    );
    // The tall panel: its rows 286-289.5 stretch to make it 510 high.
    p.nine(
        key("info_panel", kk),
        "info_panel_large",
        [-56.0, -110.0, 284.0, 400.0],
        [0.0, 286.0, 0.0, 50.5],
        [0.0, 286.0 / 340.0, 0.0, 50.5 / 340.0],
        &m.then(&layer(0.0)),
        Cx::alpha(alpha),
    );
    let weapon_k = ed.info[index].weapon_mode.get(now);
    let b = &ed.build;
    // Which weapon slot is described: the focused row's, or the open list's.
    let slot = match &ed.sub {
        Some(sub) => match sub.field {
            Field::Weapon(s) | Field::WeaponUpgrade(s) => s,
            _ => 0,
        },
        None => (ed.row[0] / 2).min(1),
    };
    let title_m = m
        .then(&layer(-400.0))
        .then(&M3::translate(-74.0, -130.0, 0.0));
    let ability = CustData::item(&data.abilities, &b.ability);
    // The plate; its text names the ability (in weapon mode the weapon's own
    // title row shows instead).
    draw_info_title_plate(p, kk, &title_m, alpha);
    if let Some(a) = ability {
        let s = spec(
            a.name.to_uppercase(),
            FlashFont::Normal,
            24.0,
            CYAN_TEXT,
            Align::Left,
            350.0,
        );
        let w = p.width(FlashFont::Normal, 24.0, &a.name.to_uppercase());
        let sq = if w > 250.0 { 250.0 / w } else { 1.0 };
        p.text(
            key("info_title_text", kk),
            s,
            2.0,
            2.0,
            &title_m
                .then(&M3::translate(66.0, 2.0, 0.0))
                .then(&M3::scale(sq, 1.0)),
            Cx::alpha(alpha * (1.0 - weapon_k)),
        );
    }
    let details = m
        .then(&layer(-800.0))
        .then(&M3::translate(-18.0, -88.0, 0.0));
    let none = data.string("Shared.None", "None");
    if weapon_k > 0.003 {
        let a = alpha * weapon_k;
        let weapon = CustData::item(&data.weapons, &b.weapons[slot]);
        // (While a list is browsed the build can hold another weapon's upgrade.)
        let upgrade = CustData::item(&data.weapon_upgrades, &b.weapon_upgrades[slot])
            .filter(|u| u.owner == b.weapons[slot]);
        // The weapon's title row (as a title: bright text, no plate).
        let row = details
            .then(&M3::translate(-16.0, -36.0, 0.0))
            .then(&M3::scale(1.2, 1.2));
        let name = weapon.map(|w| w.name.to_uppercase()).unwrap_or_default();
        let w = p.width(FlashFont::Normal, 18.0, &name);
        let sq = if w > 200.0 { 200.0 / w } else { 1.0 };
        p.text(
            key("info_weapon", kk),
            spec(name, FlashFont::Normal, 18.0, CYAN_TEXT, Align::Left, 350.0),
            2.0,
            2.0,
            &row.then(&M3::translate(22.0, 0.0, 0.0))
                .then(&M3::scale(sq, 1.0)),
            Cx::alpha(a),
        );
        // Its upgrade row.
        let up = details.then(&M3::translate(9.0, 1.0, 0.0));
        p.nine(
            key("info_up_bar", kk),
            "upgrade_bar",
            [0.0, 1.0, 245.5, 27.0],
            [8.0, 1.0, 1.0, 1.0],
            [8.0 / 104.0 * 0.8125, 0.04, 0.01, 0.04],
            &up,
            Cx::alpha(a),
        );
        p.image(
            key("info_up_arrow", kk),
            "upgrade_arrow",
            [9.5, 4.0, 27.5, 22.0],
            &up,
            Cx::fill([77.0, 174.0, 196.0], a),
        );
        let up_name = upgrade
            .map(|u| u.name.to_uppercase())
            .unwrap_or_else(|| none.to_uppercase());
        p.text(
            key("info_up_text", kk),
            spec(
                up_name,
                FlashFont::Normal,
                18.0,
                [77, 174, 196],
                Align::Left,
                220.0,
            ),
            27.5,
            2.0,
            &up,
            Cx::alpha(a),
        );
        // The description: the upgrade's while its list is open, else the weapon's.
        let describing_upgrade = matches!(
            ed.sub.as_ref().map(|s| s.field),
            Some(Field::WeaponUpgrade(_))
        );
        let desc = if describing_upgrade {
            upgrade.map(|u| u.description.clone()).unwrap_or_else(|| {
                data.string(
                    "CustomCharacter.WeaponUpgradeNoneDesc",
                    "Don't apply a weapon upgrade",
                )
            })
        } else {
            weapon.map(|w| w.description.clone()).unwrap_or_default()
        };
        let panel = &mut ed.info[index];
        if panel.described != desc {
            panel.described = desc.clone();
            panel.described_at = now;
        }
        let da = crate::flash::Ease::RegularOut
            .apply(((now - panel.described_at) / 0.4).clamp(0.0, 1.0));
        let lines = wrap(p.text, FlashFont::Normal, 16.0, 226.0, &desc);
        for (i, line) in lines.iter().take(6).enumerate() {
            let s = shadowed(
                spec(
                    line.clone(),
                    FlashFont::Normal,
                    16.0,
                    CYAN_TEXT,
                    Align::Left,
                    230.0,
                ),
                [0.0, 0.0, 0.2, 0.9],
            );
            p.text(
                key("info_desc", kk * 10 + i),
                s,
                13.0,
                32.0 + i as f32 * 17.0,
                &details,
                Cx::alpha(a * da),
            );
        }
        // The weapon's picture, below the description, as a pale silhouette.
        let text_h = lines.len().min(6) as f32 * 17.0;
        if let Some(w) = weapon
            && let Some(h) = p.img.wires.get(&w.id).cloned()
        {
            let wm = details
                .then(&M3::translate(-10.0, 30.0 + text_h + 10.0, 0.0))
                .then(&M3::scale(0.5, 0.5));
            p.canvas().image(
                key("info_wire", kk),
                &h,
                [0.0, 0.0, 512.0, 256.0],
                &wm,
                Cx::fill([204.0, 255.0, 255.0], a),
            );
        }
        // The ratings.
        let stats = details.then(&M3::translate(32.0, 275.0, 0.0));
        let r = weapon.map(|w| w.ratings).unwrap_or([0.0; 4]);
        let rows = [
            ("StatDamage", "Damage", "stat_damage"),
            ("StatRange", "Range", "stat_range"),
            ("StatAccuracy", "Accuracy", "stat_accuracy"),
            ("StatFireRate", "Rate of Fire", "stat_fire_rate"),
        ];
        let icon_y = [5.0, 50.0, 95.0, 140.0];
        let bar_y = [23.0, 69.0, 114.0, 159.0];
        let label_y = [3.0, 48.0, 93.0, 138.0];
        for (i, (sk, fallback, icon)) in rows.iter().enumerate() {
            let label = data.string(&format!("CustomCharacter.{sk}"), fallback);
            stat_label(p, kk + 10 + i, &label, 2.0, label_y[i], &stats, a);
            draw_stat_bar(
                p,
                kk + 10 + i,
                r[i],
                1.0,
                &stats.then(&M3::translate(2.0, bar_y[i], 0.0)),
                a,
            );
            p.image(
                key("stat_icon", kk + 10 + i),
                icon,
                [-32.0, icon_y[i], -2.0, icon_y[i] + 30.0],
                &stats,
                Cx::alpha(a),
            );
        }
    }
    if weapon_k < 0.997 {
        // The ability: its description, its upgrade's row and description, picture.
        let a = alpha * (1.0 - weapon_k);
        let ab = details.then(&M3::translate(0.0, 28.0, 0.0));
        let desc = ability.map(|x| x.description.clone()).unwrap_or_default();
        let lines = wrap(p.text, FlashFont::Normal, 16.0, 226.0, &desc);
        for (i, line) in lines.iter().take(5).enumerate() {
            let s = shadowed(
                spec(
                    line.clone(),
                    FlashFont::Normal,
                    16.0,
                    CYAN_TEXT,
                    Align::Left,
                    230.0,
                ),
                [0.0, 0.0, 0.2, 0.9],
            );
            p.text(
                key("ab_desc", kk * 10 + i),
                s,
                13.0,
                -12.0 + i as f32 * 17.0,
                &ab,
                Cx::alpha(a),
            );
        }
        let up_y = -14.0 + lines.len().min(5) as f32 * 17.0 + 12.0;
        let skill =
            CustData::item(&data.skills, &b.ability_upgrade).filter(|s| s.owner == b.ability);
        let row = ab.then(&M3::translate(-5.0, up_y, 0.0));
        p.nine(
            key("ab_up_plate", kk),
            "sub_title",
            [0.0, 2.0, 256.0, 40.0],
            [19.0, 1.0, 1.0, 1.0],
            [19.0 / 152.0 * 0.59375, 0.03, 0.01, 0.03],
            &row,
            Cx::alpha(a),
        );
        let name = skill
            .map(|s| s.name.to_uppercase())
            .unwrap_or_else(|| none.to_uppercase());
        p.text(
            key("ab_up_text", kk),
            spec(name, FlashFont::Normal, 18.0, TEAL_TEXT, Align::Left, 220.0),
            24.0,
            2.0,
            &row,
            Cx::alpha(a),
        );
        let udesc = skill.map(|s| s.description.clone()).unwrap_or_else(|| {
            data.string(
                "CustomCharacter.AbilityUpgradeNoneDesc",
                "Don't apply an ability upgrade",
            )
        });
        let ulines = wrap(p.text, FlashFont::Normal, 16.0, 226.0, &udesc);
        for (i, line) in ulines.iter().take(4).enumerate() {
            let s = shadowed(
                spec(
                    line.clone(),
                    FlashFont::Normal,
                    16.0,
                    CYAN_TEXT,
                    Align::Left,
                    230.0,
                ),
                [0.0, 0.0, 0.2, 0.9],
            );
            p.text(
                key("ab_udesc", kk * 10 + i),
                s,
                19.0,
                up_y + 33.0 + i as f32 * 17.0,
                &ab,
                Cx::alpha(a),
            );
        }
        if let Some(x) = ability
            && let Some(h) = p.img.wires.get(&x.id).cloned()
        {
            let y = up_y + 33.0 + ulines.len().min(4) as f32 * 17.0 + 10.0;
            let wm = ab
                .then(&M3::translate(40.55, y, 0.0))
                .then(&M3::scale(0.6, 0.6));
            p.canvas().image(
                key("ab_wire", kk),
                &h,
                [0.0, 0.0, 256.0, 256.0],
                &wm,
                Cx::fill([204.0, 255.0, 255.0], a),
            );
        }
    }
}

/// An info panel's title plate (IA9, scale-9, 330 wide) and its text.
fn draw_info_title(p: &mut Painter, k: usize, text: &str, m: &M3, alpha: f32) {
    draw_info_title_plate(p, k, m, alpha);
    let t = text.to_uppercase();
    let s = spec(t, FlashFont::Normal, 24.0, CYAN_TEXT, Align::Left, 350.0);
    p.text(key("info_title_text", k), s, 68.0, 4.0, m, Cx::alpha(alpha));
}

fn draw_info_title_plate(p: &mut Painter, k: usize, m: &M3, alpha: f32) {
    // 300 x 75 art stretched to 330: its band 271-291 takes the extra 30 px.
    p.nine(
        key("info_title_plate", k),
        "info_title",
        [0.0, 0.0, 330.0, 75.0],
        [271.0, 0.0, 9.0, 0.0],
        [271.0 / 300.0, 0.0, 9.0 / 300.0, 0.0],
        m,
        Cx::alpha(alpha),
    );
}

// ---------------------------------------------------------------------------
// The rename prompt
// ---------------------------------------------------------------------------

/// HmUtility.DisplayTextPrompt: "Name Character Build", a line to type in, OK and
/// Cancel, over the dimmed screen.
fn draw_rename(p: &mut Painter, ed: &EditorUi, data: &CustData, now: f32) {
    let Some(r) = &ed.rename else {
        return;
    };
    let k = crate::flash::Ease::StrongOut.apply(((now - r.opened) / 0.2).clamp(0.0, 1.0));
    let id = M3::IDENTITY;
    p.fill(
        key("prompt_dim", 0),
        [-2000.0, -2000.0, STAGE_W + 2000.0, STAGE_H + 2000.0],
        &id,
        [0.0; 3],
        0.6 * k,
    );
    let (x0, y0, x1, y1) = (280.0, 270.0, 840.0, 450.0);
    p.fill(
        key("prompt_box", 0),
        [x0, y0, x1, y1],
        &id,
        [9.0, 34.0, 41.0],
        0.95 * k,
    );
    p.fill(
        key("prompt_edge", 0),
        [x0, y0, x1, y0 + 2.0],
        &id,
        [0.0, 255.0, 211.0],
        0.8 * k,
    );
    p.fill(
        key("prompt_edge", 1),
        [x0, y1 - 2.0, x1, y1],
        &id,
        [0.0, 255.0, 211.0],
        0.8 * k,
    );
    let title = data.string(
        "CustomCharacter.RenameCharacterTitle",
        "Name Character Build",
    );
    p.text(
        key("prompt_title", 0),
        spec(
            title.to_uppercase(),
            FlashFont::Header,
            28.0,
            CYAN_TEXT,
            Align::Left,
            520.0,
        ),
        x0 + 20.0,
        y0 + 14.0,
        &id,
        Cx::alpha(k),
    );
    let desc = data.string(
        "CustomCharacter.RenameCharacterDesc",
        "Enter a new name for this character build.",
    );
    p.text(
        key("prompt_desc", 0),
        spec(
            desc,
            FlashFont::Normal,
            16.0,
            [255, 255, 255],
            Align::Left,
            520.0,
        ),
        x0 + 20.0,
        y0 + 52.0,
        &id,
        Cx::alpha(k),
    );
    // The line being typed, with a blinking caret.
    p.fill(
        key("prompt_field", 0),
        [x0 + 20.0, y0 + 80.0, x1 - 20.0, y0 + 106.0],
        &id,
        [121.0, 181.0, 181.0],
        k,
    );
    p.fill(
        key("prompt_field", 1),
        [x0 + 21.0, y0 + 81.0, x1 - 21.0, y0 + 105.0],
        &id,
        [235.0, 245.0, 245.0],
        k,
    );
    let caret = if ((now - r.opened) * 2.0) as i32 % 2 == 0 {
        "|"
    } else {
        ""
    };
    p.text(
        key("prompt_text", 0),
        spec(
            format!("{}{caret}", r.text),
            FlashFont::Normal,
            18.0,
            [40, 40, 40],
            Align::Left,
            500.0,
        ),
        x0 + 26.0,
        y0 + 83.0,
        &id,
        Cx::alpha(k),
    );
    for (i, (label, target)) in [
        (data.string("ButtonHints.Ok", "OK"), Target::PromptOk),
        (
            data.string("ButtonHints.Cancel", "Cancel"),
            Target::PromptCancel,
        ),
    ]
    .iter()
    .enumerate()
    {
        let bx = x1 - 20.0 - (2 - i) as f32 * 136.0 + 4.0;
        let rect = [bx, y1 - 52.0, bx + 132.0, y1 - 16.0];
        p.fill(key("prompt_button", i), rect, &id, HINT_FILL, 0.8 * k);
        p.text(
            key("prompt_button_text", i),
            spec(
                label.clone(),
                FlashFont::Title,
                14.0,
                HINT_TEXT,
                Align::Center,
                132.0,
            ),
            bx,
            rect[1] + 9.0,
            &id,
            Cx::alpha(k),
        );
        p.hit(*target, rect, &id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every hint's controller button has its picture among the screen's images,
    /// and the picture and its label fit their button, centred where the label
    /// was (the longest label, Unlock Camera, a little left of it, and a label too
    /// long for the button shrunk to fit).
    #[test]
    fn a_hints_button_picture_and_label_fit_its_button() {
        for hint in [
            Hint::ToggleFaction,
            Hint::Close,
            Hint::CameraLock,
            Hint::Randomize,
            Hint::ChangeForm,
            Hint::CyclePage,
            Hint::Back,
        ] {
            let name = hint.pad_button();
            assert!(IMAGES.contains(&name), "{name}");
            let path = crate::assets_dir()
                .join("customize")
                .join("img")
                .join(format!("{name}.png"));
            assert!(
                path.exists() || std::env::var_os("BEVY_ASSET_ROOT").is_none(),
                "{path:?}"
            );
        }
        let cx = 560.0;
        let inside = |lay: &IconHint, words: f32| {
            let right = lay.text_x + words * lay.size / 14.0;
            lay.icon_x >= cx - HINT_W * 0.5 + HINT_ICON_MARGIN - 1e-3
                && right <= cx + HINT_W * 0.5 - HINT_ICON_MARGIN + 1e-3
                && (lay.text_x - lay.icon_x - HINT_ICON - HINT_ICON_GAP).abs() < 1e-4
        };
        // Back (31 px wide): centred on the label's place.
        let lay = hint_with_icon(cx + HINT_TEXT_SHIFT, 31.2);
        assert!(inside(&lay, 31.2));
        assert_eq!(lay.size, 14.0);
        let mid = (lay.icon_x + lay.text_x + 31.2) * 0.5;
        assert!((mid - (cx + HINT_TEXT_SHIFT)).abs() < 1e-3);
        // Unlock Camera (95 px): whole, kept inside.
        let lay = hint_with_icon(cx + HINT_TEXT_SHIFT, 95.3);
        assert!(inside(&lay, 95.3));
        assert_eq!(lay.size, 14.0);
        // Too long: shrunk.
        let lay = hint_with_icon(cx + HINT_TEXT_SHIFT, 140.0);
        assert!(inside(&lay, 140.0));
        assert!(lay.size < 14.0);
    }
}
