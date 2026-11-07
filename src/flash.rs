//! A small Scaleform-style 3D UI layer, for menus whose clips turn and move in depth.
//!
//! The game's menus are Scaleform (Flash) movies, and some of them use Scaleform's
//! 3D extensions: clips with `_z`, `_xrotation` and `_yrotation`, seen through a
//! 55-degree perspective (the customize screen's class tags and its editor's page
//! stack). This module draws such a menu the way Scaleform does:
//!
//! * Every image or line of text is a flat quad placed by its clip's full 3D matrix
//!   (see [`M3`] and [`Clip`]), in Flash's coordinates (stage pixels, y down, z into
//!   the screen; `_z` is in twips).
//! * A camera on its own render layer ([`FLASH_LAYER`]) looks at the stage from
//!   where Scaleform's projection puts the eye, so the quads land where the movie
//!   would draw them. It draws over everything (order 11).
//! * Quads are drawn in the order they are submitted (Flash's display-list order,
//!   whatever their depth), through a material that applies Flash colour transforms
//!   (`assets/shaders/flash.wgsl`), blended over what is behind them as Flash
//!   blends (normally, or with the add and multiply blend modes: [`Blend`]).
//! * Text is drawn by Bevy's UI into a texture atlas (one hidden camera), and each
//!   line is a quad cut from it.
//!
//! Callers describe the frame each update by pushing [`DrawCmd`]s (through a
//! [`Canvas`]) into [`FlashFrame`]; `apply_flash` turns them into entities, keyed so
//! the same element keeps its entity from frame to frame. Elements not drawn in a
//! frame are hidden.

use crate::menu::MenuText;
use bevy::asset::RenderAssetUsages;
use bevy::camera::RenderTarget;
use bevy::camera::visibility::RenderLayers;
use bevy::core_pipeline::tonemapping::Tonemapping;
use bevy::mesh::{Indices, MeshVertexBufferLayoutRef};
use bevy::pbr::{MaterialPipeline, MaterialPipelineKey};
use bevy::prelude::*;
use bevy::reflect::TypePath;
use bevy::render::render_resource::{
    AsBindGroup, BlendComponent, BlendFactor, BlendOperation, BlendState, Extent3d,
    PrimitiveTopology, RenderPipelineDescriptor, SpecializedMeshPipelineError, TextureDimension,
    TextureFormat, TextureUsages,
};
use bevy::render::view::Hdr;
use bevy::shader::ShaderRef;
use bevy::window::PrimaryWindow;
use std::collections::HashMap;

/// Render layer of the 3D menu layer and its camera.
pub const FLASH_LAYER: usize = 3;
/// The movies' stage.
pub const STAGE_W: f32 = 1120.0;
pub const STAGE_H: f32 = 720.0;
/// Scaleform's default perspective: a 55-degree field of view across the frame.
const FOV_DEGREES: f32 = 55.0;
/// Draw-order step, added to the distance the transparent pass sorts by (much more
/// than any depth difference on the stage).
const ORDER_STEP: f32 = 1000.0;
/// The text atlas's size (texels).
const ATLAS_W: u32 = 4096;
const ATLAS_H: u32 = 2048;
/// Text is drawn into the atlas at the window's pixel size, but no finer than
/// this many texels per stage pixel (a 2560 x 1440 window is 2): the whole
/// screen's text then fits the atlas.
const MAX_TEXT_SCALE: f32 = 2.5;
/// A full atlas starts over (and its lines are drawn again) at most this often.
const ATLAS_RESET_SECS: f32 = 1.0;
/// Space around each line of text in the atlas (texels).
const ATLAS_PAD: u32 = 3;

pub struct FlashPlugin;

impl Plugin for FlashPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(MaterialPlugin::<CxMaterial>::default())
            .init_resource::<FlashFrame>()
            .init_resource::<FlashView>()
            .add_systems(Startup, setup_flash)
            .add_systems(First, update_flash_view)
            .add_systems(Update, apply_flash.in_set(FlashSystems));
    }
}

/// Where the frame's draw commands become entities: systems that draw run before it.
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub struct FlashSystems;

// ---------------------------------------------------------------------------
// Math: Flash's 3D clip transforms (plain arrays, so they can be tested alone)
// ---------------------------------------------------------------------------

/// An affine 3D transform in Flash's coordinates (x right, y down, z into the
/// screen, in stage pixels): p' = m * (x, y, z, 1), rows of m.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct M3 {
    pub m: [[f32; 4]; 3],
}

impl M3 {
    pub const IDENTITY: M3 = M3 {
        m: [
            [1.0, 0.0, 0.0, 0.0],
            [0.0, 1.0, 0.0, 0.0],
            [0.0, 0.0, 1.0, 0.0],
        ],
    };

    pub fn translate(x: f32, y: f32, z: f32) -> M3 {
        let mut t = M3::IDENTITY;
        t.m[0][3] = x;
        t.m[1][3] = y;
        t.m[2][3] = z;
        t
    }

    pub fn scale(sx: f32, sy: f32) -> M3 {
        let mut t = M3::IDENTITY;
        t.m[0][0] = sx;
        t.m[1][1] = sy;
        t
    }

    /// A Flash 2D matrix (a, b, c, d, tx, ty): x' = a x + c y + tx, y' = b x + d y + ty.
    pub fn flat(a: f32, b: f32, c: f32, d: f32, tx: f32, ty: f32) -> M3 {
        M3 {
            m: [[a, c, 0.0, tx], [b, d, 0.0, ty], [0.0, 0.0, 1.0, 0.0]],
        }
    }

    /// `_yrotation` (degrees): positive brings the clip's right edge toward the viewer.
    pub fn rot_y(degrees: f32) -> M3 {
        let (s, c) = degrees.to_radians().sin_cos();
        M3 {
            m: [[c, 0.0, s, 0.0], [0.0, 1.0, 0.0, 0.0], [-s, 0.0, c, 0.0]],
        }
    }

    /// `_xrotation` (degrees).
    pub fn rot_x(degrees: f32) -> M3 {
        let (s, c) = degrees.to_radians().sin_cos();
        M3 {
            m: [[1.0, 0.0, 0.0, 0.0], [0.0, c, -s, 0.0], [0.0, s, c, 0.0]],
        }
    }

    /// `_rotation` (degrees, clockwise on screen).
    pub fn rot_z(degrees: f32) -> M3 {
        let (s, c) = degrees.to_radians().sin_cos();
        M3 {
            m: [[c, -s, 0.0, 0.0], [s, c, 0.0, 0.0], [0.0, 0.0, 1.0, 0.0]],
        }
    }

    /// self * other: `other` applies first (a child's matrix inside its parent's).
    pub fn then(&self, other: &M3) -> M3 {
        let a = &self.m;
        let b = &other.m;
        let mut out = [[0.0f32; 4]; 3];
        for (i, row) in out.iter_mut().enumerate() {
            for (j, v) in row.iter_mut().enumerate() {
                *v = a[i][0] * b[0][j] + a[i][1] * b[1][j] + a[i][2] * b[2][j];
                if j == 3 {
                    *v += a[i][3];
                }
            }
        }
        M3 { m: out }
    }

    pub fn point(&self, p: [f32; 3]) -> [f32; 3] {
        let m = &self.m;
        [
            m[0][0] * p[0] + m[0][1] * p[1] + m[0][2] * p[2] + m[0][3],
            m[1][0] * p[0] + m[1][1] * p[1] + m[1][2] * p[2] + m[1][3],
            m[2][0] * p[0] + m[2][1] * p[1] + m[2][2] * p[2] + m[2][3],
        ]
    }

    pub fn vector(&self, v: [f32; 3]) -> [f32; 3] {
        let m = &self.m;
        [
            m[0][0] * v[0] + m[0][1] * v[1] + m[0][2] * v[2],
            m[1][0] * v[0] + m[1][1] * v[1] + m[1][2] * v[2],
            m[2][0] * v[0] + m[2][1] * v[1] + m[2][2] * v[2],
        ]
    }

    /// Column-major 4x4 (for Bevy's Mat4::from_cols_array).
    pub fn cols(&self) -> [f32; 16] {
        let m = &self.m;
        [
            m[0][0], m[1][0], m[2][0], 0.0, //
            m[0][1], m[1][1], m[2][1], 0.0, //
            m[0][2], m[1][2], m[2][2], 0.0, //
            m[0][3], m[1][3], m[2][3], 1.0,
        ]
    }
}

/// A clip's placement the way ActionScript sets it: `_x`, `_y`, `_z` (twips),
/// `_xscale`/`_yscale` (1.0 = 100), `_xrotation`, `_yrotation` and `_rotation`
/// (degrees). Its matrix is T(x, y, z/20) Ry Rx Rz S.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Clip {
    pub x: f32,
    pub y: f32,
    pub z: f32,
    pub xscale: f32,
    pub yscale: f32,
    pub xrot: f32,
    pub yrot: f32,
    pub rot: f32,
}

impl Clip {
    pub const fn at(x: f32, y: f32) -> Clip {
        Clip {
            x,
            y,
            z: 0.0,
            xscale: 1.0,
            yscale: 1.0,
            xrot: 0.0,
            yrot: 0.0,
            rot: 0.0,
        }
    }

    pub fn matrix(&self) -> M3 {
        let mut m = M3::translate(self.x, self.y, self.z / 20.0);
        if self.yrot != 0.0 {
            m = m.then(&M3::rot_y(self.yrot));
        }
        if self.xrot != 0.0 {
            m = m.then(&M3::rot_x(self.xrot));
        }
        if self.rot != 0.0 {
            m = m.then(&M3::rot_z(self.rot));
        }
        m.then(&M3::scale(self.xscale, self.yscale))
    }
}

/// The perspective's focal length (stage pixels) for a frame `visible_width` stage
/// pixels wide.
pub fn focal_length(visible_width: f32) -> f32 {
    visible_width * 0.5 / (FOV_DEGREES * 0.5).to_radians().tan()
}

/// Where a point (Flash 3D, stage pixels) is seen on the stage.
#[cfg(test)]
fn project(p: [f32; 3], focal: f32) -> [f32; 2] {
    let s = focal / (focal + p[2]).max(1e-3);
    let (cx, cy) = (STAGE_W * 0.5, STAGE_H * 0.5);
    [cx + (p[0] - cx) * s, cy + (p[1] - cy) * s]
}

/// Where the eye's ray through stage point `at` meets the plane of a clip with
/// matrix `m`, in the clip's own coordinates; None if it runs along the plane or
/// meets it behind the eye.
pub fn unproject(at: [f32; 2], focal: f32, m: &M3) -> Option<[f32; 2]> {
    let eye = [STAGE_W * 0.5, STAGE_H * 0.5, -focal];
    let dir = [at[0] - eye[0], at[1] - eye[1], focal];
    let o = m.point([0.0, 0.0, 0.0]);
    let u = m.vector([1.0, 0.0, 0.0]);
    let v = m.vector([0.0, 1.0, 0.0]);
    let n = cross(u, v);
    let denom = dot(dir, n);
    if denom.abs() < 1e-9 {
        return None;
    }
    let t = dot(sub(o, eye), n) / denom;
    if t <= 0.0 {
        return None;
    }
    let hit = [
        eye[0] + dir[0] * t,
        eye[1] + dir[1] * t,
        eye[2] + dir[2] * t,
    ];
    // Solve hit - o = a u + b v (least squares on the plane).
    let d = sub(hit, o);
    let (uu, uv, vv) = (dot(u, u), dot(u, v), dot(v, v));
    let (du, dv) = (dot(d, u), dot(d, v));
    let det = uu * vv - uv * uv;
    if det.abs() < 1e-12 {
        return None;
    }
    Some([(du * vv - dv * uv) / det, (dv * uu - du * uv) / det])
}

fn cross(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

fn dot(a: [f32; 3], b: [f32; 3]) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn sub(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

/// The easing functions the movies use (mx.transitions.easing, called as
/// f(p) = ease(p, 0, 1, 1)).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Ease {
    Linear,
    RegularIn,
    RegularOut,
    RegularInOut,
    StrongOut,
}

impl Ease {
    pub fn apply(self, p: f32) -> f32 {
        let p = p.clamp(0.0, 1.0);
        match self {
            Ease::Linear => p,
            Ease::RegularIn => p * p,
            Ease::RegularOut => p * (2.0 - p),
            Ease::RegularInOut => {
                let t = p * 2.0;
                if t < 1.0 {
                    0.5 * t * t
                } else {
                    let t = t - 1.0;
                    -0.5 * (t * (t - 2.0) - 1.0)
                }
            }
            Ease::StrongOut => 1.0 - (1.0 - p).powi(5),
        }
    }
}

/// A value moving from one number to another over time (hms.motion.Tween's
/// tweenTo: a new tween starts from wherever the value is).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Tween {
    from: f32,
    to: f32,
    start: f32,
    secs: f32,
    ease: Ease,
}

impl Tween {
    pub const fn at(v: f32) -> Tween {
        Tween {
            from: v,
            to: v,
            start: 0.0,
            secs: 0.0,
            ease: Ease::Linear,
        }
    }

    pub fn get(&self, now: f32) -> f32 {
        if self.secs <= 0.0 || now >= self.start + self.secs {
            return self.to;
        }
        let p = ((now - self.start) / self.secs).max(0.0);
        self.from + (self.to - self.from) * self.ease.apply(p)
    }

    /// Where it is going.
    pub fn target(&self) -> f32 {
        self.to
    }

    /// Tween from where it is now to `to`.
    pub fn to(&mut self, now: f32, to: f32, secs: f32, ease: Ease) {
        let from = self.get(now);
        *self = Tween {
            from,
            to,
            start: now,
            secs,
            ease,
        };
    }

    pub fn from_to(&mut self, now: f32, from: f32, to: f32, secs: f32, ease: Ease) {
        *self = Tween {
            from,
            to,
            start: now,
            secs,
            ease,
        };
    }
}

/// A key for a drawn element: the same key in the next frame is the same entity.
pub fn key(name: &str, index: usize) -> u64 {
    // FNV-1a over the name, then the index.
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in name.bytes().chain((index as u64).to_le_bytes()) {
        h ^= b as u64;
        h = h.wrapping_mul(0x0100_0000_01b3);
    }
    h
}

// ---------------------------------------------------------------------------
// Colour transforms
// ---------------------------------------------------------------------------

/// A Flash colour transform, out = in * mul + add, worked out on sRGB values (0-1;
/// alpha included).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Cx {
    pub mul: [f32; 4],
    pub add: [f32; 4],
}

impl Cx {
    pub const IDENTITY: Cx = Cx {
        mul: [1.0; 4],
        add: [0.0; 4],
    };

    pub fn alpha(a: f32) -> Cx {
        Cx {
            mul: [1.0, 1.0, 1.0, a],
            add: [0.0; 4],
        }
    }

    /// A colour transform from the movie: multipliers (0-1) and offsets (0-255).
    pub fn new(mul: [f32; 4], add_255: [f32; 4]) -> Cx {
        Cx {
            mul,
            add: add_255.map(|v| v / 255.0),
        }
    }

    /// Flat colour (sRGB 0-255) at `alpha` of the shape's own alpha: the movie's
    /// (0,0,0,a, r,g,b,0) recolouring.
    pub fn fill(rgb: [f32; 3], alpha: f32) -> Cx {
        Cx {
            mul: [0.0, 0.0, 0.0, alpha],
            add: [rgb[0] / 255.0, rgb[1] / 255.0, rgb[2] / 255.0, 0.0],
        }
    }
}

// ---------------------------------------------------------------------------
// What callers draw
// ---------------------------------------------------------------------------

/// The fonts the movies use (their TrueType copies in assets/menu/fonts).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum FlashFont {
    /// $NormalFont: Register Demi Bold.
    Normal,
    /// $TitleFont: Register Medium.
    Title,
    /// $HeaderFont: Register Light.
    Header,
    /// $NumberFontWide: Energon Numbers Wide (digits, and "*", the shards' sign).
    NumberWide,
}

impl FlashFont {
    /// The name menu.txt keeps its advance widths under.
    pub fn name(self) -> &'static str {
        match self {
            FlashFont::Normal => "RegisterDemiBold",
            FlashFont::Title => "RegisterMedium",
            FlashFont::Header => "RegisterLight",
            FlashFont::NumberWide => "EnergonNumbersWide",
        }
    }

    fn file(self) -> &'static str {
        match self {
            FlashFont::Normal => "menu/fonts/RegisterDemiBold.ttf",
            FlashFont::Title => "menu/fonts/RegisterMedium.ttf",
            FlashFont::Header => "menu/fonts/RegisterLight.ttf",
            FlashFont::NumberWide => "menu/fonts/EnergonNumbersWide.ttf",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Align {
    Left,
    Center,
    Right,
}

/// A line of text in a field `width` wide, its top-left at the field's origin
/// (Flash's first line starts 2 px into a field whose bounds start at -2).
#[derive(Clone, Debug, PartialEq)]
pub struct TextSpec {
    pub text: String,
    pub font: FlashFont,
    /// Pixels (the field's font height).
    pub size: f32,
    /// sRGB 0-255.
    pub color: [u8; 3],
    pub align: Align,
    pub width: f32,
    /// A drop shadow: offset (stage pixels) and colour (sRGBA 0-1).
    pub shadow: Option<([f32; 2], [f32; 4])>,
    /// A glow round the letters (a GlowFilter, or a DropShadowFilter at distance 0).
    pub glow: Option<Glow>,
    /// How much bigger than its stage size the text is usually seen (its clips'
    /// scale), so its atlas copy is drawn that much finer.
    pub zoom: f32,
}

/// A Flash glow filter on a text field: the letters' outline blurred by a box
/// `blur` stage pixels across (its blurX and blurY), made `strength` times as
/// opaque (to full at most), in `color` (sRGB 0-255), under the letters. A
/// DropShadowFilter with no distance is the same in its colour.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Glow {
    pub color: [u8; 3],
    pub strength: f32,
    pub blur: f32,
}

/// How an element is blended over what is behind it: Flash's blend modes (a clip's
/// blendMode) the movies use.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Blend {
    /// Over it, by its alpha.
    #[default]
    Normal,
    /// Added to it ('add': its colour times its alpha).
    Add,
    /// Multiplying it ('multiply': by its colour, as much as its alpha says).
    Multiply,
}

#[derive(Clone, Debug)]
pub enum Draw {
    /// An image stretched over the rectangle, or plain colour (no image: white
    /// through the colour transform).
    Image {
        image: Option<Handle<Image>>,
        /// Scale-9: the borders (left, top, right, bottom) in the rectangle's
        /// units, and the same borders as parts of the texture (0-1).
        nine: Option<([f32; 4], [f32; 4])>,
    },
    Text(TextSpec),
}

#[derive(Clone, Debug)]
pub struct DrawCmd {
    pub key: u64,
    pub draw: Draw,
    /// x0 y0 x1 y1 in the space of `matrix` (for text: its field's origin and
    /// width are used).
    pub rect: [f32; 4],
    pub matrix: M3,
    pub cx: Cx,
    pub blend: Blend,
}

/// This frame's draw commands, in drawing order (bottom first).
#[derive(Resource, Default)]
pub struct FlashFrame {
    pub cmds: Vec<DrawCmd>,
}

/// Helps build a frame: elements under a matrix and colour transform.
pub struct Canvas<'a> {
    pub cmds: &'a mut Vec<DrawCmd>,
}

impl Canvas<'_> {
    pub fn image(&mut self, key: u64, image: &Handle<Image>, rect: [f32; 4], m: &M3, cx: Cx) {
        self.push(key, Some(image.clone()), None, rect, m, cx);
    }

    /// A scale-9 image: `borders` (left, top, right, bottom) keep their size in the
    /// rectangle's units; `tex_borders` are the same borders in texture parts.
    #[allow(clippy::too_many_arguments)]
    pub fn nine(
        &mut self,
        key: u64,
        image: &Handle<Image>,
        rect: [f32; 4],
        borders: [f32; 4],
        tex_borders: [f32; 4],
        m: &M3,
        cx: Cx,
    ) {
        self.push(
            key,
            Some(image.clone()),
            Some((borders, tex_borders)),
            rect,
            m,
            cx,
        );
    }

    /// Plain colour over a rectangle.
    pub fn fill(&mut self, key: u64, rect: [f32; 4], m: &M3, rgb: [f32; 3], alpha: f32) {
        self.push(key, None, None, rect, m, Cx::fill(rgb, alpha));
    }

    /// An image (scale-9 with `nine`: borders and texture borders, as `nine` takes
    /// them; plain colour without an image) in a blend mode.
    #[allow(clippy::too_many_arguments)]
    pub fn blended(
        &mut self,
        key: u64,
        image: Option<&Handle<Image>>,
        nine: Option<([f32; 4], [f32; 4])>,
        rect: [f32; 4],
        m: &M3,
        cx: Cx,
        blend: Blend,
    ) {
        let before = self.cmds.len();
        self.push(key, image.cloned(), nine, rect, m, cx);
        if let Some(cmd) = self.cmds.get_mut(before) {
            cmd.blend = blend;
        }
    }

    /// A line of text with its field's top-left at (x, y).
    pub fn text(&mut self, key: u64, spec: TextSpec, x: f32, y: f32, m: &M3, cx: Cx) {
        if cx.mul[3] <= 0.003 && cx.add[3] <= 0.003 {
            return;
        }
        let w = spec.width;
        self.cmds.push(DrawCmd {
            key,
            draw: Draw::Text(spec),
            rect: [x, y, x + w, y],
            matrix: *m,
            cx,
            blend: Blend::Normal,
        });
    }

    fn push(
        &mut self,
        key: u64,
        image: Option<Handle<Image>>,
        nine: Option<([f32; 4], [f32; 4])>,
        rect: [f32; 4],
        m: &M3,
        cx: Cx,
    ) {
        // Fully transparent: nothing to draw.
        if cx.mul[3] <= 0.003 && cx.add[3] <= 0.003 {
            return;
        }
        if rect[2] <= rect[0] || rect[3] <= rect[1] {
            return;
        }
        self.cmds.push(DrawCmd {
            key,
            draw: Draw::Image { image, nine },
            rect,
            matrix: *m,
            cx,
            blend: Blend::Normal,
        });
    }
}

/// The width of a line of text as Flash lays it out (the sum of the font's advances).
pub(crate) fn text_width(text: &MenuText, font: FlashFont, size: f32, s: &str) -> f32 {
    text.width(font.name(), s, size)
}

// ---------------------------------------------------------------------------
// The stage in the window
// ---------------------------------------------------------------------------

/// How the stage sits in the window: Scaleform's ShowAll (scaled to fit, centred),
/// and the perspective that goes with the frame's visible width.
#[derive(Resource, Clone, Copy, Debug, PartialEq)]
pub struct FlashView {
    /// Window pixels (logical) per stage pixel.
    pub scale: f32,
    /// Where the stage's top-left is in the window.
    pub offset: Vec2,
    pub window: Vec2,
    /// The perspective's focal length (stage pixels).
    pub focal: f32,
    /// The window's scale factor (physical pixels per logical pixel).
    pub dpi: f32,
}

impl Default for FlashView {
    fn default() -> Self {
        FlashView::new(Vec2::new(1280.0, 720.0), 1.0)
    }
}

impl FlashView {
    pub fn new(window: Vec2, dpi: f32) -> FlashView {
        let window = window.max(Vec2::ONE);
        let scale = (window.x / STAGE_W).min(window.y / STAGE_H);
        FlashView {
            scale,
            offset: (window - Vec2::new(STAGE_W, STAGE_H) * scale) * 0.5,
            window,
            focal: focal_length(window.x / scale),
            dpi,
        }
    }

    /// A window point (logical pixels) on the stage.
    pub fn to_stage(&self, p: Vec2) -> Vec2 {
        (p - self.offset) / self.scale
    }

    /// The stage's visible height (stage pixels).
    fn visible_height(&self) -> f32 {
        self.window.y / self.scale
    }
}

fn update_flash_view(
    window: Option<Single<&Window, With<PrimaryWindow>>>,
    mut view: ResMut<FlashView>,
) {
    let Some(window) = window else {
        return;
    };
    let new = FlashView::new(
        Vec2::new(window.width(), window.height()),
        window.scale_factor(),
    );
    if *view != new {
        *view = new;
    }
}

// ---------------------------------------------------------------------------
// Material
// ---------------------------------------------------------------------------

/// A texture (white without one), or a part of it, through a Flash colour
/// transform worked out in sRGB as Flash does. The output is premultiplied, or
/// for the multiply blend mode the factor to multiply by.
#[derive(Asset, TypePath, AsBindGroup, Debug, Clone)]
#[bind_group_data(CxKey)]
pub struct CxMaterial {
    #[uniform(0)]
    mul: Vec4,
    #[uniform(1)]
    add: Vec4,
    /// x: 2 for the multiply blend mode (else 0); y: 1 when the texture's colours
    /// are premultiplied; z: a text glow's half width (texels, 0 for none) and w
    /// its strength.
    #[uniform(2)]
    params: Vec4,
    /// The texture's part shown: xy offset, zw size.
    #[uniform(3)]
    uv_rect: Vec4,
    #[texture(4)]
    #[sampler(5)]
    texture: Option<Handle<Image>>,
    /// The glow's colour (sRGB 0-1).
    #[uniform(6)]
    glow: Vec4,
    /// Draw order (see `depth_bias`).
    order: f32,
    blend: Blend,
}

/// What a CxMaterial's pipeline depends on: its blend mode.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct CxKey {
    blend: Blend,
}

impl From<&CxMaterial> for CxKey {
    fn from(material: &CxMaterial) -> Self {
        CxKey {
            blend: material.blend,
        }
    }
}

impl Material for CxMaterial {
    fn fragment_shader() -> ShaderRef {
        "shaders/flash.wgsl".into()
    }

    fn alpha_mode(&self) -> AlphaMode {
        AlphaMode::Premultiplied
    }

    /// Added to the distance the transparent pass sorts by: elements are drawn in
    /// the order they were submitted, whatever their depth (as Flash does).
    fn depth_bias(&self) -> f32 {
        self.order
    }

    fn enable_prepass() -> bool {
        false
    }

    fn enable_shadows() -> bool {
        false
    }

    /// No back-face culling: clips can turn away and are mirrored by negative scales.
    /// The add and multiply blend modes blend the colour their way (the alpha
    /// behind is kept).
    fn specialize(
        _pipeline: &MaterialPipeline,
        descriptor: &mut RenderPipelineDescriptor,
        _layout: &MeshVertexBufferLayoutRef,
        key: MaterialPipelineKey<Self>,
    ) -> Result<(), SpecializedMeshPipelineError> {
        descriptor.primitive.cull_mode = None;
        let color = match key.bind_group_data.blend {
            Blend::Normal => return Ok(()),
            // dst + src (premultiplied: its colour times its alpha).
            Blend::Add => BlendComponent {
                src_factor: BlendFactor::One,
                dst_factor: BlendFactor::One,
                operation: BlendOperation::Add,
            },
            // dst * src (the shader gives the factor).
            Blend::Multiply => BlendComponent {
                src_factor: BlendFactor::Dst,
                dst_factor: BlendFactor::Zero,
                operation: BlendOperation::Add,
            },
        };
        let keep = BlendComponent {
            src_factor: BlendFactor::Zero,
            dst_factor: BlendFactor::One,
            operation: BlendOperation::Add,
        };
        if let Some(fragment) = descriptor.fragment.as_mut() {
            for target in fragment.targets.iter_mut().flatten() {
                target.blend = Some(BlendState { color, alpha: keep });
            }
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Entities
// ---------------------------------------------------------------------------

/// The 3D menu layer's camera.
#[derive(Component)]
pub struct FlashCamera;

/// An element's quad.
#[derive(Component)]
struct FlashElem;

/// A line of text in the atlas.
#[derive(Component)]
struct AtlasText;

/// A place in the atlas (texels).
#[derive(Clone, Copy, Debug, PartialEq)]
struct Slot {
    x: u32,
    y: u32,
    w: u32,
    h: u32,
}

/// Rows of the atlas, filled left to right.
#[derive(Default)]
struct Shelves {
    rows: Vec<(u32, u32, u32)>, // (y, height, next x)
    bottom: u32,
}

impl Shelves {
    fn alloc(&mut self, w: u32, h: u32) -> Option<Slot> {
        if w > ATLAS_W || h > ATLAS_H {
            return None;
        }
        for row in self.rows.iter_mut() {
            let (y, rh, x) = *row;
            if h <= rh && h * 4 >= rh * 3 && x + w <= ATLAS_W {
                row.2 = x + w;
                return Some(Slot { x, y, w, h });
            }
        }
        if self.bottom + h > ATLAS_H {
            return None;
        }
        let y = self.bottom;
        self.bottom += h;
        self.rows.push((y, h, w));
        Some(Slot { x: 0, y, w, h })
    }
}

/// What a material was last set to (to leave unchanged ones alone).
#[derive(Clone, PartialEq)]
struct MatState {
    mul: Vec4,
    add: Vec4,
    params: Vec4,
    uv_rect: Vec4,
    glow: Vec4,
    texture: Option<AssetId<Image>>,
    order: f32,
    blend: Blend,
}

struct Elem {
    entity: Entity,
    material: Handle<CxMaterial>,
    mat: MatState,
    /// Its own mesh (scale-9, or with a slant baked in) and the geometry it was
    /// built for.
    own: Option<(Handle<Mesh>, Vec<f32>)>,
    /// For text: its atlas line, the node drawing it and what it shows.
    text: Option<(Slot, Entity, TextSpec, f32)>,
    used: bool,
}

#[derive(Resource)]
struct FlashState {
    root: Entity,
    atlas_camera: Entity,
    atlas_root: Entity,
    atlas: Handle<Image>,
    quad: Handle<Mesh>,
    fonts: HashMap<FlashFont, Handle<Font>>,
    elems: HashMap<u64, Elem>,
    shelves: Shelves,
    /// The text scale the atlas was laid out for.
    atlas_scale: f32,
    /// A line found no room: the atlas starts over (not before `atlas_reset_at`
    /// plus ATLAS_RESET_SECS, so a frame with more text than fits can't make it
    /// start over every frame).
    atlas_full: bool,
    atlas_reset_at: f32,
}

fn setup_flash(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    mut images: ResMut<Assets<Image>>,
    mut meshes: ResMut<Assets<Mesh>>,
) {
    commands.spawn((
        FlashCamera,
        Camera3d::default(),
        Camera {
            // Over the game, the garage and the menu, without clearing them.
            order: 11,
            clear_color: ClearColorConfig::None,
            ..default()
        },
        Projection::from(PerspectiveProjection {
            fov: 0.57,
            near: 10.0,
            far: 100_000.0,
            ..default()
        }),
        Tonemapping::None,
        // HDR like the cameras it draws over.
        Hdr,
        Transform::from_xyz(STAGE_W * 0.5, -STAGE_H * 0.5, focal_length(1280.0)),
        RenderLayers::layer(FLASH_LAYER),
    ));
    // The root turns Flash's coordinates (y down, z into the screen) into Bevy's.
    let root = commands
        .spawn((
            Transform::from_rotation(Quat::from_rotation_x(std::f32::consts::PI)),
            Visibility::Visible,
            RenderLayers::layer(FLASH_LAYER),
        ))
        .id();

    // The text atlas: Bevy's UI draws each line into it.
    let mut atlas = Image::new_fill(
        Extent3d {
            width: ATLAS_W,
            height: ATLAS_H,
            ..default()
        },
        TextureDimension::D2,
        &[0, 0, 0, 0],
        TextureFormat::Bgra8UnormSrgb,
        RenderAssetUsages::default(),
    );
    atlas.texture_descriptor.usage =
        TextureUsages::TEXTURE_BINDING | TextureUsages::COPY_DST | TextureUsages::RENDER_ATTACHMENT;
    let atlas = images.add(atlas);
    let atlas_camera = commands
        .spawn((
            Camera2d,
            Camera {
                // Before the cameras that show it.
                order: -1,
                clear_color: ClearColorConfig::Custom(Color::NONE),
                ..default()
            },
            RenderTarget::Image(atlas.clone().into()),
            Tonemapping::None,
            Msaa::Off,
            // Nothing of the world: only the atlas's UI.
            RenderLayers::layer(FLASH_LAYER + 1),
        ))
        .id();
    let atlas_root = commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                width: Val::Px(ATLAS_W as f32),
                height: Val::Px(ATLAS_H as f32),
                ..default()
            },
            UiTargetCamera(atlas_camera),
        ))
        .id();

    let fonts = [
        FlashFont::Normal,
        FlashFont::Title,
        FlashFont::Header,
        FlashFont::NumberWide,
    ]
    .into_iter()
    .map(|f| (f, asset_server.load::<Font>(f.file())))
    .collect();
    let quad = meshes.add(mesh_from(quad_geometry()));
    commands.insert_resource(FlashState {
        root,
        atlas_camera,
        atlas_root,
        atlas,
        quad,
        fonts,
        elems: HashMap::new(),
        shelves: Shelves::default(),
        atlas_scale: 0.0,
        atlas_full: false,
        atlas_reset_at: f32::MIN,
    });
}

fn mesh_from((positions, uvs, indices): Geometry) -> Mesh {
    let normals = vec![[0.0, 0.0, 1.0]; positions.len()];
    Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::default(),
    )
    .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, positions)
    .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, normals)
    .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, uvs)
    .with_inserted_indices(Indices::U32(indices))
}

/// A mesh's positions, uvs and triangles.
type Geometry = (Vec<[f32; 3]>, Vec<[f32; 2]>, Vec<u32>);

/// The unit quad (0, 0) to (1, 1).
fn quad_geometry() -> Geometry {
    (
        vec![
            [0.0, 0.0, 0.0],
            [1.0, 0.0, 0.0],
            [1.0, 1.0, 0.0],
            [0.0, 1.0, 0.0],
        ],
        vec![[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]],
        vec![0, 1, 2, 0, 2, 3],
    )
}

/// A scale-9 quad `w` x `h` (from 0, 0): borders (left, top, right, bottom) in
/// those units and as parts of the texture.
fn nine_geometry(w: f32, h: f32, b: [f32; 4], t: [f32; 4]) -> Geometry {
    // Borders that don't fit share what there is.
    let kx = if b[0] + b[2] > w {
        w / (b[0] + b[2])
    } else {
        1.0
    };
    let ky = if b[1] + b[3] > h {
        h / (b[1] + b[3])
    } else {
        1.0
    };
    let xs = [0.0, b[0] * kx, w - b[2] * kx, w];
    let ys = [0.0, b[1] * ky, h - b[3] * ky, h];
    let us = [0.0, t[0], 1.0 - t[2], 1.0];
    let vs = [0.0, t[1], 1.0 - t[3], 1.0];
    let mut positions = Vec::with_capacity(16);
    let mut uvs = Vec::with_capacity(16);
    for j in 0..4 {
        for i in 0..4 {
            positions.push([xs[i], ys[j], 0.0]);
            uvs.push([us[i], vs[j]]);
        }
    }
    let mut indices = Vec::with_capacity(54);
    for j in 0..3u32 {
        for i in 0..3u32 {
            let a = j * 4 + i;
            indices.extend([a, a + 1, a + 5, a, a + 5, a + 4]);
        }
    }
    (positions, uvs, indices)
}

/// Whether a Bevy Transform (translation, rotation, scale) can hold `m`: its axes
/// at right angles to each other, none of them collapsed.
fn is_trs(m: &M3) -> bool {
    let col = |j: usize| [m.m[0][j], m.m[1][j], m.m[2][j]];
    let c = [col(0), col(1), col(2)];
    let len = c.map(|v| dot(v, v).sqrt());
    if len.iter().any(|l| *l < 1e-6) {
        return false;
    }
    [(0, 1), (0, 2), (1, 2)]
        .iter()
        .all(|&(i, j)| dot(c[i], c[j]).abs() <= 1e-4 * len[i] * len[j])
}

fn to_transform(m: &M3) -> Transform {
    Transform::from_matrix(Mat4::from_cols_array(&m.cols()))
}

/// Turns this frame's draw commands into entities.
#[allow(clippy::too_many_arguments)]
fn apply_flash(
    mut commands: Commands,
    time: Res<Time>,
    mut frame: ResMut<FlashFrame>,
    state: Option<ResMut<FlashState>>,
    view: Res<FlashView>,
    mut materials: ResMut<Assets<CxMaterial>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut elems: Query<(&mut Transform, &mut Visibility, &mut Mesh3d), With<FlashElem>>,
    mut texts: Query<
        (
            &mut Node,
            &mut Text,
            &mut TextFont,
            &mut TextColor,
            &mut TextLayout,
            &mut TextShadow,
            &mut Visibility,
        ),
        (With<AtlasText>, Without<FlashElem>),
    >,
    mut camera: Query<
        (&mut Camera, &mut Transform, &mut Projection),
        (With<FlashCamera>, Without<FlashElem>),
    >,
    mut cameras: Query<&mut Camera, Without<FlashCamera>>,
) {
    let Some(mut state) = state else {
        return;
    };
    let state = &mut *state;

    // The cameras only work while something is drawn.
    let active = !frame.cmds.is_empty();
    if let Ok(mut c) = cameras.get_mut(state.atlas_camera)
        && c.is_active != active
    {
        c.is_active = active;
    }

    // The camera, where Scaleform's eye is for this window.
    if let Ok((mut c, mut t, mut projection)) = camera.single_mut() {
        if c.is_active != active {
            c.is_active = active;
        }
        let eye = Vec3::new(STAGE_W * 0.5, -STAGE_H * 0.5, view.focal);
        if t.translation != eye {
            t.translation = eye;
        }
        let fov = 2.0 * (view.visible_height() * 0.5 / view.focal).atan();
        if let Projection::Perspective(p) = &mut *projection
            && (p.fov - fov).abs() > 1e-6
        {
            p.fov = fov;
        }
    }

    // Text is drawn into the atlas at the window's pixel size; a new size starts it
    // over, as does running out of room (lines that grew leave their old places).
    let now = time.elapsed_secs();
    let text_scale = (view.scale * view.dpi).min(MAX_TEXT_SCALE);
    let full_reset = state.atlas_full && now - state.atlas_reset_at >= ATLAS_RESET_SECS;
    if (state.atlas_scale - text_scale).abs() > 1e-3 || full_reset {
        state.atlas_scale = text_scale;
        state.atlas_full = false;
        state.atlas_reset_at = now;
        state.shelves = Shelves::default();
        for elem in state.elems.values_mut() {
            if let Some((_, node, _, _)) = elem.text.take() {
                commands.entity(node).despawn();
            }
        }
    }

    for elem in state.elems.values_mut() {
        elem.used = false;
    }
    let cmds = std::mem::take(&mut frame.cmds);
    for (i, cmd) in cmds.into_iter().enumerate() {
        let order = (i as f32 + 1.0) * ORDER_STEP;
        let mut mat = MatState {
            mul: Vec4::from_array(cmd.cx.mul),
            add: Vec4::from_array(cmd.cx.add),
            params: Vec4::ZERO,
            uv_rect: Vec4::new(0.0, 0.0, 1.0, 1.0),
            glow: Vec4::ZERO,
            texture: None,
            order,
            blend: cmd.blend,
        };
        if cmd.blend == Blend::Multiply {
            mat.params.x = 2.0;
        }
        let [x0, y0, x1, y1] = cmd.rect;
        let texture: Option<Handle<Image>>;
        // The quad's place in the element's space, and its own mesh (scale-9).
        let mut local = M3::translate(x0, y0, 0.0).then(&M3::scale(x1 - x0, y1 - y0));
        let mut nine = None;
        let mut text = None;
        match &cmd.draw {
            Draw::Image { image, nine: n } => {
                texture = image.clone();
                if let Some((b, t)) = n {
                    local = M3::translate(x0, y0, 0.0);
                    nine = Some([
                        x1 - x0,
                        y1 - y0,
                        b[0],
                        b[1],
                        b[2],
                        b[3],
                        t[0],
                        t[1],
                        t[2],
                        t[3],
                    ]);
                }
            }
            Draw::Text(spec) => {
                let zoom = text_scale * spec.zoom.max(0.25);
                let pad = text_pad(spec, zoom);
                let w = (spec.width * zoom).ceil().max(1.0) as u32 + 2 * pad;
                let h = (spec.size * 1.45 * zoom).ceil().max(1.0) as u32 + 2 * pad;
                text = Some((spec.clone(), zoom, w, h));
                texture = Some(state.atlas.clone());
                mat.params.y = 1.0;
                if let Some(g) = spec.glow {
                    mat.params.z = g.blur * 0.5 * zoom;
                    mat.params.w = g.strength;
                    mat.glow = Vec4::new(
                        g.color[0] as f32 / 255.0,
                        g.color[1] as f32 / 255.0,
                        g.color[2] as f32 / 255.0,
                        1.0,
                    );
                }
            }
        }
        mat.texture = texture.as_ref().map(|t| t.id());

        // The element's entity (made the first time its key is drawn).
        let is_new = !state.elems.contains_key(&cmd.key);
        if is_new {
            let material = materials.add(CxMaterial {
                mul: mat.mul,
                add: mat.add,
                params: mat.params,
                uv_rect: mat.uv_rect,
                texture: texture.clone(),
                glow: mat.glow,
                order,
                blend: mat.blend,
            });
            let entity = commands
                .spawn((
                    FlashElem,
                    Mesh3d(state.quad.clone()),
                    MeshMaterial3d(material.clone()),
                    Transform::default(),
                    Visibility::Hidden,
                    RenderLayers::layer(FLASH_LAYER),
                    ChildOf(state.root),
                ))
                .id();
            state.elems.insert(
                cmd.key,
                Elem {
                    entity,
                    material,
                    mat: mat.clone(),
                    own: None,
                    text: None,
                    used: false,
                },
            );
        }
        let Some(elem) = state.elems.get_mut(&cmd.key) else {
            continue;
        };
        elem.used = true;

        // Text: its line in the atlas.
        if let Some((spec, zoom, w, h)) = text {
            let fits = elem
                .text
                .as_ref()
                .is_some_and(|(slot, _, _, _)| slot.w >= w && slot.h >= h);
            if !fits {
                if let Some((_, node, _, _)) = elem.text.take() {
                    commands.entity(node).despawn();
                }
                let Some(slot) = state.shelves.alloc(w, h) else {
                    // No room: not shown this frame (hidden below); the atlas
                    // starts over soon.
                    warn_once!("The menu's text atlas is full: starting it over");
                    state.atlas_full = true;
                    elem.used = false;
                    continue;
                };
                let node = commands
                    .spawn((
                        AtlasText,
                        text_node(&slot, text_pad(&spec, zoom), (w, h)),
                        Text::new(spec.text.clone()),
                        TextFont {
                            font: state.fonts[&spec.font].clone(),
                            font_size: spec.size * zoom,
                            ..default()
                        },
                        TextColor(Color::srgb_u8(spec.color[0], spec.color[1], spec.color[2])),
                        text_layout(spec.align),
                        text_shadow(&spec, zoom),
                        Visibility::Inherited,
                        ChildOf(state.atlas_root),
                    ))
                    .id();
                elem.text = Some((slot, node, spec.clone(), zoom));
            } else if let Some((slot, node, old, old_zoom)) = elem.text.as_mut()
                && (*old != spec || *old_zoom != zoom)
                && let Ok((mut n, mut t, mut f, mut c, mut l, mut s, _)) = texts.get_mut(*node)
            {
                *n = text_node(slot, text_pad(&spec, zoom), (w, h));
                if t.0 != spec.text {
                    t.0 = spec.text.clone();
                }
                f.font = state.fonts[&spec.font].clone();
                f.font_size = spec.size * zoom;
                c.0 = Color::srgb_u8(spec.color[0], spec.color[1], spec.color[2]);
                *l = text_layout(spec.align);
                *s = text_shadow(&spec, zoom);
                *old = spec.clone();
                *old_zoom = zoom;
            }
            if let Some((slot, node, spec, zoom)) = &elem.text {
                if let Ok((.., mut v)) = texts.get_mut(*node) {
                    v.set_if_neq(Visibility::Inherited);
                }
                let pad = text_pad(spec, *zoom) as f32 / zoom;
                let (sw, sh) = (slot.w as f32 / zoom, slot.h as f32 / zoom);
                local = M3::translate(x0 - pad, y0 - pad, 0.0).then(&M3::scale(sw, sh));
                let (aw, ah) = (ATLAS_W as f32, ATLAS_H as f32);
                mat.uv_rect = Vec4::new(
                    slot.x as f32 / aw,
                    slot.y as f32 / ah,
                    slot.w as f32 / aw,
                    slot.h as f32 / ah,
                );
            }
        }

        // A Transform can't slant: a matrix with a slant (the stat bars) is baked
        // into the element's own mesh, which is then only moved. Scale-9 meshes
        // are built for their size.
        let full = cmd.matrix.then(&local);
        let slanted = !is_trs(&full);
        let geometry: Option<Vec<f32>> = (nine.is_some() || slanted).then(|| {
            let mut g: Vec<f32> = nine.map(|n| n.to_vec()).unwrap_or_default();
            if slanted {
                g.extend(full.m.iter().flat_map(|row| row[..3].iter().copied()));
            }
            g
        });
        let mesh = match geometry {
            Some(g) => {
                let rebuild = elem.own.as_ref().is_none_or(|(_, old)| {
                    old.len() != g.len() || old.iter().zip(&g).any(|(a, b)| (a - b).abs() > 0.01)
                });
                if rebuild {
                    let (mut positions, uvs, indices) = match nine {
                        Some([w, h, b0, b1, b2, b3, t0, t1, t2, t3]) => {
                            nine_geometry(w, h, [b0, b1, b2, b3], [t0, t1, t2, t3])
                        }
                        None => quad_geometry(),
                    };
                    if slanted {
                        for p in positions.iter_mut() {
                            *p = full.vector(*p);
                        }
                    }
                    let built = mesh_from((positions, uvs, indices));
                    let handle = match elem.own.take() {
                        Some((handle, _)) => {
                            if let Some(m) = meshes.get_mut(&handle) {
                                *m = built;
                            }
                            handle
                        }
                        None => meshes.add(built),
                    };
                    elem.own = Some((handle, g));
                }
                elem.own.as_ref().map(|(h, _)| h.clone())
            }
            None => Some(state.quad.clone()),
        };

        let transform = if slanted {
            Transform::from_xyz(full.m[0][3], full.m[1][3], full.m[2][3])
        } else {
            to_transform(&full)
        };
        if is_new {
            let mut e = commands.entity(elem.entity);
            e.insert((transform, Visibility::Inherited));
            if let Some(mesh) = mesh {
                e.insert(Mesh3d(mesh));
            }
        } else if let Ok((mut t, mut v, mut m)) = elems.get_mut(elem.entity) {
            if *t != transform {
                *t = transform;
            }
            v.set_if_neq(Visibility::Inherited);
            if let Some(mesh) = mesh
                && m.0 != mesh
            {
                m.0 = mesh;
            }
        }
        if elem.mat != mat {
            if let Some(m) = materials.get_mut(&elem.material) {
                m.mul = mat.mul;
                m.add = mat.add;
                m.params = mat.params;
                m.uv_rect = mat.uv_rect;
                m.glow = mat.glow;
                m.texture = texture;
                m.order = mat.order;
                m.blend = mat.blend;
            }
            elem.mat = mat;
        }
    }

    // What wasn't drawn this frame is hidden.
    for elem in state.elems.values() {
        if elem.used {
            continue;
        }
        if let Ok((_, mut v, _)) = elems.get_mut(elem.entity) {
            v.set_if_neq(Visibility::Hidden);
        }
        if let Some((_, node, _, _)) = &elem.text
            && let Ok((.., mut v)) = texts.get_mut(*node)
        {
            v.set_if_neq(Visibility::Hidden);
        }
    }
}

/// The room left round a line of text in its atlas slot (texels): the usual
/// margin, and room for its glow.
fn text_pad(spec: &TextSpec, zoom: f32) -> u32 {
    ATLAS_PAD + spec.glow.map_or(0, |g| (g.blur * 0.5 * zoom).ceil() as u32)
}

/// A line's node in the atlas: at its slot's corner, the size of its field (`w` by
/// `h` with the margins, as the text branch measures it), which a slot kept from a
/// bigger field is bigger than, so that a centred or right-aligned line lines up
/// within its own field.
fn text_node(slot: &Slot, pad: u32, (w, h): (u32, u32)) -> Node {
    Node {
        position_type: PositionType::Absolute,
        left: Val::Px((slot.x + pad) as f32),
        top: Val::Px((slot.y + pad) as f32),
        width: Val::Px(w.min(slot.w).saturating_sub(2 * pad) as f32),
        height: Val::Px(h.min(slot.h).saturating_sub(2 * pad) as f32),
        overflow: Overflow::clip(),
        ..default()
    }
}

/// How a field's line is laid out. Bevy lines up a line that may not wrap only
/// against its own width, which leaves every such line at its node's left edge: a
/// centred or right-aligned field wraps at its node's width instead, which lines
/// its line up across the field as Flash does (its line is short: nothing wraps).
fn text_layout(align: Align) -> TextLayout {
    TextLayout {
        justify: match align {
            Align::Left => Justify::Left,
            Align::Center => Justify::Center,
            Align::Right => Justify::Right,
        },
        linebreak: match align {
            Align::Left => LineBreak::NoWrap,
            Align::Center | Align::Right => LineBreak::WordBoundary,
        },
    }
}

fn text_shadow(spec: &TextSpec, zoom: f32) -> TextShadow {
    match spec.shadow {
        Some((offset, rgba)) => TextShadow {
            offset: Vec2::new(offset[0], offset[1]) * zoom,
            color: Color::srgba(rgba[0], rgba[1], rgba[2], rgba[3]),
        },
        None => TextShadow {
            offset: Vec2::ZERO,
            color: Color::NONE,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: f32, b: f32, eps: f32) -> bool {
        (a - b).abs() <= eps
    }

    #[test]
    fn turning_brings_the_right_edge_forward() {
        // +_yrotation: the clip's right edge comes toward the viewer (z falls).
        let m = Clip {
            yrot: 30.0,
            ..Clip::at(100.0, 50.0)
        }
        .matrix();
        let right = m.point([10.0, 0.0, 0.0]);
        assert!(right[2] < 0.0);
        assert!(close(
            right[0],
            100.0 + 10.0 * 30f32.to_radians().cos(),
            1e-4
        ));
        // _z is in twips.
        let deep = Clip {
            z: 400.0,
            ..Clip::at(0.0, 0.0)
        }
        .matrix();
        assert!(close(deep.point([0.0, 0.0, 0.0])[2], 20.0, 1e-6));
    }

    #[test]
    fn then_applies_the_child_first() {
        let parent = M3::translate(10.0, 0.0, 0.0).then(&M3::scale(2.0, 2.0));
        let child = M3::translate(5.0, 1.0, 0.0);
        let p = parent.then(&child).point([1.0, 1.0, 0.0]);
        assert_eq!(p, [10.0 + 2.0 * 6.0, 2.0 * 2.0, 0.0]);
    }

    #[test]
    fn focal_length_matches_scaleform() {
        // A 16:9 window shows 1280 x 720 stage pixels: f = 640 / tan(27.5 deg).
        assert!(close(focal_length(1280.0), 1229.43, 0.01));
        let view = FlashView::new(Vec2::new(1920.0, 1080.0), 1.0);
        assert!(close(view.scale, 1.5, 1e-6));
        assert!(close(view.focal, 1229.43, 0.01));
        // The stage on the window's flat plane projects to itself.
        let p = project([100.0, 700.0, 0.0], view.focal);
        assert!(close(p[0], 100.0, 1e-3) && close(p[1], 700.0, 1e-3));
    }

    #[test]
    fn unproject_inverts_project() {
        let f = focal_length(1280.0);
        let m = Clip {
            z: 1500.0,
            xrot: -25.0,
            yrot: 40.0,
            rot: 10.0,
            xscale: 0.7,
            yscale: 1.3,
            ..Clip::at(820.0, 230.0)
        }
        .matrix();
        for (a, b) in [(0.0, 0.0), (120.0, -40.0), (-75.5, 300.25), (275.0, 55.0)] {
            let screen = project(m.point([a, b, 0.0]), f);
            let back = unproject(screen, f, &m).expect("hits the plane");
            assert!(close(back[0], a, 1e-2), "{back:?} vs {a}");
            assert!(close(back[1], b, 1e-2), "{back:?} vs {b}");
        }
        // A clip seen exactly edge-on is never hit.
        let edge = Clip {
            yrot: 90.0,
            ..Clip::at(560.0, 360.0)
        }
        .matrix();
        let _ = unproject([560.0, 360.0], f, &edge);
    }

    #[test]
    fn eases_run_from_zero_to_one() {
        for e in [
            Ease::Linear,
            Ease::RegularIn,
            Ease::RegularOut,
            Ease::RegularInOut,
            Ease::StrongOut,
        ] {
            assert!(close(e.apply(0.0), 0.0, 1e-6));
            assert!(close(e.apply(1.0), 1.0, 1e-6));
            let mut last = 0.0;
            for i in 1..=100 {
                let v = e.apply(i as f32 / 100.0);
                assert!(v >= last - 1e-6, "{e:?} goes back at {i}");
                last = v;
            }
        }
        assert!(close(Ease::RegularInOut.apply(0.5), 0.5, 1e-6));
    }

    #[test]
    fn tweens_start_from_where_they_are() {
        let mut t = Tween::at(0.0);
        t.to(1.0, 10.0, 2.0, Ease::Linear);
        assert!(close(t.get(1.0), 0.0, 1e-6));
        assert!(close(t.get(2.0), 5.0, 1e-6));
        assert!(close(t.get(5.0), 10.0, 1e-6));
        // Turned round half way: from 5 back to 0.
        t.to(2.0, 0.0, 1.0, Ease::Linear);
        assert!(close(t.get(2.5), 2.5, 1e-6));
        assert_eq!(t.target(), 0.0);
    }

    #[test]
    fn keys_differ() {
        assert_ne!(key("slot", 1), key("slot", 2));
        assert_ne!(key("slot", 1), key("slots", 1));
        assert_eq!(key("tag", 3), key("tag", 3));
    }

    #[test]
    fn shelves_pack_and_fill() {
        let mut s = Shelves::default();
        let a = s.alloc(100, 20).unwrap();
        let b = s.alloc(50, 18).unwrap();
        assert_eq!((a.x, a.y), (0, 0));
        // Similar heights share a row.
        assert_eq!((b.x, b.y), (100, 0));
        let c = s.alloc(10, 40).unwrap();
        assert_eq!(c.y, 20);
        assert!(s.alloc(ATLAS_W + 1, 10).is_none());
        let mut n = 0;
        while s.alloc(ATLAS_W, 64).is_some() {
            n += 1;
        }
        assert_eq!(n, (ATLAS_H - 60) / 64);
    }

    #[test]
    fn slants_need_their_own_mesh() {
        assert!(is_trs(
            &Clip {
                yrot: 40.0,
                xscale: 0.7,
                yscale: 0.4,
                ..Clip::at(1.0, 2.0)
            }
            .matrix()
        ));
        // Mirrored is fine; slanted or collapsed isn't.
        assert!(is_trs(&M3::flat(-1.0, 0.0, 0.0, 1.0, 32.0, 0.0)));
        assert!(!is_trs(&M3::flat(1.0, 0.0, -0.599, 1.0, 0.0, 0.0)));
        assert!(!is_trs(&M3::scale(0.0, 1.0)));
        // A non-uniform scale outside a turn slants.
        assert!(!is_trs(&M3::scale(2.0, 1.0).then(&M3::rot_z(30.0))));
    }

    #[test]
    fn nine_slices_keep_their_borders() {
        let (p, uv, idx) =
            nine_geometry(275.0, 55.0, [42.0, 22.0, 58.0, 22.0], [0.2, 0.4, 0.25, 0.4]);
        assert_eq!(p.len(), 16);
        assert_eq!(uv.len(), 16);
        assert_eq!(idx.len(), 54);
        assert_eq!(p[1][0], 42.0);
        assert_eq!(p[2][0], 275.0 - 58.0);
        assert_eq!(uv[2][0], 0.75);
        // Too small: the borders share the space.
        let (p, _, _) = nine_geometry(50.0, 10.0, [42.0, 22.0, 58.0, 22.0], [0.2, 0.4, 0.25, 0.4]);
        assert!(p[1][0] <= p[2][0] + 1e-4);
        assert!(p[4][1] <= p[8][1] + 1e-4);
    }
}
