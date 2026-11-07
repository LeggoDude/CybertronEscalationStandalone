//! The scoreboard in a match, as Escalation shows it: the Scoreboard key (Tab)
//! brings it up and puts it away (TnHUD's ShowScores, toggling bShowScores: its
//! SetShowScores starts the movie UI_GFxInGameStats_p.InGameStats_GFX and hands
//! it the players, or closes it). It is the movie over the game, which plays on
//! under it: the view darkened, the mode and the map, the mode's rules, and the
//! players' list (pregame.rs's, PlayerList_GFX loaded into it). Esc puts it away
//! too, as the movie's own back does (closeInGameStats, with its sound) rather than
//! bringing up the pause menu; the pause menu, the Choose Character screen and the
//! start of the match (pregame.rs) put it away.
//!
//! The movie, drawn through flash.rs in its numbers (stage pixels):
//!
//! * the view through the starting scoreboard's blue gradient (sprite 2, stretched
//!   over the window, in the multiply blend mode), brought in to 60% over a second;
//! * the mode on the map (gameModeOnMap_txt: Shared.GameModeOnMap's "`g on `m",
//!   the mode's name in white, upper case: Escalation's name is left empty, so it
//!   reads " ON DOWNFALL") in orange, with the mode's rules under it
//!   (gameModeRules_txt: "Survive waves of attacks") in blue, centred at x 560;
//! * the players' list at (290, 218), at 85%: the team, the columns and a row for
//!   each player, sorted as the movie sorts them (score and kills down, deaths up,
//!   then the names), this player's row focused. The movie loads the list in the
//!   'InGame' view (the Infiltrator's ability first); here its first column is the
//!   lobby's Level instead: the symbol of the class each player plays and his
//!   level in it, popping in as the list opens or he changes class.
//!
//! The numbers are each player's match so far, as the game counts them on his
//! TnPlayerReplicationInfoSurvival: his Score (AddScore adds every bit of energon
//! he earns to his score as to his money), Kills, Deaths, Revives (AddRevive: none
//! until reviving is in) and Money (the energon he has to spend). Each PC counts
//! its own player's ([`Tally`]) and sends them with his Jazz (net.rs). The team's
//! score is TnTeamInfo's, which Escalation never scores (its kill rule,
//! TnGameRules_ScoreKillsSV, gives the team nothing): 0, as in the game. Its
//! button hints (staticButtonHints_mc) are left out.

use crate::controls::{Action, Controls, Inputs, Mode};
use crate::flash::{Canvas, Cx, DrawCmd, FlashFont, FlashFrame, FlashView, M3, text_width};
use crate::menu::{AppState, MenuText, Pause};
use crate::net::{MatchStats, Net, ScoreLine};
use crate::pregame::{List, ListView, PreGameImages, PreMatch, Row, draw_list, line, sort_rows};
use crate::sound::Sfx;
use bevy::input::mouse::AccumulatedMouseScroll;
use bevy::prelude::*;
use std::collections::HashMap;

/// The movie's back (closeInGameStats).
const SND_CLOSE: &str = "BL_HUD_INTERFACE.BUTTON_REENTER_GAMEPLAY";

/// Sprite 6 at (560, 159): gameModeOnMap_txt at (-248, 2) in it and
/// gameModeRules_txt at (-248, 20), both 500 wide (their text centred on the
/// sprite) in 16 pixel $NormalFont, #FF9332 and #50B5D5.
const HEADER_AT: (f32, f32) = (560.0, 159.0);
const MAP_Y: f32 = 2.0;
const RULES_Y: f32 = 20.0;
const HEADER_SIZE: f32 = 16.0;
const MAP_COLOR: [u8; 3] = [255, 147, 50];
const RULES_COLOR: [u8; 3] = [80, 181, 213];
/// The mode's name before the map's (GameModeFriendlyName): Escalation's is empty.
const MODE_NAME: &str = "";
/// playerListAnchor_mc at (290, 218), at 85% (sprite 7).
const LIST_AT: (f32, f32) = (290.0, 218.0);
const LIST_SCALE: f32 = 0.85;

/// This PC's player's match so far: his Score (the energon he has earned, as
/// AddScore counts it), his Kills (the enemies he finished), his Deaths and his
/// Revives.
#[derive(Resource, Default, Clone, Copy, Debug, PartialEq, Eq)]
pub struct Tally {
    pub score: u32,
    pub kills: u16,
    pub deaths: u16,
    pub revives: u16,
}

impl Tally {
    /// Energon earned (from a kill of his or a share of one).
    pub fn earned(&mut self, energon: u32) {
        self.score = self.score.saturating_add(energon);
    }

    /// An enemy he finished.
    pub fn killed(&mut self) {
        self.kills = self.kills.saturating_add(1);
    }

    /// He died.
    pub fn died(&mut self) {
        self.deaths = self.deaths.saturating_add(1);
    }
}

/// The scoreboard: when it came up (game time), while it is up; when each player's
/// class came into its list (by his slot); and this PC's player's numbers and class
/// as last sent.
#[derive(Resource, Default)]
pub struct Scoreboard {
    opened: Option<f32>,
    classes: HashMap<usize, (usize, f32)>,
    mine: MatchStats,
    my_class: Option<usize>,
}

impl Scoreboard {
    pub fn is_open(&self) -> bool {
        self.opened.is_some()
    }

    fn open(&mut self, now: f32) {
        self.opened = Some(now);
        self.classes.clear();
    }

    fn close(&mut self) {
        self.opened = None;
    }
}

pub struct ScoreboardPlugin;

impl Plugin for ScoreboardPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Tally>()
            .init_resource::<Scoreboard>()
            .add_systems(OnEnter(AppState::Game), reset)
            .add_systems(
                Update,
                (
                    share_stats,
                    draw_scoreboard.after(crate::pregame::countdown),
                    draw_end_screen,
                )
                    .chain()
                    .run_if(in_state(AppState::Game))
                    .before(crate::flash::FlashSystems),
            );
    }
}

/// A match starts: nothing counted, the scoreboard down.
fn reset(mut tally: crate::split::BySeat<Tally>, mut board: ResMut<Scoreboard>) {
    for tally in tally.each() {
        *tally = Tally::default();
    }
    *board = Scoreboard::default();
}

/// This PC's player's numbers for the scoreboards, his and the other PCs' (net.rs
/// sends them with his Jazz): his tally, his money, and the level of the class he
/// plays (xp.rs). Split screen's guest's too, for his line (net.rs; his class's
/// level as the first player's progress has it: he plays the first player's
/// builds).
fn share_stats(
    tally: crate::split::BySeatRef<Tally>,
    score: crate::split::BySeatRef<crate::Score>,
    chosen: crate::split::BySeatRef<crate::choose::Chosen>,
    progress: Option<Res<crate::xp::Progress>>,
    mut net: ResMut<Net>,
    mut board: ResMut<Scoreboard>,
) {
    for seat in 0..crate::split::SEATS {
        let class = chosen.seat(seat).build.map(|(class, _)| class);
        let level = match (class, progress.as_deref()) {
            (Some(class), Some(progress)) => progress.level(class),
            _ => 0,
        };
        let stats = stats_of(tally.seat(seat), score.seat(seat).energon, level);
        if seat == 0 {
            net.set_stats(stats);
            board.mine = stats;
            board.my_class = class;
        } else {
            net.set_guest_stats(stats);
        }
    }
}

/// The numbers as the scoreboard shows them.
fn stats_of(tally: &Tally, money: u32, level: u32) -> MatchStats {
    MatchStats {
        level: level.min(u8::MAX as u32) as u8,
        score: tally.score,
        kills: tally.kills,
        deaths: tally.deaths,
        revives: tally.revives,
        money,
    }
}

/// The Scoreboard key (a controller's Back: pad.rs) brings it up and puts it away
/// (no sound: SetShowScores closes the movie itself); Esc puts it away with the
/// movie's back sound, and isn't
/// left for the pause menu. Runs before the pause menu's input. Only while the
/// match is in play for this player: not before it has begun for him (pregame.rs),
/// nor under the pause menu or the Choose Character screen.
#[allow(clippy::too_many_arguments)]
pub fn scoreboard_input(
    time: Res<Time>,
    mut keys: ResMut<ButtonInput<KeyCode>>,
    mouse: Res<ButtonInput<MouseButton>>,
    scroll: Res<AccumulatedMouseScroll>,
    (controls, pads): (Res<Controls>, crate::pad::Pads),
    (pause, pre): (Res<Pause>, Res<PreMatch>),
    local: Query<(&crate::Player, Option<&crate::split::Seat>), With<crate::LocalPlayer>>,
    mut board: ResMut<Scoreboard>,
    mut sfx: ResMut<Sfx>,
) {
    if pause.open || pause.choosing || pre.waiting {
        board.close();
        return;
    }
    if board.is_open() && keys.just_pressed(KeyCode::Escape) {
        keys.clear_just_pressed(KeyCode::Escape);
        board.close();
        sfx.play(SND_CLOSE);
        return;
    }
    let inputs = Inputs {
        keys: &keys,
        mouse: &mouse,
        wheel: scroll.delta.y,
    };
    // (Either of this PC's players, by his own controls: split screen's guest by his
    // controller.)
    let asked = local.iter().any(|(p, seat)| {
        let mode = match p.form {
            crate::Form::Vehicle => Mode::Vehicle,
            _ => Mode::Robot,
        };
        let seat = crate::split::seat_of(seat);
        let pad = pads.seat(seat);
        if pad.active || seat == 1 {
            pad.action_pressed(mode, Action::Scoreboard)
        } else {
            controls.pressed(mode, Action::Scoreboard, &inputs)
        }
    });
    if asked {
        if board.is_open() {
            board.close();
        } else {
            board.open(time.elapsed_secs());
        }
    }
}

/// gameModeOnMap_txt's text (updateGameModeOnMap): the mode on the map, upper case.
fn mode_on_map(text: &MenuText, map: &str) -> String {
    text.get("Shared.GameModeOnMap", "`g on `m")
        .replace("`g", MODE_NAME)
        .replace("`m", map)
        .to_uppercase()
}

/// The rows: the players in the match (online), or this player alone (the starting
/// scoreboard's too: pregame.rs).
pub(crate) fn rows(net: &Net, board: &mut Scoreboard, now: f32) -> Vec<Row> {
    let mut lines = net.scoreboard();
    if lines.is_empty() {
        lines.push(ScoreLine {
            slot: 0,
            name: net.name().to_string(),
            me: true,
            class: board.my_class,
            stats: board.mine,
            ping_ms: 0,
        });
    }
    let mut rows: Vec<Row> = lines
        .into_iter()
        .map(|l| {
            let s = l.stats;
            let class = l.class.map(|class| {
                let since = match board.classes.get(&l.slot) {
                    Some(&(c, since)) if c == class => since,
                    _ => {
                        board.classes.insert(l.slot, (class, now));
                        now
                    }
                };
                (class, s.level as u32, now - since)
            });
            Row {
                name: l.name,
                me: l.me,
                ping_ms: l.ping_ms,
                class: class.map(|(c, level, _)| (c, level)),
                class_shown_for: class.map_or(0.0, |(_, _, t)| t),
                numbers: [
                    s.score,
                    s.kills as u32,
                    s.deaths as u32,
                    s.revives as u32,
                    s.money,
                ],
            }
        })
        .collect();
    sort_rows(&mut rows);
    rows
}

#[allow(clippy::too_many_arguments)]
fn draw_scoreboard(
    time: Res<Time>,
    mut board: ResMut<Scoreboard>,
    view: Res<FlashView>,
    text: Option<Res<MenuText>>,
    images: Option<Res<PreGameImages>>,
    net: Res<Net>,
    current_map: Res<crate::CurrentMap>,
    mut frame: ResMut<FlashFrame>,
) {
    let (Some(opened), Some(text)) = (board.opened, text) else {
        return;
    };
    let now = time.elapsed_secs();
    let t = now - opened;
    let mut cmds: Vec<DrawCmd> = Vec::new();
    let mut c = Canvas { cmds: &mut cmds };
    if let Some(img) = images.as_deref() {
        crate::pregame::draw_shade(&mut c, img, &view, t);
    }
    let map = current_map.0.and_then(|m| crate::MAPS.get(m));
    // The mode on the map, and its rules.
    let map_name = map.map_or(String::new(), |m| text.get(m.name_key, m.name));
    for (i, (s, y, color)) in [
        (mode_on_map(&text, &map_name), MAP_Y, MAP_COLOR),
        (
            text.get(
                "TransGame.TnOnlineGameSettingsSV.RulesFormat",
                "Survive waves of attacks",
            ),
            RULES_Y,
            RULES_COLOR,
        ),
    ]
    .into_iter()
    .enumerate()
    {
        let w = text_width(&text, FlashFont::Normal, HEADER_SIZE, &s);
        c.text(
            crate::flash::key("sb_header", i),
            line(s, HEADER_SIZE, color, w + 4.0, None),
            HEADER_AT.0 - w * 0.5,
            HEADER_AT.1 + y,
            &M3::IDENTITY,
            Cx::IDENTITY,
        );
    }
    let rows = rows(&net, &mut board, now);
    draw_list(
        &mut c,
        images.as_deref(),
        &text,
        &List {
            view: ListView::Level,
            rows: &rows,
            team_score: 0,
            decepticons: map.is_some_and(|m| m.decepticons),
            at: M3::translate(LIST_AT.0, LIST_AT.1, 0.0).then(&M3::scale(LIST_SCALE, LIST_SCALE)),
            t,
        },
    );
    frame.cmds.extend(cmds);
}

/// The game over (revive.rs: the team's last life lost; or waves.rs: its last wave
/// won): the final scores as the
/// game shows them as an Escalation game ends (EndGameStats_GFX, over the view;
/// an iconic mode, so its scoreboard view alone, with no experience tally and, in
/// a match, no button hints), drawn through flash.rs in its numbers: the view
/// through the starting scoreboard's gradient (background_mc, multiplied) brought
/// in over a second; footer_mc's and header_mc's bands of light and the game
/// mode's icon, as the start of the match has them (pregame.rs); header_mc's
/// title (setTitle: the game over message, "You survived until wave `r", upper
/// case, 40 pixel $TitleFont, white), sliding in from 32 pixels left as it fades in
/// over half a second; and the players' list at (320, 162), as the scoreboard in
/// the match shows it. After END_SCREEN_SECS the host ends the match (revive.rs)
/// and every player is back in the lobby.
#[allow(clippy::too_many_arguments)]
fn draw_end_screen(
    (team, waves): (Res<crate::revive::Team>, Res<crate::waves::Waves>),
    mut board: ResMut<Scoreboard>,
    time: Res<Time>,
    view: Res<FlashView>,
    text: Option<Res<MenuText>>,
    images: Option<Res<PreGameImages>>,
    net: Res<Net>,
    current_map: Res<crate::CurrentMap>,
    mut frame: ResMut<FlashFrame>,
) {
    let Some(text) = text.filter(|_| team.over) else {
        return;
    };
    // (It takes the scoreboard's place.)
    board.close();
    let t = team.over_for;
    let now = time.elapsed_secs();
    let mut cmds: Vec<DrawCmd> = Vec::new();
    let mut c = Canvas { cmds: &mut cmds };
    if let Some(img) = images.as_deref() {
        let a = crate::flash::Ease::RegularOut.apply(t / END_SHADE_SECS);
        crate::pregame::shade(&mut c, img, &view, a);
        crate::pregame::draw_bands(&mut c, img, t);
    }
    let k = crate::flash::Ease::RegularOut.apply(t / END_TITLE_SECS);
    let title = END_TITLE
        .replace("`r", &waves.reached().to_string())
        .to_uppercase();
    let w = text_width(&text, FlashFont::Title, END_TITLE_SIZE, &title);
    let mut spec = line(title, END_TITLE_SIZE, [255, 255, 255], w + 4.0, None);
    spec.font = FlashFont::Title;
    let x = END_TITLE_X.0 + (END_TITLE_X.1 - END_TITLE_X.0) * k;
    c.text(
        crate::flash::key("end_title", 0),
        spec,
        END_HEADER_AT.0 + x,
        END_HEADER_AT.1 + END_TITLE_Y,
        &M3::IDENTITY,
        Cx::alpha(k),
    );
    let map = current_map.0.and_then(|m| crate::MAPS.get(m));
    let rows = rows(&net, &mut board, now);
    draw_list(
        &mut c,
        images.as_deref(),
        &text,
        &List {
            view: ListView::Level,
            rows: &rows,
            team_score: 0,
            decepticons: map.is_some_and(|m| m.decepticons),
            at: M3::translate(END_LIST_AT.0, END_LIST_AT.1, 0.0),
            t,
        },
    );
    frame.cmds.extend(cmds);
}

/// background_mc's fade in (interp 1 s, to full).
const END_SHADE_SECS: f32 = 1.0;
/// header_mc at (-103, 172) (the scoreboard view's ScoreboardHeaderY), its
/// title_txt 96 pixels up, setTitle sliding it from x 380 to 412 over half a
/// second as it fades in.
const END_HEADER_AT: (f32, f32) = (-103.0, 172.0);
const END_TITLE_Y: f32 = -96.0;
const END_TITLE_X: (f32, f32) = (380.0, 412.0);
const END_TITLE_SECS: f32 = 0.5;
const END_TITLE_SIZE: f32 = 40.0;
/// The game over message (TransContent.int TnSurvivalGameOverMessage: the wave
/// reached as `r, waves.rs), the title's.
const END_TITLE: &str = "You survived until wave `r";
/// scoreboard_mc's place, the list loaded into its playerListAnchor_mc.
const END_LIST_AT: (f32, f32) = (320.0, 162.0);
/// How long the final scores show before the match ends (the host's: revive.rs).
pub const END_SCREEN_SECS: f32 = 10.0;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_tally_counts_what_the_game_counts() {
        let mut tally = Tally::default();
        tally.earned(150);
        tally.killed();
        tally.earned(40);
        tally.died();
        assert_eq!(
            tally,
            Tally {
                score: 190,
                kills: 1,
                deaths: 1,
                revives: 0,
            }
        );
        // Money is what is left to spend; the score keeps all that was earned.
        let stats = stats_of(&tally, 25, 7);
        assert_eq!((stats.score, stats.money, stats.level), (190, 25, 7));
        // (Levels past what a byte holds stay at its top.)
        assert_eq!(stats_of(&tally, 0, 300).level, u8::MAX);
    }

    #[test]
    fn the_header_names_the_map_after_an_empty_mode() {
        let text = MenuText::default();
        assert_eq!(mode_on_map(&text, "Downfall"), " ON DOWNFALL");
    }

    #[test]
    fn a_class_pops_in_when_it_first_shows_and_again_when_it_changes() {
        let mut net = Net::default();
        net.set_name("Chris");
        let mut board = Scoreboard {
            my_class: Some(2),
            mine: MatchStats {
                level: 12,
                score: 300,
                ..default()
            },
            ..default()
        };
        let first = rows(&net, &mut board, 10.0);
        assert_eq!(first.len(), 1);
        assert_eq!(first[0].class, Some((2, 12)));
        assert_eq!(first[0].class_shown_for, 0.0);
        assert_eq!(first[0].numbers, [300, 0, 0, 0, 0]);
        let later = rows(&net, &mut board, 10.5);
        assert_eq!(later[0].class_shown_for, 0.5);
        board.my_class = Some(0);
        let changed = rows(&net, &mut board, 11.0);
        assert_eq!(changed[0].class, Some((0, 12)));
        assert_eq!(changed[0].class_shown_for, 0.0);
    }
}
