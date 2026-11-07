//! Local split screen: a second player on this PC, on a controller of his own, the
//! window shared between the two (Settings > Display > Split Screen: one above the
//! other, or side by side).
//!
//! The second player (the guest) joins in the party lobby ("Press START to join"
//! under the first player's name, while a controller other than his is
//! connected), and comes into the Escalation lobby with him as this PC hosts a
//! game; or in the Escalation lobby itself, while this PC hosts it: by pressing
//! Start on a controller other than the first player's (B leaves again). He takes a slot in the game as a player online would (net.rs's
//! guest): the lobby lists him, he chooses his character after the first player
//! has chosen his, the waves and the team's lives count him. While he is in, the
//! game takes no players online (and he can join only a lobby nobody online has
//! joined). His progress isn't kept: the experience and unlocks are the first
//! player's (xp.rs).
//!
//! Each of this PC's players has his `Seat` (0 the first, 1 the guest), and so has
//! everything that is his: his Jazz (main.rs's LocalPlayer), his camera (its
//! OrbitCamera) drawing into his part of the window, his HUD. The guest's camera
//! is made from the first player's (with its effects, fog and the rest as the
//! Graphics level and the map have them) as he joins, and goes as he leaves.

use bevy::camera::{CameraOutputMode, Viewport};
use bevy::ecs::system::SystemParam;
use bevy::prelude::*;

use crate::menu::DisplaySettings;

/// This PC's players at most: the first player and his guest.
pub const SEATS: usize = 2;

/// The render layer each player's view has of its own, beside the world's (0): the
/// maps' and the weapons' effects as his camera sees them (levelfx.rs: the guest's
/// a copy of each, made for his camera; an effect on a player's screen on his
/// alone).
pub const VIEW_LAYERS: [usize; SEATS] = [11, 12];

/// The layers a game camera sees: the world's and its seat's view's (the
/// characters' shadows' too, shadows.rs).
pub fn view_layers(seat: usize) -> bevy::camera::visibility::RenderLayers {
    bevy::camera::visibility::RenderLayers::from_layers(&[0, VIEW_LAYERS[seat.min(SEATS - 1)]])
}

/// Which of this PC's players something is for (0 the first, 1 the guest).
#[derive(Component, Clone, Copy, PartialEq, Eq, Hash, Debug, Default)]
pub struct Seat(pub usize);

/// The seat of something of this PC's players' (the first player's without one).
pub fn seat_of(seat: Option<&Seat>) -> usize {
    seat.map_or(0, |s| s.0.min(SEATS - 1))
}

/// Each seat's Jazz (main.rs's LocalPlayer), kept as they come and go: whose
/// HUD something that befell a Jazz goes to.
#[derive(Resource, Default, Debug)]
pub struct Seats(pub [Option<Entity>; SEATS]);

impl Seats {
    /// The seat of `jazz`, if he is one of this PC's players.
    pub fn of(&self, jazz: Entity) -> Option<usize> {
        self.0.iter().position(|e| *e == Some(jazz))
    }
}

/// How the window is shared (Split Screen).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Layout {
    /// The first player's view above, the guest's below.
    #[default]
    TopAndBottom,
    /// The first player's on the left, the guest's on the right.
    SideBySide,
}

/// The guest while he is in: the controller he plays with (None once it has gone:
/// he plays on, with no orders, until one is plugged in and he presses Start on it,
/// or he leaves in the lobby).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Guest {
    pub gamepad: Option<Entity>,
}

/// Split screen: the layout, the guest while he is in a game, and Player 2 in the
/// party.
#[derive(Resource, Default)]
pub struct Split {
    pub layout: Layout,
    pub guest: Option<Guest>,
    /// Player 2 in the party lobby (menu.rs's party_guest): the controller he pressed
    /// Start on there. He comes into the next game this PC hosts with the first
    /// player (Host Game or Offline Game: its lobby has him in as it opens), goes
    /// back to the party with him, and stays until he presses B or the party
    /// lobby is left for the main menu.
    pub party: Option<Entity>,
}

impl Split {
    /// The window is shared: there is a guest.
    pub fn active(&self) -> bool {
        self.guest.is_some()
    }

    /// The players on this PC.
    pub fn seats(&self) -> usize {
        1 + usize::from(self.active())
    }

    /// The guest's controller, while he has one.
    pub fn guest_pad(&self) -> Option<Entity> {
        self.guest.and_then(|g| g.gamepad)
    }
}

/// The guest's own copy of a resource that holds one player's state (his HUD's,
/// his energon, his scoreboard numbers, his character...): the first player's is
/// the resource itself.
#[derive(Resource, Default, Debug, Deref, DerefMut)]
pub struct Second<T: Send + Sync + 'static>(pub T);

/// A resource that holds one player's state, for each of this PC's players: the
/// first player's (the resource) and the guest's (`Second`).
#[derive(SystemParam)]
pub struct BySeat<'w, T: Resource> {
    first: ResMut<'w, T>,
    guest: ResMut<'w, Second<T>>,
}

impl<T: Resource> BySeat<'_, T> {
    /// Seat `seat`'s.
    pub fn seat(&mut self, seat: usize) -> &mut T {
        if seat == 1 {
            &mut self.guest.0
        } else {
            &mut self.first
        }
    }

    /// The first player's.
    pub fn first(&mut self) -> &mut T {
        &mut self.first
    }

    /// Both players' (the guest's whether or not he is in).
    pub fn each(&mut self) -> [&mut T; SEATS] {
        [&mut *self.first, &mut self.guest.0]
    }
}

/// The same to read.
#[derive(SystemParam)]
pub struct BySeatRef<'w, T: Resource> {
    first: Res<'w, T>,
    guest: Res<'w, Second<T>>,
}

impl<T: Resource> BySeatRef<'_, T> {
    /// Seat `seat`'s.
    pub fn seat(&self, seat: usize) -> &T {
        if seat == 1 {
            &self.guest.0
        } else {
            &self.first
        }
    }
}

/// Seat `seat`'s part of a picture `size` across (pixels) with `seats` players
/// sharing it, laid out by `layout`: (its top left corner, its size). One player has
/// all of it; two have a half each, a thin black line between them (DIVIDER pixels
/// a 1080-line picture, at least one).
pub fn seat_rect(layout: Layout, seats: usize, seat: usize, size: UVec2) -> (UVec2, UVec2) {
    if seats < 2 {
        return (UVec2::ZERO, size);
    }
    let along = |n: u32| {
        // (The line: as thick for a picture of any size, its pixels split between
        // the halves.)
        let line = ((size.y as f32 * DIVIDER / 1080.0).round() as u32).clamp(1, n / 4);
        let first = (n - line) / 2;
        let second_at = first + line;
        if seat == 0 {
            (0, first)
        } else {
            (second_at, n - second_at)
        }
    };
    match layout {
        Layout::TopAndBottom => {
            let (y, h) = along(size.y);
            (UVec2::new(0, y), UVec2::new(size.x, h))
        }
        Layout::SideBySide => {
            let (x, w) = along(size.x);
            (UVec2::new(x, 0), UVec2::new(w, size.y))
        }
    }
}

/// The line between the halves, in pixels of a 1080-line picture.
const DIVIDER: f32 = 2.0;

/// The width to height the game's field of view (which is across) is taken over
/// for a view `aspect` wide: as it is, as the game takes it, but for a split
/// view's halves, whose views would otherwise come out squeezed (one above the
/// other: a sliver of the height) or stretched (side by side: twice the height), it
/// is kept between the shapes of an ordinary screen (4:3 to 21:9): a half above or
/// below sees farther across, a half beside the other less far across, each
/// keeping a picture that looks like the game's.
pub fn fov_aspect(aspect: f32, split: bool) -> f32 {
    if split {
        aspect.clamp(4.0 / 3.0, 21.0 / 9.0)
    } else {
        aspect
    }
}

/// Whether the guest's controller is `gamepad`.
pub fn is_guest_pad(split: &Split, gamepad: Entity) -> bool {
    split.guest_pad() == Some(gamepad)
}

/// The first player's controller: the first connected one that isn't the guest's.
pub fn first_pad<'a, T>(
    split: &Split,
    mut gamepads: impl Iterator<Item = (Entity, &'a T)>,
) -> Option<(Entity, &'a T)> {
    gamepads.find(|(e, _)| !is_guest_pad(split, *e))
}

pub struct SplitPlugin;

impl Plugin for SplitPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Split>()
            .init_resource::<Seats>()
            // (The guest's own HUD, energon, scoreboard numbers, character and
            // spectating.)
            .init_resource::<Second<crate::hud::HudState>>()
            .init_resource::<Second<crate::Score>>()
            .init_resource::<Second<crate::scoreboard::Tally>>()
            .init_resource::<Second<crate::choose::Chosen>>()
            .init_resource::<Second<crate::revive::Spectate>>()
            .add_systems(
                PreUpdate,
                (follow_layout, follow_net, reclaim_guest_pad, keep_seats),
            )
            // (Before Bevy sizes the cameras' views: their projections take the
            // halves' shapes.)
            .add_systems(
                PostUpdate,
                (guest_view, place_views)
                    .chain()
                    .before(bevy::camera::CameraUpdateSystems),
            );
    }
}

/// Each seat's Jazz.
fn keep_seats(mut seats: ResMut<Seats>, jazzes: Query<(Entity, &Seat), With<crate::LocalPlayer>>) {
    let mut now = [None; SEATS];
    for (jazz, seat) in &jazzes {
        if let Some(at) = now.get_mut(seat.0) {
            *at = Some(jazz);
        }
    }
    if seats.0 != now {
        seats.0 = now;
    }
}

/// Split Screen as the Display settings have it.
fn follow_layout(display: Res<DisplaySettings>, mut split: ResMut<Split>) {
    let layout = if display.split_side_by_side {
        Layout::SideBySide
    } else {
        Layout::TopAndBottom
    };
    if split.layout != layout {
        split.layout = layout;
    }
}

/// The guest goes with the game (leaving it, or the game going: net.rs).
fn follow_net(net: Res<crate::net::Net>, mut split: ResMut<Split>) {
    if split.guest.is_some() && net.guest_slot().is_none() {
        split.guest = None;
        info!("Split screen: the second player has left");
    }
}

/// The guest's controller gone (unplugged), the first other than the first
/// player's that presses Start becomes his.
fn reclaim_guest_pad(
    mut split: ResMut<Split>,
    controls: Res<crate::controls::Controls>,
    gamepads: Query<(Entity, &Gamepad)>,
) {
    let Some(guest) = split.guest else {
        return;
    };
    if guest.gamepad.is_some_and(|e| gamepads.contains(e)) {
        return;
    }
    let first = controls
        .controller
        .then(|| first_pad(&split, gamepads.iter()).map(|(e, _)| e))
        .flatten();
    if let Some((pad, _)) = gamepads
        .iter()
        .find(|(e, g)| Some(*e) != first && g.just_pressed(GamepadButton::Start))
    {
        split.guest = Some(Guest { gamepad: Some(pad) });
        // (In the party with it too.)
        split.party = Some(pad);
        info!("Split screen: the second player plays on {pad:?}");
    }
}

/// The guest's camera: made from the first player's as he comes in (everything it
/// has, its OrbitCamera started afresh), gone as he leaves.
fn guest_view(
    mut commands: Commands,
    split: Res<Split>,
    level: Res<crate::Level>,
    cameras: Query<
        (
            Entity,
            &Seat,
            Option<&bevy::camera::visibility::RenderLayers>,
            &Camera,
        ),
        With<crate::OrbitCamera>,
    >,
) {
    let guest = cameras.iter().find(|(_, s, ..)| s.0 == 1).map(|(e, ..)| e);
    match (split.active(), guest) {
        (true, None) => {
            let Some((first, _, layers, camera)) = cameras.iter().find(|(_, s, ..)| s.0 == 0)
            else {
                return;
            };
            // (An order of its own from the start: below the first player's, as
            // graphics.rs's apply_render_scale keeps it.)
            let camera = Camera {
                order: camera.order - 2,
                ..camera.clone()
            };
            // (Its view's own layer in place of the first player's.)
            let layers = layers
                .cloned()
                .unwrap_or_else(|| view_layers(0))
                .without(VIEW_LAYERS[0])
                .with(VIEW_LAYERS[1]);
            let view = commands
                .entity(first)
                .clone_and_spawn_with_opt_out(|b| {
                    b.deny::<crate::OrbitCamera>();
                    b.deny::<Seat>();
                })
                .insert((crate::fresh_orbit(&level), Seat(1), layers, camera))
                .id();
            info!("Split screen: the second player's camera {view:?}");
        }
        (false, Some(view)) => {
            commands.entity(view).despawn();
        }
        _ => {}
    }
}

/// Each player's camera draws into his part of the picture (all of it alone), and
/// the first player's part is cleared first: the line between the halves black.
/// (Each in its own order, the first player's last: the Render Scale's picture
/// puts them below the camera showing it, graphics.rs.)
fn place_views(
    split: Res<Split>,
    mut cameras: Query<(&mut Camera, &Seat), With<crate::OrbitCamera>>,
) {
    let seats = split.seats();
    for (mut camera, seat) in &mut cameras {
        let Some(size) = camera.physical_target_size() else {
            continue;
        };
        let viewport = (seats > 1 && size.x >= 8 && size.y >= 8).then(|| {
            let (at, size) = seat_rect(split.layout, seats, seat.0, size);
            Viewport {
                physical_position: at,
                physical_size: size,
                ..default()
            }
        });
        let same = match (&camera.viewport, &viewport) {
            (Some(a), Some(b)) => {
                a.physical_position == b.physical_position && a.physical_size == b.physical_size
            }
            (None, None) => true,
            _ => false,
        };
        if !same {
            camera.viewport = viewport;
        }
        let black = seats > 1;
        let is_black = matches!(
            camera.output_mode,
            CameraOutputMode::Write {
                clear_color: ClearColorConfig::Custom(_),
                ..
            }
        );
        if black != is_black {
            camera.output_mode = CameraOutputMode::Write {
                blend_state: None,
                clear_color: if black {
                    ClearColorConfig::Custom(Color::BLACK)
                } else {
                    ClearColorConfig::Default
                },
            };
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn two_players_share_the_window_with_a_line_between() {
        let size = UVec2::new(1920, 1080);
        assert_eq!(
            seat_rect(Layout::TopAndBottom, 1, 0, size),
            (UVec2::ZERO, size)
        );
        let (a, b) = (
            seat_rect(Layout::TopAndBottom, 2, 0, size),
            seat_rect(Layout::TopAndBottom, 2, 1, size),
        );
        assert_eq!(a, (UVec2::ZERO, UVec2::new(1920, 539)));
        assert_eq!(b, (UVec2::new(0, 541), UVec2::new(1920, 539)));
        let (a, b) = (
            seat_rect(Layout::SideBySide, 2, 0, size),
            seat_rect(Layout::SideBySide, 2, 1, size),
        );
        assert_eq!(a, (UVec2::ZERO, UVec2::new(959, 1080)));
        assert_eq!(b, (UVec2::new(961, 0), UVec2::new(959, 1080)));
        // An odd size: the halves the same, the line the rest.
        let odd = UVec2::new(1001, 721);
        let (a, b) = (
            seat_rect(Layout::TopAndBottom, 2, 0, odd),
            seat_rect(Layout::TopAndBottom, 2, 1, odd),
        );
        assert_eq!(a.1.y, b.1.y);
        assert_eq!(b.0.y + b.1.y, 721);
        assert!(b.0.y > a.1.y);
        // A tiny picture still has its halves.
        let tiny = seat_rect(Layout::SideBySide, 2, 1, UVec2::new(8, 8));
        assert!(tiny.1.x >= 3 && tiny.0.x + tiny.1.x == 8);
    }

    #[test]
    fn a_split_view_keeps_an_ordinary_screens_shape_for_its_field_of_view() {
        // Alone, the view's own shape (the game's: its field of view is across).
        assert_eq!(fov_aspect(32.0 / 9.0, false), 32.0 / 9.0);
        assert_eq!(fov_aspect(16.0 / 9.0, true), 16.0 / 9.0);
        // A half of a 16:9 window above the other: as a 21:9 screen sees.
        assert!((fov_aspect(1920.0 / 539.0, true) - 21.0 / 9.0).abs() < 1e-6);
        // Side by side: as a 4:3 screen sees.
        assert!((fov_aspect(959.0 / 1080.0, true) - 4.0 / 3.0).abs() < 1e-6);
    }
}
