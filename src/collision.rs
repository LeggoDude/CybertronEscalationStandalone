//! Triangle-soup collision for the imported level (no physics engine needed).
//!
//! The maps' `assets/levels/<map>_col.bin` (made by `tools/build_map.py`) are
//! little-endian, with corners in metres in Bevy coordinates, packed small (a map's
//! million triangles would take 35 MB as floats): `"FOCC"`, a `u32` version (3), an
//! origin (3 `f32`) and a step (`f32`); then a `u32` count and 9 `i16`s per
//! triangle, each corner `origin + i * step`; then the same again for the triangles
//! that only stop movement (the map's blocking volumes: invisible walls that shots,
//! sight lines and the camera pass), and again for those that only stop the
//! players (its HmPlayerOnlyBlockingVolumes: the enemies' spawn walls, which their
//! AI walks through, IgnoreBlockingBy). (Version 2's files have no third set.)
//!
//! A triangle's corners go round so that its normal (`(b - a) x (c - a)`) faces out
//! of what it is part of, as the game draws it: the players' movement tells the
//! underside of a rock's overhang from a floor by it (`Tri::under`).
//!
//! Triangles are bucketed into a 2D grid over X/Z so that queries only look at the
//! few hundred triangles near the player.
//!
//! Some triangles come and go: the locked doors' (doors.rs), in groups that can be
//! switched on and off (a door closed, the same door open, a player blocker). Their
//! files, `assets/levels/<map>_doors.bin` (tools/build_doors.py), are packed the same
//! way: `"FOCD"`, a `u32` version (1), the origin and step, a `u32` group count, and
//! for each group a `u16` name length and its UTF-8 name, a `u8` (1: solid, 2: the
//! players' movement only, else movement only), and its triangles (a `u32` count, 9
//! `i16`s each).
//!
//! The module only uses `std` (plain `[f32; 3]` vectors) so it can be unit-tested
//! with `rustc --test src/collision.rs`, without building Bevy.

use std::io;
use std::path::Path;

pub type V3 = [f32; 3];

fn sub(a: V3, b: V3) -> V3 {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}
fn add(a: V3, b: V3) -> V3 {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}
fn mul(a: V3, s: f32) -> V3 {
    [a[0] * s, a[1] * s, a[2] * s]
}
fn dot(a: V3, b: V3) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}
fn cross(a: V3, b: V3) -> V3 {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}
fn len(a: V3) -> f32 {
    dot(a, a).sqrt()
}

/// What a triangle stops: everything (rays too), all movement, or only the players'
/// movement (the AI's pawns pass it).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Blocks {
    Solid,
    Movement,
    Players,
}

/// Who a movement query is for: a player (every wall stops him), an AI's pawn
/// (the players-only walls don't), or a Sniper flying between its points (the
/// level's solid surfaces alone: no blocking volume, ai/sniper.rs's FlyTo).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mover {
    Player,
    Ai,
    AiFlying,
}

#[derive(Clone, Copy)]
struct Tri {
    a: V3,
    b: V3,
    c: V3,
    /// Unit normal, by the order of its corners (facing out of what it is part of,
    /// for the map's own surfaces: tools/col_winding.py). Most queries only use the
    /// size of its y; `under` its sign.
    n: V3,
    /// Rays hit it (false for the walls that only stop movement).
    solid: bool,
    /// Only the players' movement stops at it.
    players_only: bool,
    /// Its group (0: always there; else one that can be switched off).
    group: u16,
    /// The underside of something of the map's (a rock's overhang, a beam's): one of
    /// its own solid surfaces facing down at least as steeply as a floor faces up, and
    /// one-sided (a surface the file has both ways round is a floor too). A player
    /// is never lifted onto one from below, and is stopped under it (ground_lift,
    /// push_out_for).
    under: bool,
}

impl Tri {
    /// The corners' lowest and highest coordinates.
    fn bounds(&self) -> (V3, V3) {
        let (a, b, c) = (self.a, self.b, self.c);
        (
            [
                a[0].min(b[0]).min(c[0]),
                a[1].min(b[1]).min(c[1]),
                a[2].min(b[2]).min(c[2]),
            ],
            [
                a[0].max(b[0]).max(c[0]),
                a[1].max(b[1]).max(c[1]),
                a[2].max(b[2]).max(c[2]),
            ],
        )
    }
}

/// Surfaces at least this flat count as floor; steeper ones are walls.
pub const WALKABLE_NY: f32 = 0.55;

/// The grid's cells are this wide (metres).
const CELL: f32 = 4.0;

fn invalid(what: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, format!("collision file {what}"))
}

fn u32_at(bytes: &[u8], at: usize) -> io::Result<u32> {
    bytes
        .get(at..at + 4)
        .map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
        .ok_or_else(|| invalid("truncated"))
}

fn f32_at(bytes: &[u8], at: usize) -> io::Result<f32> {
    u32_at(bytes, at).map(f32::from_bits)
}

/// A count at `*at` and that many triangles of 9 `i16`s after it (the compact
/// format's), moving `*at` past them.
fn compact_triangles(
    bytes: &[u8],
    at: &mut usize,
    origin: V3,
    step: f32,
) -> io::Result<Vec<[f32; 9]>> {
    let count = u32_at(bytes, *at)? as usize;
    *at += 4;
    let data = bytes
        .get(*at..*at + count * 18)
        .ok_or_else(|| invalid("truncated"))?;
    *at += count * 18;
    Ok(data
        .chunks_exact(18)
        .map(|t| {
            let mut f = [0f32; 9];
            for (k, v) in f.iter_mut().enumerate() {
                let i = i16::from_le_bytes([t[2 * k], t[2 * k + 1]]);
                *v = origin[k % 3] + i as f32 * step;
            }
            f
        })
        .collect())
}

/// Triangles (corners in a row) as Tris, stopping what `blocks` says, in a group
/// (degenerate ones left out).
fn add_tris(tris: &mut Vec<Tri>, flat: &[[f32; 9]], blocks: Blocks, group: u16) {
    for f in flat {
        let a = [f[0], f[1], f[2]];
        let b = [f[3], f[4], f[5]];
        let c = [f[6], f[7], f[8]];
        let n = cross(sub(b, a), sub(c, a));
        let l = len(n);
        if l < 1e-8 || !l.is_finite() {
            continue;
        }
        tris.push(Tri {
            a,
            b,
            c,
            n: mul(n, 1.0 / l),
            solid: blocks == Blocks::Solid,
            players_only: blocks == Blocks::Players,
            group,
            under: false,
        });
    }
}

/// Marks the map's own undersides (Tri::under): its solid surfaces facing down as
/// steeply as WALKABLE_NY, but not those it also has the other way round (the same
/// corners in the opposite order: a two-sided surface, which may be stood on).
fn mark_undersides(tris: &mut [Tri]) {
    // A triangle's corners in a fixed order (their bits), and which way round it is
    // from them (the parity of the order it has them in).
    let key = |t: &Tri| -> ([u32; 9], bool) {
        let mut c = [t.a, t.b, t.c].map(|p| p.map(f32::to_bits));
        let mut odd = false;
        for i in 0..3 {
            for j in 0..2 - i {
                if c[j] > c[j + 1] {
                    c.swap(j, j + 1);
                    odd = !odd;
                }
            }
        }
        let mut k = [0u32; 9];
        for (i, p) in c.iter().enumerate() {
            k[i * 3..i * 3 + 3].copy_from_slice(p);
        }
        (k, odd)
    };
    let own = |t: &Tri| t.solid && t.group == 0 && t.n[1].abs() >= WALKABLE_NY;
    let mut ways: std::collections::HashMap<[u32; 9], u8> = std::collections::HashMap::new();
    for t in tris.iter().filter(|t| own(t)) {
        let (k, odd) = key(t);
        *ways.entry(k).or_insert(0) |= if odd { 2 } else { 1 };
    }
    for t in tris.iter_mut() {
        t.under = own(t) && t.n[1] <= -WALKABLE_NY && ways.get(&key(t).0) != Some(&3);
    }
}

/// A switchable group of a doors file: its name, what it stops, its triangles.
pub struct Group {
    pub name: String,
    pub blocks: Blocks,
    pub tris: Vec<[f32; 9]>,
}

/// A doors file's groups (see the module docs).
pub fn load_groups(path: &Path) -> io::Result<Vec<Group>> {
    let bytes = std::fs::read(path)?;
    if !bytes.starts_with(b"FOCD") {
        return Err(invalid(
            "of an unknown format (rebuild it with tools/build_doors.py)",
        ));
    }
    if u32_at(&bytes, 4)? != 1 {
        return Err(invalid("of an unknown version"));
    }
    let origin = [f32_at(&bytes, 8)?, f32_at(&bytes, 12)?, f32_at(&bytes, 16)?];
    let step = f32_at(&bytes, 20)?;
    let count = u32_at(&bytes, 24)? as usize;
    let mut at = 28;
    let mut out = Vec::with_capacity(count);
    for _ in 0..count {
        let n = bytes
            .get(at..at + 2)
            .map(|b| u16::from_le_bytes([b[0], b[1]]) as usize)
            .ok_or_else(|| invalid("truncated"))?;
        at += 2;
        let name = bytes
            .get(at..at + n)
            .map(|b| String::from_utf8_lossy(b).into_owned())
            .ok_or_else(|| invalid("truncated"))?;
        at += n;
        let blocks = match *bytes.get(at).ok_or_else(|| invalid("truncated"))? {
            1 => Blocks::Solid,
            2 => Blocks::Players,
            _ => Blocks::Movement,
        };
        at += 1;
        let tris = compact_triangles(&bytes, &mut at, origin, step)?;
        out.push(Group { name, blocks, tris });
    }
    Ok(out)
}

pub struct LevelCollision {
    tris: Vec<Tri>,
    /// The switchable groups that are off (by group number; group 0 is never off).
    off: Vec<bool>,
    cell: f32,
    min_x: f32,
    min_z: f32,
    nx: usize,
    nz: usize,
    /// CSR layout: triangles of cell i are items[starts[i]..starts[i + 1]].
    starts: Vec<u32>,
    items: Vec<u32>,
}

impl LevelCollision {
    pub fn load(path: &Path) -> io::Result<Self> {
        Self::parse(&std::fs::read(path)?)
    }

    /// A collision file's contents (see the module docs).
    pub fn parse(bytes: &[u8]) -> io::Result<Self> {
        if !bytes.starts_with(b"FOCC") {
            return Err(invalid(
                "of an unknown format (rebuild it with tools/build_map.py)",
            ));
        }
        let version = u32_at(bytes, 4)?;
        if !(2..=3).contains(&version) {
            return Err(invalid("of an unknown version"));
        }
        let origin = [f32_at(bytes, 8)?, f32_at(bytes, 12)?, f32_at(bytes, 16)?];
        let step = f32_at(bytes, 20)?;
        let mut at = 24;
        let solid = compact_triangles(bytes, &mut at, origin, step)?;
        let movement_only = compact_triangles(bytes, &mut at, origin, step)?;
        let players_only = if version >= 3 {
            compact_triangles(bytes, &mut at, origin, step)?
        } else {
            Vec::new()
        };
        let mut c = Self::from_parts(&solid, &movement_only, CELL);
        if !players_only.is_empty() {
            let mut tris = c.tris;
            add_tris(&mut tris, &players_only, Blocks::Players, 0);
            c = Self::build(tris, c.off, CELL);
        }
        Ok(c)
    }

    /// Solid triangles, and ones that only stop movement (rays pass them).
    pub fn from_parts(solid: &[[f32; 9]], movement_only: &[[f32; 9]], cell: f32) -> Self {
        let mut tris = Vec::with_capacity(solid.len() + movement_only.len());
        add_tris(&mut tris, solid, Blocks::Solid, 0);
        add_tris(&mut tris, movement_only, Blocks::Movement, 0);
        Self::build(tris, vec![false], cell)
    }

    /// The grid of `tris`, `off` its groups' state.
    fn build(mut tris: Vec<Tri>, off: Vec<bool>, cell: f32) -> Self {
        mark_undersides(&mut tris);
        let (mut min_x, mut min_z, mut max_x, mut max_z) = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
        for t in &tris {
            for p in [t.a, t.b, t.c] {
                min_x = min_x.min(p[0]);
                max_x = max_x.max(p[0]);
                min_z = min_z.min(p[2]);
                max_z = max_z.max(p[2]);
            }
        }
        if tris.is_empty() {
            (min_x, min_z, max_x, max_z) = (0.0, 0.0, 1.0, 1.0);
        }
        let nx = (((max_x - min_x) / cell).floor() as usize + 1).max(1);
        let nz = (((max_z - min_z) / cell).floor() as usize + 1).max(1);
        let mut grid = LevelCollision {
            tris,
            off,
            cell,
            min_x,
            min_z,
            nx,
            nz,
            starts: Vec::new(),
            items: Vec::new(),
        };
        // Two passes: count per cell, then fill.
        let mut counts = vec![0u32; nx * nz + 1];
        for t in &grid.tris {
            let (x0, z0, x1, z1) = grid.tri_cells(t);
            for z in z0..=z1 {
                for x in x0..=x1 {
                    counts[z * nx + x] += 1;
                }
            }
        }
        let mut starts = vec![0u32; nx * nz + 1];
        for i in 0..nx * nz {
            starts[i + 1] = starts[i] + counts[i];
        }
        let mut fill = starts.clone();
        let mut items = vec![0u32; starts[nx * nz] as usize];
        for (i, t) in grid.tris.iter().enumerate() {
            let (x0, z0, x1, z1) = grid.tri_cells(t);
            for z in z0..=z1 {
                for x in x0..=x1 {
                    let c = z * nx + x;
                    items[fill[c] as usize] = i as u32;
                    fill[c] += 1;
                }
            }
        }
        grid.starts = starts;
        grid.items = items;
        grid
    }

    pub fn triangle_count(&self) -> usize {
        self.tris.len()
    }

    /// The same collision with more solid triangles (a map's props it hasn't the
    /// collision of: recreator.rs's pods on Oblivion).
    pub fn with_solids(self, extra: &[[f32; 9]]) -> Self {
        let mut tris = self.tris;
        add_tris(&mut tris, extra, Blocks::Solid, 0);
        Self::build(tris, self.off, self.cell)
    }

    /// The same collision with switchable groups of triangles (each its triangles
    /// and what they stop), all on: their group numbers.
    pub fn with_groups(self, groups: &[(Vec<[f32; 9]>, Blocks)]) -> (Self, Vec<u16>) {
        let mut tris = self.tris;
        let mut off = self.off;
        let mut ids = Vec::with_capacity(groups.len());
        for (g, blocks) in groups {
            let id = off.len().min(u16::MAX as usize) as u16;
            off.push(false);
            add_tris(&mut tris, g, *blocks, id);
            ids.push(id);
        }
        (Self::build(tris, off, self.cell), ids)
    }

    /// Whether a triangle stops `who` moving (its group on; the players-only walls
    /// not an AI's pawn, and only the solid ones a Sniper in flight).
    fn stops(&self, t: &Tri, who: Mover) -> bool {
        self.live(t)
            && match who {
                Mover::Player => true,
                Mover::Ai => !t.players_only,
                Mover::AiFlying => t.solid,
            }
    }

    /// Switches a group on or off (group 0, the map's own, stays on).
    pub fn set_group(&mut self, group: u16, on: bool) {
        if group > 0
            && let Some(o) = self.off.get_mut(group as usize)
        {
            *o = !on;
        }
    }

    /// Whether a group is on.
    pub fn group_on(&self, group: u16) -> bool {
        !self.off.get(group as usize).copied().unwrap_or(false)
    }

    /// Whether a triangle is there (its group on).
    fn live(&self, t: &Tri) -> bool {
        t.group == 0 || !self.off[t.group as usize]
    }

    fn cell_of(&self, x: f32, z: f32) -> (usize, usize) {
        let cx = ((x - self.min_x) / self.cell)
            .floor()
            .clamp(0.0, (self.nx - 1) as f32) as usize;
        let cz = ((z - self.min_z) / self.cell)
            .floor()
            .clamp(0.0, (self.nz - 1) as f32) as usize;
        (cx, cz)
    }

    fn tri_cells(&self, t: &Tri) -> (usize, usize, usize, usize) {
        let x_lo = t.a[0].min(t.b[0]).min(t.c[0]);
        let x_hi = t.a[0].max(t.b[0]).max(t.c[0]);
        let z_lo = t.a[2].min(t.b[2]).min(t.c[2]);
        let z_hi = t.a[2].max(t.b[2]).max(t.c[2]);
        let (x0, z0) = self.cell_of(x_lo, z_lo);
        let (x1, z1) = self.cell_of(x_hi, z_hi);
        (x0, z0, x1, z1)
    }

    fn inside(&self, x: f32, z: f32) -> bool {
        x >= self.min_x
            && z >= self.min_z
            && x < self.min_x + self.nx as f32 * self.cell
            && z < self.min_z + self.nz as f32 * self.cell
    }

    fn cell_tris(&self, cx: usize, cz: usize) -> &[u32] {
        let c = cz * self.nx + cx;
        &self.items[self.starts[c] as usize..self.starts[c + 1] as usize]
    }

    /// Height of the highest floor surface under (x, z) between `bottom` and `top`.
    pub fn ground(&self, x: f32, z: f32, top: f32, bottom: f32) -> Option<f32> {
        self.ground_for(x, z, top, bottom, Mover::Player)
    }

    /// The same for `who` (an AI's pawn stands on no players-only wall).
    pub fn ground_for(&self, x: f32, z: f32, top: f32, bottom: f32, who: Mover) -> Option<f32> {
        self.ground_lift(x, z, top, bottom, who, f32::INFINITY)
    }

    /// The same, an underside (Tri::under) counting only up to `lift_top`: a body
    /// whose feet were there is never lifted onto the underside of a rock's overhang
    /// it walks in under (and so into the rock), though it may come down onto one
    /// from above.
    pub fn ground_lift(
        &self,
        x: f32,
        z: f32,
        top: f32,
        bottom: f32,
        who: Mover,
        lift_top: f32,
    ) -> Option<f32> {
        if !self.inside(x, z) {
            return None;
        }
        let (cx, cz) = self.cell_of(x, z);
        let mut best: Option<f32> = None;
        for &i in self.cell_tris(cx, cz) {
            let t = &self.tris[i as usize];
            if !self.stops(t, who) {
                continue;
            }
            if t.n[1].abs() < WALKABLE_NY {
                continue;
            }
            // Barycentric coordinates of (x, z) in the triangle's X/Z projection.
            let (ax, az) = (t.a[0], t.a[2]);
            let (v0x, v0z) = (t.b[0] - ax, t.b[2] - az);
            let (v1x, v1z) = (t.c[0] - ax, t.c[2] - az);
            let (px, pz) = (x - ax, z - az);
            let d = v0x * v1z - v1x * v0z;
            if d.abs() < 1e-9 {
                continue;
            }
            let u = (px * v1z - v1x * pz) / d;
            let v = (v0x * pz - px * v0z) / d;
            const E: f32 = -1e-4;
            if u < E || v < E || u + v > 1.0 - E {
                continue;
            }
            let y = t.a[1] + u * (t.b[1] - t.a[1]) + v * (t.c[1] - t.a[1]);
            if t.under && y > lift_top {
                continue;
            }
            if y <= top && y >= bottom && best.is_none_or(|b| y > b) {
                best = Some(y);
            }
        }
        best
    }

    /// Height of the lowest flat surface over (x, z) between `bottom` and `top` that
    /// stops `who`: a ceiling, or the underside of something (the blocking volumes'
    /// too: a jet can't climb through the lids over a map; an AI's pawn passes the
    /// players-only ones).
    pub fn ceiling_for(&self, x: f32, z: f32, bottom: f32, top: f32, who: Mover) -> Option<f32> {
        if !self.inside(x, z) {
            return None;
        }
        let (cx, cz) = self.cell_of(x, z);
        let mut best: Option<f32> = None;
        for &i in self.cell_tris(cx, cz) {
            let t = &self.tris[i as usize];
            if !self.stops(t, who) {
                continue;
            }
            if t.n[1].abs() < WALKABLE_NY {
                continue;
            }
            let (ax, az) = (t.a[0], t.a[2]);
            let (v0x, v0z) = (t.b[0] - ax, t.b[2] - az);
            let (v1x, v1z) = (t.c[0] - ax, t.c[2] - az);
            let (px, pz) = (x - ax, z - az);
            let d = v0x * v1z - v1x * v0z;
            if d.abs() < 1e-9 {
                continue;
            }
            let u = (px * v1z - v1x * pz) / d;
            let v = (v0x * pz - px * v0z) / d;
            const E: f32 = -1e-4;
            if u < E || v < E || u + v > 1.0 - E {
                continue;
            }
            let y = t.a[1] + u * (t.b[1] - t.a[1]) + v * (t.c[1] - t.a[1]);
            if y <= top && y >= bottom && best.is_none_or(|b| y < b) {
                best = Some(y);
            }
        }
        best
    }

    /// Pushes a vertical capsule-ish body (spheres of `radius` stacked from
    /// `low` to `high`, centre heights) out of walls. Only moves it sideways.
    /// Contacts below `step_top` are ignored: those are ledges low enough to step
    /// onto (the ground query lifts the body onto them).
    /// Returns the horizontal correction (x, z), or None if nothing was touched.
    /// (A player's: the game's movement asks push_out_for.)
    #[cfg(test)]
    pub fn push_out(
        &self,
        pos: V3,
        radius: f32,
        low: f32,
        high: f32,
        step_top: f32,
    ) -> Option<[f32; 2]> {
        self.push_out_for(pos, radius, low, high, step_top, Mover::Player)
    }

    /// The same for `who` (an AI's pawn walks through the players-only walls). A
    /// player is pushed out from under the map's undersides too where they come
    /// down low over his feet (under_push: above `step_top`, as walls are).
    pub fn push_out_for(
        &self,
        pos: V3,
        radius: f32,
        low: f32,
        high: f32,
        step_top: f32,
        who: Mover,
    ) -> Option<[f32; 2]> {
        let mut p = pos;
        let mut touched = false;
        // (How high an underside stops him: UNDER_REACH, or his top less a step if
        // that is higher; no higher than his top: a car passes under what clears it.)
        let top = high + radius - pos[1];
        let reach = UNDER_REACH.min(top).max(top - UNDER_HEADROOM);
        for _ in 0..3 {
            let mut moved = false;
            let (x0, z0) = self.cell_of(p[0] - radius, p[2] - radius);
            let (x1, z1) = self.cell_of(p[0] + radius, p[2] + radius);
            let mut h = low;
            loop {
                for cz in z0..=z1 {
                    for cx in x0..=x1 {
                        for &i in self.cell_tris(cx, cz) {
                            let centre = [p[0], h, p[2]];
                            let t = &self.tris[i as usize];
                            // (An underside low over a player's feet stops him too.)
                            if t.under {
                                if who == Mover::Player
                                    && let Some(step) =
                                        under_push(t, centre, p, radius, step_top, reach)
                                {
                                    p = add(p, step);
                                    moved = true;
                                    touched = true;
                                }
                                continue;
                            }
                            if t.n[1].abs() >= WALKABLE_NY || !self.stops(t, who) {
                                continue;
                            }
                            // (A quick test first: wholly below the step, or with its
                            // box clear of the sphere's, it can't be touched.)
                            let (lo, hi) = t.bounds();
                            if hi[1] < step_top
                                || lo[1] > h + radius
                                || hi[1] < h - radius
                                || lo[0] > p[0] + radius
                                || hi[0] < p[0] - radius
                                || lo[2] > p[2] + radius
                                || hi[2] < p[2] - radius
                            {
                                continue;
                            }
                            let q = closest_point(centre, t);
                            if q[1] < step_top {
                                continue;
                            }
                            let d = sub(centre, q);
                            let dist2 = dot(d, d);
                            if dist2 >= radius * radius {
                                continue;
                            }
                            // Push sideways, away from the wall.
                            let mut dir = [d[0], 0.0, d[2]];
                            let mut l = len(dir);
                            if l < 1e-4 {
                                // Centre exactly on the wall: use the wall's normal,
                                // pointing back the way we came.
                                dir = [t.n[0], 0.0, t.n[2]];
                                l = len(dir);
                                if l < 1e-4 {
                                    continue;
                                }
                                if dot(dir, sub(pos, q)) < 0.0 {
                                    dir = mul(dir, -1.0);
                                }
                            }
                            let dir = mul(dir, 1.0 / l);
                            let flat = (d[0] * d[0] + d[2] * d[2]).sqrt();
                            let dy = d[1];
                            // Horizontal distance that clears the sphere at this height.
                            let need = (radius * radius - dy * dy).max(0.0).sqrt();
                            let push = need - flat + 0.001;
                            if push > 0.0 {
                                p = add(p, mul(dir, push));
                                moved = true;
                                touched = true;
                            }
                        }
                    }
                }
                if h >= high {
                    break;
                }
                h = (h + radius).min(high);
            }
            if !moved {
                break;
            }
        }
        touched.then(|| [p[0] - pos[0], p[2] - pos[2]])
    }

    /// Distance along `dir` (unit) from `origin` to the first triangle, up to `max`.
    #[cfg(test)]
    pub fn raycast(&self, origin: V3, dir: V3, max: f32) -> Option<f32> {
        self.raycast_hit(origin, dir, max).map(|(t, _)| t)
    }

    /// The same, with the surface's normal there (facing back against the ray).
    /// Rays pass the walls that only stop movement.
    pub fn raycast_hit(&self, origin: V3, dir: V3, max: f32) -> Option<(f32, V3)> {
        self.raycast_where(origin, dir, max, |t| t.solid && self.live(t))
    }

    /// The same for one of the rays of a trace with an extent (a box swept) for
    /// `who`: the walls that stop its movement stop it too (the blocking volumes
    /// block what has an extent, BlockNonZeroExtent).
    pub fn raycast_hit_for(&self, origin: V3, dir: V3, max: f32, who: Mover) -> Option<(f32, V3)> {
        self.raycast_where(origin, dir, max, |t| self.stops(t, who))
    }

    /// Whether the first wall stopping `who` along a line is one of the switchable
    /// groups' (a locked door's, a scaffold's: not the map's own), of those on now
    /// (`live_only`) or of any of them.
    pub fn group_first(&self, origin: V3, dir: V3, max: f32, who: Mover, live_only: bool) -> bool {
        let blocks = |t: &Tri| match who {
            Mover::Player => true,
            Mover::Ai => !t.players_only,
            Mover::AiFlying => t.solid,
        };
        let own = self.raycast_where(origin, dir, max, |t| t.group == 0 && blocks(t));
        let group = self.raycast_where(origin, dir, max, |t| {
            t.group != 0 && blocks(t) && (!live_only || self.live(t))
        });
        match (group, own) {
            (Some((g, _)), Some((o, _))) => g < o,
            (Some(_), None) => true,
            _ => false,
        }
    }

    fn raycast_where(
        &self,
        origin: V3,
        dir: V3,
        max: f32,
        hits: impl Fn(&Tri) -> bool,
    ) -> Option<(f32, V3)> {
        // Walk the grid cells under the ray's X/Z path (2D DDA).
        let mut best = max;
        let mut hit = false;
        let mut normal = [0.0, 1.0, 0.0];
        let (mut cx, mut cz) = self.cell_of(origin[0], origin[2]);
        let step_x: i64 = if dir[0] > 0.0 { 1 } else { -1 };
        let step_z: i64 = if dir[2] > 0.0 { 1 } else { -1 };
        let next_bound = |c: usize, step: i64, min: f32| {
            min + (c as f32 + if step > 0 { 1.0 } else { 0.0 }) * self.cell
        };
        let mut t_max_x = if dir[0].abs() < 1e-9 {
            f32::INFINITY
        } else {
            ((next_bound(cx, step_x, self.min_x) - origin[0]) / dir[0]).max(0.0)
        };
        let mut t_max_z = if dir[2].abs() < 1e-9 {
            f32::INFINITY
        } else {
            ((next_bound(cz, step_z, self.min_z) - origin[2]) / dir[2]).max(0.0)
        };
        let t_dx = if dir[0].abs() < 1e-9 {
            f32::INFINITY
        } else {
            self.cell / dir[0].abs()
        };
        let t_dz = if dir[2].abs() < 1e-9 {
            f32::INFINITY
        } else {
            self.cell / dir[2].abs()
        };
        let mut t_cell_start = 0.0f32;
        for _ in 0..(self.nx + self.nz + 2) {
            for &i in self.cell_tris(cx, cz) {
                let tri = &self.tris[i as usize];
                if !hits(tri) {
                    continue;
                }
                if let Some(t) = ray_tri(origin, dir, tri) {
                    if t < best {
                        best = t;
                        hit = true;
                        normal = if dot(tri.n, dir) > 0.0 {
                            [-tri.n[0], -tri.n[1], -tri.n[2]]
                        } else {
                            tri.n
                        };
                    }
                }
            }
            let t_exit = t_max_x.min(t_max_z);
            if (hit && best <= t_exit) || t_cell_start > best || t_exit > max {
                break;
            }
            t_cell_start = t_exit;
            if t_max_x < t_max_z {
                let n = cx as i64 + step_x;
                if n < 0 || n >= self.nx as i64 {
                    break;
                }
                cx = n as usize;
                t_max_x += t_dx;
            } else {
                let n = cz as i64 + step_z;
                if n < 0 || n >= self.nz as i64 {
                    break;
                }
                cz = n as usize;
                t_max_z += t_dz;
            }
        }
        hit.then_some((best, normal))
    }

    /// The corners of the triangles rays hit that reach into the box from `min` to
    /// `max` (what a decal there can lie on), each once, added to `out`.
    /// How far a point is from the map's own solid surfaces (not the switchable
    /// groups': the doors' and the scaffolds'), looked for out to `max` (`max` if
    /// none is nearer).
    pub fn own_clearance(&self, p: V3, max: f32) -> f32 {
        let (x0, z0) = self.cell_of(p[0] - max, p[2] - max);
        let (x1, z1) = self.cell_of(p[0] + max, p[2] + max);
        let mut best = max * max;
        for cz in z0..=z1 {
            for cx in x0..=x1 {
                for &i in self.cell_tris(cx, cz) {
                    let t = &self.tris[i as usize];
                    if t.group != 0 || !t.solid {
                        continue;
                    }
                    let (lo, hi) = t.bounds();
                    let r = best.sqrt();
                    if (0..3).any(|k| lo[k] > p[k] + r || hi[k] < p[k] - r) {
                        continue;
                    }
                    let d = sub(p, closest_point(p, t));
                    best = best.min(dot(d, d));
                }
            }
        }
        best.sqrt()
    }

    /// Whether a tube of `radius` round the line from `a` to `b` is clear of the
    /// map's own solid surfaces, as far as rays along it tell: its middle and two
    /// rings of eight round it, at half the radius and at it.
    pub fn own_tube_clear(&self, a: V3, b: V3, radius: f32) -> bool {
        let d = sub(b, a);
        let l = len(d);
        if l < 1e-3 {
            return true;
        }
        let dir = mul(d, 1.0 / l);
        let side = if dir[1].abs() < 0.9 {
            [0.0, 1.0, 0.0]
        } else {
            [1.0, 0.0, 0.0]
        };
        let u = cross(dir, side);
        let u = mul(u, 1.0 / len(u));
        let v = cross(dir, u);
        let own = |t: &Tri| t.group == 0 && t.solid;
        let clear = |o: V3| self.raycast_where(add(a, o), dir, l, own).is_none();
        clear([0.0; 3])
            && [0.5, 1.0].iter().all(|&ring| {
                (0..8).all(|k| {
                    let angle = (k as f32 + ring) * std::f32::consts::TAU / 8.0;
                    let r = radius * ring;
                    clear(add(mul(u, angle.cos() * r), mul(v, angle.sin() * r)))
                })
            })
    }

    pub fn solid_triangles_in(&self, min: V3, max: V3, out: &mut Vec<[V3; 3]>) {
        let (x0, z0) = self.cell_of(min[0], min[2]);
        let (x1, z1) = self.cell_of(max[0], max[2]);
        let mut found: Vec<u32> = Vec::new();
        for cz in z0..=z1 {
            for cx in x0..=x1 {
                for &i in self.cell_tris(cx, cz) {
                    let tri = &self.tris[i as usize];
                    if !tri.solid || !self.live(tri) {
                        continue;
                    }
                    let (lo, hi) = tri.bounds();
                    if (0..3).all(|k| lo[k] <= max[k] && hi[k] >= min[k]) {
                        found.push(i);
                    }
                }
            }
        }
        // (A triangle over several cells is listed in each.)
        found.sort_unstable();
        found.dedup();
        out.extend(found.iter().map(|&i| {
            let t = &self.tris[i as usize];
            [t.a, t.b, t.c]
        }));
    }
}

/// How high over a player's feet an underside stops him (push_out_for), at the
/// least: where a rock's overhang comes down lower than this over the floor, he is
/// pushed out from under it. A taller body reaches as high as its top less
/// UNDER_HEADROOM (a Titan's 4.3 m), so that his head (and the camera over his
/// shoulder) stays out of the rock; a lower one (a car) no higher than its top.
/// Higher parts he may walk under as before, his head in them (the low ceilings the
/// bigger classes brush).
pub const UNDER_REACH: f32 = 2.2;
const UNDER_HEADROOM: f32 = 1.1;

/// Undersides flatter than this (the horizontal part of their normal: a ceiling) are
/// left to the ceiling queries: pushing sideways can't clear them.
const UNDER_MIN_SLOPE: f32 = 0.25;

/// The sideways push that takes a player's sphere at `centre` (his feet at `feet`)
/// out from under underside `t`: when it touches the sphere higher than `step_top`
/// (lower down it is the undercut foot of something, a lump he steps onto or a wall
/// that stops him higher up, as a wall's foot is) and lower than `reach` over his
/// feet, his feet below it (not on top of what it is the underside of: a thin slab
/// he stands on).
fn under_push(t: &Tri, centre: V3, feet: V3, radius: f32, step_top: f32, reach: f32) -> Option<V3> {
    let slope = (t.n[0] * t.n[0] + t.n[2] * t.n[2]).sqrt();
    if slope < UNDER_MIN_SLOPE {
        return None;
    }
    let (lo, hi) = t.bounds();
    let (low, high) = (step_top.max(feet[1] + 0.05), feet[1] + reach);
    if lo[1] > high
        || hi[1] < low
        || lo[1] > centre[1] + radius
        || hi[1] < centre[1] - radius
        || lo[0] > centre[0] + radius
        || hi[0] < centre[0] - radius
        || lo[2] > centre[2] + radius
        || hi[2] < centre[2] - radius
    {
        return None;
    }
    let q = closest_point(centre, t);
    if q[1] <= low || q[1] >= high {
        return None;
    }
    let d = sub(centre, q);
    let dist = len(d);
    if dist >= radius || dot(sub([feet[0], low, feet[2]], t.a), t.n) <= 0.0 {
        return None;
    }
    let out = [t.n[0] / slope, 0.0, t.n[2] / slope];
    let flat = (d[0] * d[0] + d[2] * d[2]).sqrt();
    let across = dot(d, t.n);
    if across <= -0.99 * dist || flat <= 1e-3 {
        // The centre past its face (over it, in what it is the underside of): out
        // the way it faces, far enough for the sphere to clear it, a sphere's width
        // at most at a time.
        let need = if across > 0.0 {
            radius - dist
        } else {
            radius + dist
        };
        return Some(mul(out, (need / slope).min(radius) + 0.001));
    }
    // As from a wall: sideways, away from where it touches him (so that he slides
    // along it), far enough to clear it at this height.
    let push = (radius * radius - d[1] * d[1]).max(0.0).sqrt() - flat + 0.001;
    (push > 0.0).then(|| [d[0] / flat * push, 0.0, d[2] / flat * push])
}

/// Möller-Trumbore, both sides.
fn ray_tri(o: V3, d: V3, t: &Tri) -> Option<f32> {
    let e1 = sub(t.b, t.a);
    let e2 = sub(t.c, t.a);
    let p = cross(d, e2);
    let det = dot(e1, p);
    if det.abs() < 1e-9 {
        return None;
    }
    let inv = 1.0 / det;
    let s = sub(o, t.a);
    let u = dot(s, p) * inv;
    if !(0.0..=1.0).contains(&u) {
        return None;
    }
    let q = cross(s, e1);
    let v = dot(d, q) * inv;
    if v < 0.0 || u + v > 1.0 {
        return None;
    }
    let dist = dot(e2, q) * inv;
    (dist > 1e-4).then_some(dist)
}

/// Closest point on a triangle (Ericson, Real-Time Collision Detection 5.1.5).
fn closest_point(p: V3, t: &Tri) -> V3 {
    let (a, b, c) = (t.a, t.b, t.c);
    let ab = sub(b, a);
    let ac = sub(c, a);
    let ap = sub(p, a);
    let d1 = dot(ab, ap);
    let d2 = dot(ac, ap);
    if d1 <= 0.0 && d2 <= 0.0 {
        return a;
    }
    let bp = sub(p, b);
    let d3 = dot(ab, bp);
    let d4 = dot(ac, bp);
    if d3 >= 0.0 && d4 <= d3 {
        return b;
    }
    let vc = d1 * d4 - d3 * d2;
    if vc <= 0.0 && d1 >= 0.0 && d3 <= 0.0 {
        let v = d1 / (d1 - d3);
        return add(a, mul(ab, v));
    }
    let cp = sub(p, c);
    let d5 = dot(ab, cp);
    let d6 = dot(ac, cp);
    if d6 >= 0.0 && d5 <= d6 {
        return c;
    }
    let vb = d5 * d2 - d1 * d6;
    if vb <= 0.0 && d2 >= 0.0 && d6 <= 0.0 {
        let w = d2 / (d2 - d6);
        return add(a, mul(ac, w));
    }
    let va = d3 * d6 - d5 * d4;
    if va <= 0.0 && (d4 - d3) >= 0.0 && (d5 - d6) >= 0.0 {
        let w = (d4 - d3) / ((d4 - d3) + (d5 - d6));
        return add(b, mul(sub(c, b), w));
    }
    let denom = 1.0 / (va + vb + vc);
    let v = vb * denom;
    let w = vc * denom;
    add(a, add(mul(ab, v), mul(ac, w)))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A 40 x 40 floor at y = 0, a 10 m step up to y = 2 for x > 10, and a wall at z = 5.
    fn scene() -> LevelCollision {
        let mut t = vec![
            [-20.0, 0.0, -20.0, -20.0, 0.0, 20.0, 10.0, 0.0, 20.0],
            [-20.0, 0.0, -20.0, 10.0, 0.0, 20.0, 10.0, 0.0, -20.0],
            [10.0, 2.0, -20.0, 10.0, 2.0, 20.0, 20.0, 2.0, 20.0],
            [10.0, 2.0, -20.0, 20.0, 2.0, 20.0, 20.0, 2.0, -20.0],
        ];
        // wall: x from -5 to 5, y 0..8, at z = 5
        t.push([-5.0, 0.0, 5.0, 5.0, 0.0, 5.0, 5.0, 8.0, 5.0]);
        t.push([-5.0, 0.0, 5.0, 5.0, 8.0, 5.0, -5.0, 8.0, 5.0]);
        LevelCollision::from_parts(&t, &[], 4.0)
    }

    #[test]
    fn ground_heights() {
        let c = scene();
        assert_eq!(c.ground(0.0, 0.0, 1.0, -5.0), Some(0.0));
        assert_eq!(c.ground(15.0, 0.0, 3.0, -5.0), Some(2.0));
        // the step is above the search window
        assert_eq!(c.ground(15.0, 0.0, 1.0, -5.0), None);
        assert_eq!(c.ground(100.0, 0.0, 1.0, -5.0), None);
    }

    #[test]
    fn groups_come_and_go() {
        // A door (a wall at z = -5) and a raised floor, each a group of its own.
        let door = vec![
            [-5.0, 0.0, -5.0, 5.0, 0.0, -5.0, 5.0, 8.0, -5.0],
            [-5.0, 0.0, -5.0, 5.0, 8.0, -5.0, -5.0, 8.0, -5.0],
        ];
        let deck = vec![[-20.0, 3.0, -20.0, -20.0, 3.0, -12.0, -12.0, 3.0, -20.0]];
        let (mut c, ids) = scene().with_groups(&[(door, Blocks::Solid), (deck, Blocks::Movement)]);
        assert_eq!(ids.len(), 2);
        assert!(c.push_out([0.0, 0.0, -4.0], 1.4, 2.0, 5.0, 1.0).is_some());
        assert!(c.raycast([0.0, 1.0, 0.0], [0.0, 0.0, -1.0], 20.0).is_some());
        assert_eq!(c.ground(-18.0, -18.0, 4.0, -5.0), Some(3.0));
        c.set_group(ids[0], false);
        c.set_group(ids[1], false);
        assert!(!c.group_on(ids[0]));
        assert!(c.push_out([0.0, 0.0, -4.0], 1.4, 2.0, 5.0, 1.0).is_none());
        assert!(c.raycast([0.0, 1.0, 0.0], [0.0, 0.0, -1.0], 20.0).is_none());
        assert_eq!(c.ground(-18.0, -18.0, 4.0, -5.0), Some(0.0));
        // The map's own can't be switched off.
        c.set_group(0, false);
        assert_eq!(c.ground(0.0, 0.0, 1.0, -5.0), Some(0.0));
        c.set_group(ids[0], true);
        assert!(c.push_out([0.0, 0.0, -4.0], 1.4, 2.0, 5.0, 1.0).is_some());
    }

    /// A players-only wall (the enemies' spawn walls) stops the players' movement
    /// but not an AI's pawn's, nor rays; a version 3 file carries them.
    #[test]
    fn players_only_walls() {
        let wall = vec![
            [-5.0, 0.0, -5.0, 5.0, 0.0, -5.0, 5.0, 8.0, -5.0],
            [-5.0, 0.0, -5.0, 5.0, 8.0, -5.0, -5.0, 8.0, -5.0],
        ];
        // (A lid over the floor's corner, as the volumes' tops are.)
        let lid = vec![[-20.0, 3.0, -20.0, -20.0, 3.0, -16.0, -16.0, 3.0, -20.0]];
        let (c, _) =
            scene().with_groups(&[(wall.clone(), Blocks::Players), (lid, Blocks::Players)]);
        let at = [0.0, 0.0, -4.0];
        assert!(c.push_out(at, 1.4, 2.0, 5.0, 1.0).is_some());
        assert!(c.push_out_for(at, 1.4, 2.0, 5.0, 1.0, Mover::Ai).is_none());
        assert!(c.raycast([0.0, 1.0, 0.0], [0.0, 0.0, -1.0], 20.0).is_none());
        assert_eq!(c.ground(-18.0, -18.0, 4.0, -5.0), Some(3.0));
        assert_eq!(c.ground_for(-18.0, -18.0, 4.0, -5.0, Mover::Ai), Some(0.0));
        // The file: a version 3 with the wall in its third set.
        let q = |v: f32| ((v / 0.01).round() as i16).to_le_bytes();
        let mut bytes = b"FOCC".to_vec();
        bytes.extend(3u32.to_le_bytes());
        for f in [0.0f32, 0.0, 0.0, 0.01] {
            bytes.extend(f.to_le_bytes());
        }
        bytes.extend(0u32.to_le_bytes());
        bytes.extend(0u32.to_le_bytes());
        bytes.extend((wall.len() as u32).to_le_bytes());
        for t in &wall {
            for v in t {
                bytes.extend(q(*v));
            }
        }
        let c = LevelCollision::parse(&bytes).unwrap();
        assert_eq!(c.triangle_count(), 2);
        assert!(c.push_out(at, 1.4, 2.0, 5.0, 1.0).is_some());
        assert!(c.push_out_for(at, 1.4, 2.0, 5.0, 1.0, Mover::Ai).is_none());
    }

    /// A floor at y = 0, an overhang over it (its underside coming down from 3 m at
    /// x = 0 to 0.2 m at x = 4, facing down and towards -x), a thin ramp at x 10..14
    /// (its top rising from 0.5 m to 2.5 m, its underside 0.3 m under it) and a low
    /// lump at x -15.5..-13.
    fn overhang(two_sided: bool) -> LevelCollision {
        let mut t = vec![
            [-20.0, 0.0, -20.0, -20.0, 0.0, 20.0, 20.0, 0.0, 20.0],
            [-20.0, 0.0, -20.0, 20.0, 0.0, 20.0, 20.0, 0.0, -20.0],
            [0.0, 3.0, -5.0, 4.0, 0.2, -5.0, 4.0, 0.2, 5.0],
            [0.0, 3.0, -5.0, 4.0, 0.2, 5.0, 0.0, 3.0, 5.0],
            [10.0, 0.5, -5.0, 10.0, 0.5, 5.0, 14.0, 2.5, 5.0],
            [10.0, 0.5, -5.0, 14.0, 2.5, 5.0, 14.0, 2.5, -5.0],
            [10.0, 0.2, -5.0, 14.0, 2.2, 5.0, 10.0, 0.2, 5.0],
            [10.0, 0.2, -5.0, 14.0, 2.2, -5.0, 14.0, 2.2, 5.0],
        ];
        // A lump on the floor at x -15.5..-13, 0.6 m high, its side undercut (its
        // lip at x -15.5 over its foot at -15).
        t.extend([
            [-15.5, 0.6, -2.0, -15.5, 0.6, 2.0, -13.0, 0.6, 2.0],
            [-15.5, 0.6, -2.0, -13.0, 0.6, 2.0, -13.0, 0.6, -2.0],
            [-15.5, 0.6, -2.0, -15.0, 0.1, 2.0, -15.5, 0.6, 2.0],
            [-15.5, 0.6, -2.0, -15.0, 0.1, -2.0, -15.0, 0.1, 2.0],
        ]);
        if two_sided {
            t.push([0.0, 3.0, -5.0, 4.0, 0.2, 5.0, 4.0, 0.2, -5.0]);
            t.push([0.0, 3.0, -5.0, 0.0, 3.0, 5.0, 4.0, 0.2, 5.0]);
        }
        LevelCollision::from_parts(&t, &[], 4.0)
    }

    #[test]
    fn overhangs_stop_players() {
        let c = overhang(false);
        // Walking in under it, his feet on the floor: its underside 1.25 m up at
        // x = 2.5 is no floor to be lifted onto (it was), though one coming down onto
        // it from above lands on it.
        let near = |g: Option<f32>, y: f32| g.is_some_and(|g| (g - y).abs() < 1e-4);
        assert!(near(c.ground(2.5, 0.0, 1.3, -1.0), 1.25));
        assert!(near(
            c.ground_lift(2.5, 0.0, 1.3, -1.0, Mover::Player, 0.05),
            0.0
        ));
        assert!(near(
            c.ground_lift(2.5, 0.0, 1.3, -1.0, Mover::Player, 1.3),
            1.25
        ));
        // Where it comes down lower than UNDER_REACH over him, he is pushed back out
        // from under it (-x), as an AI's pawn is not.
        let p = c.push_out([1.5, 0.0, 0.0], 1.4, 1.4, 1.7, 1.1).unwrap();
        assert!(p[0] < -0.05 && p[1].abs() < p[0].abs() * 0.5, "{p:?}");
        assert!(
            c.push_out_for([1.5, 0.0, 0.0], 1.4, 1.4, 1.7, 1.1, Mover::Ai)
                .is_none()
        );
        // Under its high part (3 m up, x = -1.5: higher than UNDER_REACH where his
        // sphere meets it) he may stand.
        assert!(c.push_out([-1.5, 0.0, 0.0], 1.4, 1.4, 1.7, 1.1).is_none());
        // Walking up the thin ramp, its own underside under his feet neither pushes
        // him nor keeps him off its top.
        assert!(c.push_out([12.0, 1.5, 0.0], 1.4, 2.65, 2.95, 2.6).is_none());
        assert!(near(
            c.ground_lift(12.0, 0.0, 2.6, 0.4, Mover::Player, 1.55),
            1.5
        ));
        // A lump low enough to step onto doesn't stop him with its undercut side: he
        // steps up onto it.
        assert!(c.push_out([-16.6, 0.0, 0.0], 1.4, 1.4, 1.7, 1.1).is_none());
        assert!(near(
            c.ground_lift(-15.3, 0.0, 1.1, -1.0, Mover::Player, 0.05),
            0.6
        ));
        // A two-sided surface (the file has it both ways round) is a floor as before.
        let c = overhang(true);
        assert!(near(
            c.ground_lift(2.5, 0.0, 1.3, -1.0, Mover::Player, 0.05),
            1.25
        ));
        assert!(c.push_out([1.5, 0.0, 0.0], 1.4, 1.4, 1.7, 1.1).is_none());
    }

    #[test]
    fn walls_push_out() {
        let c = scene();
        // Body centred 1 m in front of the wall with radius 1.4: pushed to z = 5 - 1.4.
        let p = c.push_out([0.0, 0.0, 4.0], 1.4, 2.0, 5.0, 1.0).unwrap();
        assert!(p[0].abs() < 1e-3, "{p:?}");
        assert!((4.0 + p[1] - 3.6).abs() < 0.01, "{p:?}");
        // From the other side it is pushed the other way.
        let p = c.push_out([0.0, 0.0, 6.0], 1.4, 2.0, 5.0, 1.0).unwrap();
        assert!((6.0 + p[1] - 6.4).abs() < 0.01, "{p:?}");
        // Far away: nothing.
        assert!(c.push_out([0.0, 0.0, -5.0], 1.4, 2.0, 5.0, 1.0).is_none());
        // A wall lower than the step height doesn't block (you step onto it).
        assert!(c.push_out([0.0, 0.0, 4.0], 1.4, 1.4, 1.4, 9.0).is_none());
        // Floor triangles never push.
        assert!(c.push_out([12.0, 0.0, -10.0], 1.4, 2.5, 5.0, 1.0).is_none());
    }

    #[test]
    fn rays() {
        let c = scene();
        let t = c
            .raycast([0.0, 4.0, -10.0], [0.0, 0.0, 1.0], 100.0)
            .unwrap();
        assert!((t - 15.0).abs() < 1e-3);
        let t = c.raycast([0.0, 4.0, 0.0], [0.0, -1.0, 0.0], 100.0).unwrap();
        assert!((t - 4.0).abs() < 1e-3);
        assert!(
            c.raycast([0.0, 4.0, -10.0], [0.0, 0.0, 1.0], 10.0)
                .is_none()
        );
        // Diagonal ray across several cells.
        let d = [0.6f32, 0.0, 0.8];
        let t = c.raycast([-4.0, 4.0, -7.0], d, 100.0).unwrap();
        assert!((t - 15.0).abs() < 1e-3, "{t}");
        // A sight line through the wall is blocked; one short of it isn't.
        assert!(c.raycast([0.0, 4.0, 0.0], [0.0, 0.0, 1.0], 10.0).is_some());
        assert!(c.raycast([0.0, 4.0, 0.0], [0.0, 0.0, 1.0], 4.0).is_none());
    }

    #[test]
    fn clearance_from_the_maps_own_surfaces() {
        let mut c = scene();
        // (The wall at z = 5: 2 m from a point at z = 3, the floor 1 m under it.)
        assert!((c.own_clearance([0.0, 2.0, 3.0], 10.0) - 2.0).abs() < 1e-4);
        assert!((c.own_clearance([0.0, 1.0, 0.0], 10.0) - 1.0).abs() < 1e-4);
        assert_eq!(c.own_clearance([0.0, 6.0, -10.0], 3.0), 3.0);
        // (A door's group doesn't count.)
        let door = vec![[-1.0, 0.0, -11.0, 1.0, 0.0, -11.0, 0.0, 9.0, -11.0]];
        let (with, _) = c.with_groups(&[(door, Blocks::Solid)]);
        c = with;
        assert_eq!(c.own_clearance([0.0, 6.0, -10.0], 3.0), 3.0);
        // A tube through the wall's plane 1 m past its end: clear for half a metre
        // round it, not for 2 m.
        assert!(c.own_tube_clear([6.0, 4.0, -5.0], [6.0, 4.0, 15.0], 0.5));
        assert!(!c.own_tube_clear([6.0, 4.0, -5.0], [6.0, 4.0, 15.0], 2.0));
    }

    #[test]
    fn triangles_in_a_box() {
        let c = scene();
        let mut found = Vec::new();
        // Round the foot of the wall: its two triangles and the low floor's two
        // (each once, though they cross several cells), not the step's.
        c.solid_triangles_in([-1.0, -0.5, 4.0], [1.0, 1.0, 6.0], &mut found);
        assert_eq!(found.len(), 4, "{found:?}");
        // Up the wall, clear of the floor: the wall's.
        found.clear();
        c.solid_triangles_in([-1.0, 3.0, 4.5], [1.0, 4.0, 5.5], &mut found);
        assert_eq!(found.len(), 2, "{found:?}");
        assert!(found.iter().all(|t| t.iter().all(|p| p[2] == 5.0)));
        // In the open: nothing.
        found.clear();
        c.solid_triangles_in([-1.0, 3.0, -4.0], [1.0, 4.0, -3.0], &mut found);
        assert!(found.is_empty());
    }

    /// The scene in the compact format, with a wall that only stops movement at
    /// x = -10 (z -5..5, y 0..8).
    fn compact_scene() -> Vec<u8> {
        let solid = [
            [-20.0, 0.0, -20.0, -20.0, 0.0, 20.0, 10.0, 0.0, 20.0],
            [-20.0, 0.0, -20.0, 10.0, 0.0, 20.0, 10.0, 0.0, -20.0],
            [10.0, 2.0, -20.0, 10.0, 2.0, 20.0, 20.0, 2.0, 20.0],
            [10.0, 2.0, -20.0, 20.0, 2.0, 20.0, 20.0, 2.0, -20.0],
            [-5.0, 0.0, 5.0, 5.0, 0.0, 5.0, 5.0, 8.0, 5.0],
            [-5.0, 0.0, 5.0, 5.0, 8.0, 5.0, -5.0, 8.0, 5.0],
        ];
        let blocking = [
            [-10.0, 0.0, -5.0, -10.0, 0.0, 5.0, -10.0, 8.0, 5.0],
            [-10.0, 0.0, -5.0, -10.0, 8.0, 5.0, -10.0, 8.0, -5.0],
        ];
        let (origin, step) = ([0.0f32, 4.0, 0.0], 0.01f32);
        let mut out = b"FOCC".to_vec();
        out.extend(2u32.to_le_bytes());
        for v in origin {
            out.extend(v.to_le_bytes());
        }
        out.extend(step.to_le_bytes());
        for part in [&solid[..], &blocking[..]] {
            out.extend((part.len() as u32).to_le_bytes());
            for t in part {
                for (k, v) in t.iter().enumerate() {
                    let i = ((v - origin[k % 3]) / step).round() as i16;
                    out.extend(i.to_le_bytes());
                }
            }
        }
        out
    }

    #[test]
    fn compact_format() {
        let c = LevelCollision::parse(&compact_scene()).unwrap();
        assert_eq!(c.triangle_count(), 8);
        assert_eq!(c.ground(0.0, 0.0, 1.0, -5.0), Some(0.0));
        assert_eq!(c.ground(15.0, 0.0, 3.0, -5.0), Some(2.0));
        let t = c
            .raycast([0.0, 4.0, -10.0], [0.0, 0.0, 1.0], 100.0)
            .unwrap();
        assert!((t - 15.0).abs() < 1e-3, "{t}");
        // The blocking wall stops a body...
        let p = c.push_out([-9.0, 0.0, 0.0], 1.4, 2.0, 5.0, 1.0).unwrap();
        assert!((-9.0 + p[0] + 8.6).abs() < 0.01, "{p:?}");
        // ... but not rays.
        assert!(c.raycast([0.0, 4.0, 0.0], [-1.0, 0.0, 0.0], 50.0).is_none());
        // Truncated or of another version: an error, not a panic.
        let bytes = compact_scene();
        assert!(LevelCollision::parse(&bytes[..bytes.len() - 5]).is_err());
        let mut other = bytes.clone();
        other[4] = 3;
        assert!(LevelCollision::parse(&other).is_err());
        assert!(LevelCollision::parse(&[]).is_err());
    }

    #[test]
    fn ray_normals_face_back() {
        let c = scene();
        // The wall from in front and from behind, and the floor from above.
        let (_, n) = c
            .raycast_hit([0.0, 4.0, -10.0], [0.0, 0.0, 1.0], 100.0)
            .unwrap();
        assert!((n[2] + 1.0).abs() < 1e-4, "{n:?}");
        let (_, n) = c
            .raycast_hit([0.0, 4.0, 10.0], [0.0, 0.0, -1.0], 100.0)
            .unwrap();
        assert!((n[2] - 1.0).abs() < 1e-4, "{n:?}");
        let d = [0.0f32, -0.6, 0.8];
        let (_, n) = c.raycast_hit([0.0, 3.0, -10.0], d, 100.0).unwrap();
        assert!((n[1] - 1.0).abs() < 1e-4, "{n:?}");
    }

    #[test]
    fn real_level_files() {
        let assets = Path::new(option_env!("CARGO_MANIFEST_DIR").unwrap_or(".")).join("assets");
        if let Ok(c) = LevelCollision::load(&assets.join("levels/downfall_col.bin")) {
            assert!(c.triangle_count() > 100_000);
            // The floor under Downfall's player start (which stands 0.81 m above it).
            let g = c.ground(81.39, 3.6, 3.0, -5.0);
            assert!(g.is_some_and(|y| y.abs() < 0.1), "{g:?}");
            // The start room's roof, where enemies spawn (one at 86.72, 32.48,
            // 19.84), is closed to the players (build_map.py's blockers): a jet
            // climbing the room's light well (63, 20) meets the blocker at 30 m,
            // under the roof (30.7 m), and the room's ceiling is where it was. The
            // AI's pawns pass the blocker (the sky open over the well up to the
            // map's lid, 38.4 m) and stand on the roof.
            let well = c.ceiling_for(63.0, 20.0, 5.0, 40.0, Mover::Player);
            assert!(well.is_some_and(|y| (y - 30.0).abs() < 0.05), "{well:?}");
            let hall = c.ceiling_for(81.39, 3.6, 2.0, 40.0, Mover::Player);
            assert!(hall.is_some_and(|y| (y - 29.4).abs() < 0.2), "{hall:?}");
            let open = c.ceiling_for(63.0, 20.0, 5.0, 38.0, Mover::Ai);
            assert!(open.is_none(), "{open:?}");
            let roof = c.ground_for(86.72, 19.84, 33.0, 29.0, Mover::Ai);
            assert!(roof.is_some_and(|y| (y - 30.72).abs() < 0.1), "{roof:?}");
        }
        if let Ok(c) = LevelCollision::load(&assets.join("levels/oblivion_col.bin")) {
            assert!(c.triangle_count() > 100_000);
            // The floor under Oblivion's player start (which stands 0.76 m above it).
            let g = c.ground(-6.88, -105.15, 1.0, -3.0);
            assert!(g.is_some_and(|y| (y + 1.23).abs() < 0.1), "{g:?}");
        }
        if let Ok(c) = LevelCollision::load(&assets.join("levels/ancients_col.bin")) {
            assert!(c.triangle_count() > 100_000);
            // The floor under Ancients' player start (which stands 0.81 m above it).
            let g = c.ground(-2.56, 89.6, 12.0, 8.0);
            assert!(g.is_some_and(|y| (y - 10.24).abs() < 0.1), "{g:?}");
        }
    }
}
