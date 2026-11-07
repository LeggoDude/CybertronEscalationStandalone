//! The maps' navigation for the enemies' AI (levels/<map>_nav.txt, from
//! tools/build_nav.py): Fall of Cybertron's own navigation mesh, as its script
//! walks it.
//!
//! * **The mesh**: NavigationCells, each a triangle (its corners, the cell across
//!   each edge), joined to their neighbours and to the jump nodes (PathNodes) by
//!   their NavConnections ([`Link`]: walk, jump, high jump, fly, each with its cost
//!   and the biggest pawn that fits). The air units fly between AirPathNodes and the
//!   Snipers' perches (TnSniperPoints).
//! * **Paths** ([`NavMesh::find_channel`]): the game's FindChannelToPoint is native:
//!   an A* search through the points (each link costing its Distance) from the cell
//!   a pawn stands on to the goal's, round blocked points and links too narrow for
//!   it. Its channel is then turned into portals (PortalBuilder.BuildPortals) and the
//!   corners a robot walks to are pulled from them (StringPulling.Funnel and
//!   AdjustPathForRadius, the corner's steering point Radius x 1.5 out from it): both
//!   ported as the script has them, in the ground plane (UE's x and y are the
//!   demo's x and z).
//! * **Reach** ([`NavMesh::is_reachable_from_cell`]): AI.IsReachableFromCell, the
//!   border edges (no cell across them, or a blocked one) a pawn's circle would touch
//!   going straight from one point to another; AI.GetReachableDestination,
//!   AI.GetSafeDirectionBackOnToNavMesh and AI.FindCellsInRadius likewise.
//! * **Blocking**: a point's bBlocked keeps paths out of it. Each map's Kismet blocks
//!   the cells and jump nodes behind its locked doors when gameplay starts and opens
//!   them when a purchase's door has finished moving ([`NavMesh::toggles`]: doors.rs
//!   tells [`NavMesh::bought`] of each purchase).
//! * **Tactical nodes**: the positions the soldiers fight from (TnTacticalPathNode),
//!   the cells they stand on holding them: their cover (left or right corner, low or
//!   high, a ledge, or none), the directions their cover hides them from (a 32-bit
//!   mask round them) and their claim groups.

// The navigation's data and queries as the game has them, some for the enemy
// types still to come (the air units): the unused-code warnings are off.
#![allow(dead_code)]

use std::collections::{BinaryHeap, HashMap};
use std::path::Path;

use bevy::prelude::*;

/// Unreal units (centimetres) to the demo's metres.
pub const UU: f32 = 0.01;
/// StringPulling's area thresholds (PointLeftOfEdge, PointRightOfEdge: 1 uu^2).
const AREA_EPSILON: f32 = 1.0 * UU * UU;
/// Core.Math's epsilon in ClosestDistanceSquaredBetween2DLineSegments (1e-6 uu^2).
const SEGMENT_EPSILON: f32 = 1e-6 * UU * UU;
/// How far above and below a point the cell under it may be (GetBestAnchor's
/// search): a pawn's feet on a slope or a step.
const ANCHOR_UP: f32 = 1.0;
const ANCHOR_DOWN: f32 = 2.5;
/// The size of the grid cells the triangles are bucketed in (metres).
const GRID: f32 = 8.0;

// ---------------------------------------------------------------------------
// Core.Math in the ground plane (UE's x, y are the demo's x, z)
// ---------------------------------------------------------------------------

pub fn dot2d(a: Vec3, b: Vec3) -> f32 {
    a.x * b.x + a.z * b.z
}

pub fn size2d(a: Vec3) -> f32 {
    (a.x * a.x + a.z * a.z).sqrt()
}

pub fn size_sq2d(a: Vec3) -> f32 {
    a.x * a.x + a.z * a.z
}

/// Core.Math.Normal2D: the vector flattened and normalised (zero stays zero).
pub fn normal2d(a: Vec3) -> Vec3 {
    Vec3::new(a.x, 0.0, a.z).normalize_or_zero()
}

/// Math.ClosestDistanceSquaredBetween2DLineSegments (P1-Q1 and P2-Q2), as the script
/// has it (its C is a full 3-D dot product).
pub fn closest_dist_sq_2d_segments(p1: Vec3, q1: Vec3, p2: Vec3, q2: Vec3) -> f32 {
    let d1 = q1 - p1;
    let d2 = q2 - p2;
    let r = p1 - p2;
    let a = dot2d(d1, d1);
    let e = dot2d(d2, d2);
    let f = dot2d(d2, r);
    let (s, t);
    if a <= SEGMENT_EPSILON && e <= SEGMENT_EPSILON {
        return dot2d(r, r);
    }
    if a <= SEGMENT_EPSILON {
        s = 0.0;
        t = (f / e).clamp(0.0, 1.0);
    } else {
        let c = d1.dot(r);
        if e <= SEGMENT_EPSILON {
            t = 0.0;
            s = (-c / a).clamp(0.0, 1.0);
        } else {
            let b = dot2d(d1, d2);
            let denominator = a * e - b * b;
            let mut ss = if denominator != 0.0 {
                ((b * f - c * e) / denominator).clamp(0.0, 1.0)
            } else {
                0.0
            };
            let mut tt = (b * ss + f) / e;
            if tt < 0.0 {
                tt = 0.0;
                ss = (-c / a).clamp(0.0, 1.0);
            } else if tt > 1.0 {
                tt = 1.0;
                ss = ((b - c) / a).clamp(0.0, 1.0);
            }
            s = ss;
            t = tt;
        }
    }
    let q = (p1 + d1 * s) - (p2 + d2 * t);
    dot2d(q, q)
}

/// Math.IntersectCircles: where two circles cross (P0 round with P0Radius, P1 with
/// P1Radius), the one on the positive side of P0 -> P1's perpendicular (Y, -X) and
/// the other.
pub fn intersect_circles(p0: Vec3, r0: f32, p1: Vec3, r1: f32) -> Option<(Vec3, Vec3)> {
    let distance = (p1 - p0).length();
    if distance > r0 + r1 {
        return None;
    }
    if distance < (r1 - r0).abs() {
        return None;
    }
    if distance == 0.0 && r1 == r0 {
        return None;
    }
    let a = (distance * distance - r1 * r1 + r0 * r0) / (distance * 2.0);
    let h = (r0 * r0 - a * a).max(0.0).sqrt();
    let base = p0 + a * ((p1 - p0) / distance);
    let direction = p1 - p0;
    let perp = Vec3::new(direction.z, 0.0, -direction.x).normalize_or_zero();
    Some((base + h * perp, base - h * perp))
}

// ---------------------------------------------------------------------------
// The mesh
// ---------------------------------------------------------------------------

/// What a navigation point is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PointKind {
    Cell,
    /// A PathNode: where a jump starts or lands.
    Path,
    /// An AirPathNode.
    Air,
    /// A TnSniperPoint.
    Sniper,
    /// A TnTacticalPathNode.
    Tactical,
}

/// A navigation point.
#[derive(Clone, Debug)]
pub struct NavPoint {
    pub kind: PointKind,
    pub name: String,
    pub at: Vec3,
    pub yaw: f32,
    /// The cell it stands on (a cell's is itself), if any.
    pub anchor: Option<u32>,
    pub blocked: bool,
}

/// A NavigationCell's triangle: its corners (edge k runs from corner k to corner
/// k + 1) and the cell across each edge.
#[derive(Clone, Debug)]
pub struct Cell {
    pub corners: [Vec3; 3],
    pub neighbours: [Option<u32>; 3],
    /// Each edge's outward normal in the ground plane.
    pub normals: [Vec3; 3],
    pub normal: Vec3,
    pub tactical: bool,
    /// The tactical nodes on it (their points).
    pub holds: Vec<u32>,
}

/// One of a cell's edges (NavigationCell._Edges[k]).
#[derive(Clone, Copy, Debug)]
pub struct Edge {
    pub start: Vec3,
    pub end: Vec3,
    pub normal: Vec3,
    pub neighbour: Option<u32>,
}

/// A NavConnection (a NavSpec).
#[derive(Clone, Copy, Debug)]
pub struct Link {
    pub to: u32,
    pub cost: f32,
    pub end: Vec3,
    pub radius: f32,
    pub height: f32,
    pub walk: bool,
    pub jump: bool,
    pub fly: bool,
    pub high: bool,
    /// A flight closed for now: a locked door (or a scaffold still standing)
    /// stands across it ([`NavMesh::shut_door_links`]).
    pub shut: bool,
}

impl Link {
    /// NavigationPoint.IsJumpRequired: a jump it can't be walked.
    pub fn jump_required(&self) -> bool {
        self.jump && !self.walk
    }
}

/// A tactical node's cover (TacticalOcclusion.ETacticalCover, in its order).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Cover {
    /// Open: no cover (a TnTactical with none).
    #[default]
    None = 0,
    /// Low cover all along.
    Low = 1,
    LowRightCorner = 2,
    LowLeftCorner = 3,
    RightCorner = 4,
    LeftCorner = 5,
    Ledge = 6,
}

/// A tactical node's TnTactical (what is fixed of it: the AI's claims are its own).
#[derive(Clone, Copy, Debug)]
pub struct Tactical {
    pub point: u32,
    pub cover: Cover,
    pub occlusion: u32,
    pub claim_group: i32,
    /// Where it is, if not at its point: a sniper point's (TnSniperPoint._Tactical,
    /// on its wall: Initialize puts it 10 uu out from its middle).
    pub at: Option<Vec3>,
}

/// A Sniper's perch (TnSniperPoint): its navigation point (out from the wall), the
/// middle of its patch of wall, the patch's half sizes (along the wall's two axes),
/// the wall's normal (a unit vector) and whether its wall was found, how far out
/// from the wall a Sniper may begin its approach (_MaxApproachDistance), and its
/// actor's up axis (its Rotation's z: the wall's axes are its normal, up crossed
/// with the normal, and up).
#[derive(Clone, Copy, Debug)]
pub struct SniperPoint {
    pub point: u32,
    pub centre: Vec3,
    pub extent: Vec3,
    pub wall_normal: Vec3,
    pub wall_valid: bool,
    pub max_approach: f32,
    pub up: Vec3,
}

/// How far out from a sniper point's middle its tactical node is (uu).
const SNIPER_TACTICAL_OUT: f32 = 10.0;

/// A TnLeaperSmashVolume (Ignition's): the floor a Leaper leaps onto its enemy
/// across. Its brush's surface (the demo's metres), each triangle facing out of
/// it, and its bounds.
#[derive(Clone, Debug, Default)]
pub struct LeapVolume {
    pub name: String,
    pub tris: Vec<[Vec3; 3]>,
    pub min: Vec3,
    pub max: Vec3,
}

impl LeapVolume {
    /// Volume.EncompassesPoint: the point (the demo's metres) is inside the brush:
    /// its surface's winding number round the point (the solid angles of its
    /// triangles over 4 pi: 1 inside (-1 if they were wound the other way), 0
    /// outside; a concave brush is no trouble).
    pub fn encompasses(&self, point: Vec3) -> bool {
        if point.cmplt(self.min).any() || point.cmpgt(self.max).any() {
            return false;
        }
        let mut sum = 0.0f32;
        for [a, b, c] in &self.tris {
            let (a, b, c) = (*a - point, *b - point, *c - point);
            let (la, lb, lc) = (a.length(), b.length(), c.length());
            let num = a.dot(b.cross(c));
            let den = la * lb * lc + a.dot(b) * lc + a.dot(c) * lb + b.dot(c) * la;
            sum += 2.0 * num.atan2(den);
        }
        (sum / (4.0 * std::f32::consts::PI)).abs() > 0.5
    }
}

/// A Kismet toggle of points: when ("start" or a console's name), seconds after,
/// on (open), off (blocked) or toggle.
#[derive(Clone, Debug)]
pub struct Toggle {
    pub when: String,
    pub secs: f32,
    pub verb: Switch,
    pub points: Vec<u32>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Switch {
    On,
    Off,
    Flip,
}

/// What a pawn can do getting about (Pathing.MovementCapabilities).
#[derive(Clone, Copy, Debug)]
pub struct Moves {
    pub walk: bool,
    pub jump: bool,
    pub fly: bool,
    pub radius: f32,
    pub height: f32,
}

impl Moves {
    pub fn ground(radius: f32, height: f32) -> Moves {
        Moves {
            walk: true,
            jump: true,
            fly: false,
            radius,
            height,
        }
    }

    pub fn air(radius: f32, height: f32) -> Moves {
        Moves {
            walk: false,
            jump: false,
            fly: true,
            radius,
            height,
        }
    }

    /// NavigationPoint.SupportsMovementCapabilities: the link is one it can take,
    /// and it fits.
    pub fn supports(&self, link: &Link) -> bool {
        let able = (link.walk && self.walk) || (link.jump && self.jump) || (link.fly && self.fly);
        able && !link.shut && (link.radius <= 0.0 || link.radius >= self.radius)
    }
}

/// A map's navigation.
#[derive(Clone, Default)]
pub struct NavMesh {
    pub points: Vec<NavPoint>,
    /// The cells (the first points).
    pub cells: Vec<Cell>,
    pub links: Vec<Link>,
    /// Each point's links: links[first[p]..first[p + 1]].
    first: Vec<u32>,
    pub tacticals: Vec<Tactical>,
    /// Each point's tactical node (its index in tacticals), if it is one.
    tactical_of: HashMap<u32, u32>,
    pub snipers: Vec<SniperPoint>,
    pub toggles: Vec<Toggle>,
    /// Its Leaper smash volumes.
    pub leap_volumes: Vec<LeapVolume>,
    /// The cells over each grid square.
    grid: HashMap<(i32, i32), Vec<u32>>,
    by_name: HashMap<String, u32>,
    /// The flights (links[i], from its point) a door or scaffold of the level's
    /// can stand across, once looked for (shut_door_links).
    door_links: Option<Vec<(usize, u32)>>,
    /// The air network's flights measured against the map's own solid surfaces so
    /// far (measure_flights): how many links looked at, and those with less room
    /// round them than they say (each one's index and its room, metres).
    flight_room: (usize, Vec<(usize, f32)>),
}

/// The room looked for round each of the air network's flights (more than the
/// biggest flier's radius), how far apart along it it is looked for, and how near
/// a sniper point's wall it isn't (a Sniper leaves the line there for its approach,
/// out from the wall: ai/sniper.rs's FlyTo).
const FLIGHT_ROOM: f32 = 2.5;
const FLIGHT_STEP: f32 = 0.5;
pub const PERCH_ROOM: f32 = 8.0;

fn v3(s: &str) -> Vec3 {
    let v: Vec<f32> = s
        .split_whitespace()
        .filter_map(|x| x.parse().ok())
        .collect();
    if v.len() >= 3 {
        Vec3::new(v[0], v[1], v[2])
    } else {
        Vec3::ZERO
    }
}

fn idx(s: &str) -> Option<u32> {
    s.trim()
        .parse::<i64>()
        .ok()
        .filter(|i| *i >= 0)
        .map(|i| i as u32)
}

impl NavMesh {
    /// levels/<stem>_nav.txt under `dir` (none without it).
    pub fn load(dir: &Path, stem: &str) -> Option<NavMesh> {
        let text = std::fs::read_to_string(dir.join(format!("levels/{stem}_nav.txt"))).ok()?;
        Some(NavMesh::parse(&text))
    }

    pub fn parse(text: &str) -> NavMesh {
        let mut mesh = NavMesh::default();
        let mut raw_links: Vec<(u32, Link)> = Vec::new();
        let mut cells: HashMap<u32, Cell> = HashMap::new();
        for line in text.lines() {
            let r: Vec<&str> = line.split('\t').collect();
            match (r[0], r.len()) {
                ("point", 8..) => {
                    let kind = match r[2] {
                        "cell" => PointKind::Cell,
                        "path" => PointKind::Path,
                        "air" => PointKind::Air,
                        "sniper" => PointKind::Sniper,
                        _ => PointKind::Tactical,
                    };
                    let i = idx(r[1]).unwrap_or(0) as usize;
                    if mesh.points.len() <= i {
                        mesh.points.resize(
                            i + 1,
                            NavPoint {
                                kind: PointKind::Tactical,
                                name: String::new(),
                                at: Vec3::ZERO,
                                yaw: 0.0,
                                anchor: None,
                                blocked: false,
                            },
                        );
                    }
                    mesh.points[i] = NavPoint {
                        kind,
                        name: r[3].to_string(),
                        at: v3(r[4]),
                        yaw: r[5].trim().parse().unwrap_or(0.0),
                        anchor: idx(r[6]),
                        blocked: r[7].trim() == "1",
                    };
                    mesh.by_name.insert(r[3].to_string(), i as u32);
                }
                ("cell", 10..) => {
                    let Some(i) = idx(r[1]) else { continue };
                    let corners = [v3(r[2]), v3(r[3]), v3(r[4])];
                    let neighbours = [idx(r[5]), idx(r[6]), idx(r[7])];
                    let centre = (corners[0] + corners[1] + corners[2]) / 3.0;
                    let normals = std::array::from_fn(|k| {
                        let (a, b) = (corners[k], corners[(k + 1) % 3]);
                        let d = b - a;
                        let n = Vec3::new(d.z, 0.0, -d.x).normalize_or_zero();
                        if dot2d(n, centre - a) > 0.0 { -n } else { n }
                    });
                    cells.insert(
                        i,
                        Cell {
                            corners,
                            neighbours,
                            normals,
                            normal: v3(r[8]),
                            tactical: r[9].trim() == "1",
                            holds: Vec::new(),
                        },
                    );
                }
                ("link", 11..) => {
                    let (Some(from), Some(to)) = (idx(r[1]), idx(r[2])) else {
                        continue;
                    };
                    let n = |i: usize| r[i].trim().parse::<f32>().unwrap_or(0.0);
                    raw_links.push((
                        from,
                        Link {
                            to,
                            cost: n(3),
                            end: v3(r[4]),
                            radius: n(5),
                            height: n(6),
                            walk: r[7].trim() == "1",
                            jump: r[8].trim() == "1",
                            fly: r[9].trim() == "1",
                            high: r[10].trim() == "1",
                            shut: false,
                        },
                    ));
                }
                ("tac", 5..) => {
                    let Some(point) = idx(r[1]) else { continue };
                    let cover = match r[2] {
                        "low" => Cover::Low,
                        "left" => Cover::LeftCorner,
                        "right" => Cover::RightCorner,
                        "lowleft" => Cover::LowLeftCorner,
                        "lowright" => Cover::LowRightCorner,
                        "ledge" => Cover::Ledge,
                        _ => Cover::None,
                    };
                    mesh.tactical_of.insert(point, mesh.tacticals.len() as u32);
                    mesh.tacticals.push(Tactical {
                        point,
                        cover,
                        occlusion: r[3].trim().parse::<i64>().unwrap_or(0) as u32,
                        claim_group: r[4].trim().parse().unwrap_or(0),
                        at: None,
                    });
                }
                ("holds", 3..) => {
                    if let Some(c) = idx(r[1]).and_then(|c| cells.get_mut(&c)) {
                        c.holds.extend(r[2..].iter().filter_map(|x| idx(x)));
                    }
                }
                ("sniper", 6..) => {
                    let Some(point) = idx(r[1]) else { continue };
                    mesh.snipers.push(SniperPoint {
                        point,
                        centre: v3(r[2]),
                        extent: v3(r[3]),
                        // (Written a hundredth of a unit long, as the places are.)
                        wall_normal: v3(r[4]).normalize_or(Vec3::Y),
                        wall_valid: r[5].trim() == "1",
                        max_approach: r.get(6).and_then(|x| x.trim().parse().ok()).unwrap_or(10.0),
                        up: r.get(7).map_or(Vec3::Y, |x| v3(x).normalize_or(Vec3::Y)),
                    });
                }
                ("toggle", 5..) => {
                    let verb = match r[3] {
                        "on" => Switch::On,
                        "off" => Switch::Off,
                        _ => Switch::Flip,
                    };
                    mesh.toggles.push(Toggle {
                        when: r[1].to_string(),
                        secs: r[2].trim().parse().unwrap_or(0.0),
                        verb,
                        points: r[4..].iter().filter_map(|x| idx(x)).collect(),
                    });
                }
                ("leapvol", 4..) => {
                    let v = idx(r[1]).unwrap_or(0) as usize;
                    if mesh.leap_volumes.len() <= v {
                        mesh.leap_volumes.resize(v + 1, LeapVolume::default());
                    }
                    mesh.leap_volumes[v].name = r[2].to_string();
                }
                ("leaptri", 5..) => {
                    let v = idx(r[1]).unwrap_or(0) as usize;
                    if mesh.leap_volumes.len() <= v {
                        mesh.leap_volumes.resize(v + 1, LeapVolume::default());
                    }
                    let t = [v3(r[2]), v3(r[3]), v3(r[4])];
                    let vol = &mut mesh.leap_volumes[v];
                    let (lo, hi) = t.iter().fold(
                        (Vec3::splat(f32::MAX), Vec3::splat(f32::MIN)),
                        |(lo, hi), p| (lo.min(*p), hi.max(*p)),
                    );
                    if vol.tris.is_empty() {
                        (vol.min, vol.max) = (lo, hi);
                    } else {
                        vol.min = vol.min.min(lo);
                        vol.max = vol.max.max(hi);
                    }
                    vol.tris.push(t);
                }
                _ => {}
            }
        }
        // The cells, in their points' order (they come first).
        let n_cells = mesh
            .points
            .iter()
            .take_while(|p| p.kind == PointKind::Cell)
            .count();
        mesh.cells = (0..n_cells as u32)
            .map(|i| {
                cells.remove(&i).unwrap_or(Cell {
                    corners: [mesh.points[i as usize].at; 3],
                    neighbours: [None; 3],
                    normals: [Vec3::X; 3],
                    normal: Vec3::Y,
                    tactical: false,
                    holds: Vec::new(),
                })
            })
            .collect();
        // The sniper points' tactical nodes (on their walls: their sniper sensors'
        // collections), each by its point.
        for sp in &mesh.snipers {
            if mesh.tactical_of.contains_key(&sp.point) {
                continue;
            }
            mesh.tactical_of
                .insert(sp.point, mesh.tacticals.len() as u32);
            mesh.tacticals.push(Tactical {
                point: sp.point,
                cover: Cover::None,
                occlusion: 0,
                claim_group: 0,
                at: Some(sp.centre + sp.wall_normal * (SNIPER_TACTICAL_OUT / 100.0)),
            });
        }
        // Links by their start.
        raw_links.sort_by_key(|(from, _)| *from);
        mesh.first = vec![0; mesh.points.len() + 1];
        for (from, _) in &raw_links {
            if (*from as usize) < mesh.points.len() {
                mesh.first[*from as usize + 1] += 1;
            }
        }
        for i in 0..mesh.points.len() {
            mesh.first[i + 1] += mesh.first[i];
        }
        mesh.links = raw_links
            .into_iter()
            .filter(|(from, _)| (*from as usize) < mesh.points.len())
            .map(|(_, l)| l)
            .collect();
        // The grid.
        for (i, c) in mesh.cells.iter().enumerate() {
            let lo = c
                .corners
                .iter()
                .fold(Vec3::splat(f32::MAX), |a, b| a.min(*b));
            let hi = c
                .corners
                .iter()
                .fold(Vec3::splat(f32::MIN), |a, b| a.max(*b));
            for gx in (lo.x / GRID).floor() as i32..=(hi.x / GRID).floor() as i32 {
                for gz in (lo.z / GRID).floor() as i32..=(hi.z / GRID).floor() as i32 {
                    mesh.grid.entry((gx, gz)).or_default().push(i as u32);
                }
            }
        }
        // The toggles at gameplay start.
        let start: Vec<Toggle> = mesh
            .toggles
            .iter()
            .filter(|t| t.when == "start")
            .cloned()
            .collect();
        for t in &start {
            mesh.switch(t);
        }
        mesh
    }

    /// Applies a toggle.
    pub fn switch(&mut self, t: &Toggle) {
        for &p in &t.points {
            if let Some(pt) = self.points.get_mut(p as usize) {
                pt.blocked = match t.verb {
                    Switch::On => false,
                    Switch::Off => true,
                    Switch::Flip => !pt.blocked,
                };
            }
        }
    }

    /// The toggles a console's purchase makes, with their seconds.
    pub fn bought(&self, console: &str) -> Vec<Toggle> {
        self.toggles
            .iter()
            .filter(|t| t.when == console)
            .cloned()
            .collect()
    }

    pub fn point_named(&self, name: &str) -> Option<u32> {
        self.by_name.get(name).copied()
    }

    pub fn links_of(&self, p: u32) -> &[Link] {
        let p = p as usize;
        if p + 1 >= self.first.len() {
            return &[];
        }
        &self.links[self.first[p] as usize..self.first[p + 1] as usize]
    }

    /// The flights of the air network (the jets' and the Snipers' ways) a locked
    /// door, or a scaffold still standing, stands across now shut, and the rest
    /// open. The navigation's own blocking keeps paths out of the places behind
    /// the doors (each map's Kismet blocks their cells and jump nodes), but not its
    /// flights through them: there the air network's links cross the doors' and the
    /// scaffolds' collision, and a Sniper or a jet sent along one flew into it and
    /// hung there, its way checked, given up and taken again. The walls of the
    /// map's own (which the network's builders flew round) aren't counted.
    pub fn shut_door_links(&mut self, collision: &crate::collision::LevelCollision) {
        use crate::collision::Mover;
        let ray = |nav: &NavMesh, i: usize, from: u32, live_only: bool| {
            let a = nav.points[from as usize].at;
            let b = nav.points[nav.links[i].to as usize].at;
            let d = b - a;
            let len = d.length();
            len > 0.05
                && collision.group_first(
                    a.to_array(),
                    (d / len).to_array(),
                    len,
                    Mover::Ai,
                    live_only,
                )
        };
        if self.door_links.is_none() {
            let mut found = Vec::new();
            for from in 0..self.points.len() as u32 {
                let range = if (from as usize) + 1 < self.first.len() {
                    self.first[from as usize] as usize..self.first[from as usize + 1] as usize
                } else {
                    0..0
                };
                for i in range {
                    if self.links[i].fly && ray(self, i, from, false) {
                        found.push((i, from));
                    }
                }
            }
            self.door_links = Some(found);
        }
        let list = self.door_links.clone().unwrap_or_default();
        for (i, from) in list {
            self.links[i].shut = ray(self, i, from, true);
        }
    }

    /// Measures more of the air network's flights (for up to `budget`) against the map's
    /// own solid surfaces (not its doors' or scaffolds': shut_door_links), each one
    /// with less room round its line than the network gives it (out to FLIGHT_ROOM)
    /// narrowed to that, so that a flier too big for it doesn't take it
    /// (Moves::supports). The game's network was built against the game's
    /// collision; here a few of its flights run into the map's own surfaces (on
    /// Downfall the four to TnSniperPoint_2779 graze a pillar), and a Sniper sent
    /// along one hung against it, its way out of it taking it back in.
    pub fn measure_flights(
        &mut self,
        collision: &crate::collision::LevelCollision,
        budget: std::time::Duration,
    ) {
        let clock = std::time::Instant::now();
        let (start, mut narrow) = std::mem::take(&mut self.flight_room);
        let mut end = start;
        while end < self.links.len() && clock.elapsed() < budget {
            let i = end;
            end += 1;
            let l = self.links[i];
            if !l.fly || l.walk || l.jump {
                continue;
            }
            // (Its point: the last whose links start at or before it.)
            let from = self.first.partition_point(|&f| f as usize <= i) - 1;
            let (mut a, mut b) = (self.points[from].at, self.points[l.to as usize].at);
            let len = (b - a).length();
            let perch = |p: usize| self.points[p].kind == PointKind::Sniper;
            let (cut_a, cut_b) = (
                if perch(from) { PERCH_ROOM } else { 0.0 },
                if perch(l.to as usize) {
                    PERCH_ROOM
                } else {
                    0.0
                },
            );
            if len <= cut_a + cut_b + FLIGHT_STEP {
                continue;
            }
            let dir = (b - a) / len;
            (a, b) = (a + dir * cut_a, b - dir * cut_b);
            // (Most are clear: a few rays tell. The rest are measured along.)
            if collision.own_tube_clear(a.to_array(), b.to_array(), FLIGHT_ROOM) {
                continue;
            }
            let n = ((b - a).length() / FLIGHT_STEP).ceil().max(1.0) as usize;
            let mut room = FLIGHT_ROOM;
            for k in 0..=n {
                let at = a + (b - a) * (k as f32 / n as f32);
                room = collision.own_clearance(at.to_array(), room);
                if room <= 0.0 {
                    break;
                }
            }
            if room < FLIGHT_ROOM && (l.radius <= 0.0 || room < l.radius) {
                // (A radius of 0 means any: the least it can be is a little more.)
                let room = room.max(0.01);
                self.links[i].radius = room;
                narrow.push((i, room));
            }
        }
        self.flight_room = (end, narrow);
    }

    /// Whether every flight has been measured (measure_flights).
    pub fn flights_measured(&self) -> bool {
        self.flight_room.0 >= self.links.len()
    }

    /// NavigationPoint.GetNavSpecTo: the link from `a` to `b`.
    /// Pathing.BreadthSearchStep: the next point of a breadth first search over
    /// the points (`searched`, from `index` on), its neighbours the moves can
    /// reach put after the rest; None once every point reached has been taken.
    pub fn breadth_search_step(
        &self,
        searched: &mut Vec<u32>,
        index: &mut usize,
        moves: &Moves,
    ) -> Option<u32> {
        let p = *searched.get(*index)?;
        *index += 1;
        for l in self.links_of(p) {
            if moves.supports(l) && !searched.contains(&l.to) {
                searched.push(l.to);
            }
        }
        Some(p)
    }

    pub fn link_between(&self, a: u32, b: u32) -> Option<&Link> {
        self.links_of(a).iter().find(|l| l.to == b)
    }

    pub fn is_cell(&self, p: u32) -> bool {
        (p as usize) < self.cells.len()
    }

    pub fn cell(&self, p: u32) -> Option<&Cell> {
        self.cells.get(p as usize)
    }

    pub fn tactical(&self, p: u32) -> Option<&Tactical> {
        self.tactical_of
            .get(&p)
            .and_then(|&t| self.tacticals.get(t as usize))
    }

    /// The point's index among the tactical nodes, if it is one.
    pub fn tactical_index(&self, p: u32) -> Option<u32> {
        self.tactical_of.get(&p).copied()
    }

    /// Where a tactical node is (by its index): its point's place, or its own.
    pub fn tactical_at(&self, t: &Tactical) -> Vec3 {
        t.at.unwrap_or(self.points[t.point as usize].at)
    }

    pub fn edge(&self, cell: u32, k: usize) -> Edge {
        let c = &self.cells[cell as usize];
        Edge {
            start: c.corners[k],
            end: c.corners[(k + 1) % 3],
            normal: c.normals[k],
            neighbour: c.neighbours[k],
        }
    }

    fn blocked(&self, p: Option<u32>) -> bool {
        p.is_none_or(|p| self.points.get(p as usize).is_none_or(|pt| pt.blocked))
    }

    /// The height of a cell's plane at (x, z).
    pub fn height_at(&self, cell: u32, x: f32, z: f32) -> f32 {
        let c = &self.cells[cell as usize];
        let n = c.normal;
        let a = c.corners[0];
        if n.y.abs() < 1e-4 {
            return a.y;
        }
        a.y - (n.x * (x - a.x) + n.z * (z - a.z)) / n.y
    }

    /// Whether (x, z) is inside a cell's triangle, `slack` metres out from its
    /// edges allowed.
    pub fn inside2d(&self, cell: u32, p: Vec3, slack: f32) -> bool {
        let c = &self.cells[cell as usize];
        (0..3).all(|k| dot2d(c.normals[k], p - c.corners[k]) <= slack)
    }

    /// GetBestAnchor's cell: the cell under `feet` (in its triangle, its plane
    /// within reach of them), the nearest in height.
    pub fn cell_at(&self, feet: Vec3) -> Option<u32> {
        let key = (
            (feet.x / GRID).floor() as i32,
            (feet.z / GRID).floor() as i32,
        );
        let mut best: Option<(u32, f32)> = None;
        for &c in self.grid.get(&key).map(|v| v.as_slice()).unwrap_or(&[]) {
            if !self.inside2d(c, feet, 1e-3) {
                continue;
            }
            let h = self.height_at(c, feet.x, feet.z);
            let dy = feet.y - h;
            if dy > ANCHOR_DOWN || dy < -ANCHOR_UP {
                continue;
            }
            if best.is_none_or(|(_, b)| dy.abs() < b) {
                best = Some((c, dy.abs()));
            }
        }
        best.map(|(c, _)| c)
    }

    /// The cell nearest `feet` within `reach` metres (in the ground plane, its
    /// height within reach too), and the nearest point on it.
    pub fn nearest_cell(&self, feet: Vec3, reach: f32) -> Option<(u32, Vec3)> {
        if let Some(c) = self.cell_at(feet) {
            return Some((
                c,
                Vec3::new(feet.x, self.height_at(c, feet.x, feet.z), feet.z),
            ));
        }
        let r = (reach / GRID).ceil() as i32;
        let (gx, gz) = (
            (feet.x / GRID).floor() as i32,
            (feet.z / GRID).floor() as i32,
        );
        let mut best: Option<(u32, Vec3, f32)> = None;
        for x in gx - r..=gx + r {
            for z in gz - r..=gz + r {
                for &c in self.grid.get(&(x, z)).map(|v| v.as_slice()).unwrap_or(&[]) {
                    if self.points[c as usize].blocked {
                        continue;
                    }
                    let p = self.closest_point_in_cell(c, feet);
                    let d = p.distance(feet);
                    if d <= reach && best.is_none_or(|(_, _, b)| d < b) {
                        best = Some((c, p, d));
                    }
                }
            }
        }
        best.map(|(c, p, _)| (c, p))
    }

    /// The point of a cell's triangle nearest `p`.
    pub fn closest_point_in_cell(&self, cell: u32, p: Vec3) -> Vec3 {
        let c = &self.cells[cell as usize];
        let q = if self.inside2d(cell, p, 0.0) {
            Vec3::new(p.x, 0.0, p.z)
        } else {
            let mut best = (Vec3::ZERO, f32::MAX);
            for k in 0..3 {
                let (a, b) = (c.corners[k], c.corners[(k + 1) % 3]);
                let ab = Vec3::new(b.x - a.x, 0.0, b.z - a.z);
                let t = (dot2d(p - a, ab) / dot2d(ab, ab).max(1e-9)).clamp(0.0, 1.0);
                let q = Vec3::new(a.x + ab.x * t, 0.0, a.z + ab.z * t);
                let d = size_sq2d(p - q);
                if d < best.1 {
                    best = (q, d);
                }
            }
            best.0
        };
        Vec3::new(q.x, self.height_at(cell, q.x, q.z), q.z)
    }

    // -----------------------------------------------------------------------
    // Paths
    // -----------------------------------------------------------------------

    /// Pathing.FindChannel(ToPoint): the points from `start` (a cell) to `goal`'s
    /// cell, by A* through the links `moves` can take, round blocked points (the
    /// start's own cell excepted). None if there is no way.
    pub fn find_channel(&self, start: u32, goal: Vec3, moves: &Moves) -> Option<Vec<u32>> {
        let goal_cell = if moves.fly {
            self.nearest_point(goal, |p| {
                matches!(p.kind, PointKind::Air | PointKind::Sniper)
            })?
        } else {
            self.cell_at(goal)?
        };
        self.find_channel_to(start, goal_cell, goal, moves)
    }

    /// The nearest point of the kinds `want` takes to `at`.
    pub fn nearest_point(&self, at: Vec3, want: impl Fn(&NavPoint) -> bool) -> Option<u32> {
        self.points
            .iter()
            .enumerate()
            .filter(|(_, p)| !p.blocked && want(p))
            .min_by(|a, b| {
                a.1.at
                    .distance_squared(at)
                    .total_cmp(&b.1.at.distance_squared(at))
            })
            .map(|(i, _)| i as u32)
    }

    /// The A* itself: from point `start` to point `goal_point` (`goal` its spot).
    pub fn find_channel_to(
        &self,
        start: u32,
        goal_point: u32,
        goal: Vec3,
        moves: &Moves,
    ) -> Option<Vec<u32>> {
        self.find_channel_round(start, goal_point, goal, moves, |_| false)
    }

    /// Pathing.FindChannel between two points, round those `avoid` takes too (the
    /// sniper points other Snipers have claimed: their bBlocked).
    pub fn find_channel_avoiding(
        &self,
        start: u32,
        goal_point: u32,
        moves: &Moves,
        avoid: impl Fn(u32) -> bool,
    ) -> Option<Vec<u32>> {
        let goal = self.points.get(goal_point as usize)?.at;
        self.find_channel_round(start, goal_point, goal, moves, avoid)
    }

    fn find_channel_round(
        &self,
        start: u32,
        goal_point: u32,
        goal: Vec3,
        moves: &Moves,
        avoid: impl Fn(u32) -> bool,
    ) -> Option<Vec<u32>> {
        if start == goal_point {
            return Some(vec![start]);
        }
        if self
            .points
            .get(goal_point as usize)
            .is_none_or(|p| p.blocked)
        {
            return None;
        }
        #[derive(PartialEq)]
        struct Open(f32, u32);
        impl Eq for Open {}
        impl PartialOrd for Open {
            fn partial_cmp(&self, o: &Self) -> Option<std::cmp::Ordering> {
                Some(self.cmp(o))
            }
        }
        impl Ord for Open {
            fn cmp(&self, o: &Self) -> std::cmp::Ordering {
                o.0.total_cmp(&self.0).then(o.1.cmp(&self.1))
            }
        }
        let n = self.points.len();
        let mut cost = vec![f32::MAX; n];
        let mut from = vec![u32::MAX; n];
        let mut open = BinaryHeap::new();
        cost[start as usize] = 0.0;
        let h = |p: u32| self.points[p as usize].at.distance(goal);
        open.push(Open(h(start), start));
        while let Some(Open(_, p)) = open.pop() {
            if p == goal_point {
                let mut out = vec![p];
                let mut at = p;
                while at != start {
                    at = from[at as usize];
                    out.push(at);
                }
                out.reverse();
                return Some(out);
            }
            let c = cost[p as usize];
            for l in self.links_of(p) {
                if !moves.supports(l) || self.points[l.to as usize].blocked || avoid(l.to) {
                    continue;
                }
                let nc = c + l.cost.max(0.01);
                if nc < cost[l.to as usize] {
                    cost[l.to as usize] = nc;
                    from[l.to as usize] = p;
                    open.push(Open(nc + h(l.to), l.to));
                }
            }
        }
        None
    }

    /// PortalBuilder.GetPortalType: how the channel goes on from its point `i`.
    fn portal_type(&self, channel: &[u32], i: usize) -> PortalType {
        if i + 1 == channel.len() {
            return PortalType::Walk;
        }
        match self.link_between(channel[i], channel[i + 1]) {
            Some(l) if l.jump_required() => {
                if l.high {
                    PortalType::HighJump
                } else {
                    PortalType::Jump
                }
            }
            _ => PortalType::Walk,
        }
    }

    /// PortalBuilder.BuildPortals: the channel's portals, from `start` to `finish`.
    pub fn build_portals(&self, start: Vec3, finish: Vec3, channel: &[u32]) -> Vec<Portal> {
        let mut portals = vec![Portal::point(start, PortalType::Walk)];
        let mut previous_point = start;
        let mut previous_cell: Option<u32> = None;
        for (i, &p) in channel.iter().enumerate() {
            let current_cell = self.is_cell(p).then_some(p);
            let current_point = self.points[p as usize].at;
            match current_cell {
                Some(cell) => {
                    self.add_portal_to_cell(&mut portals, previous_cell, previous_point, cell)
                }
                None => self.add_portal_to_point(
                    &mut portals,
                    previous_cell,
                    current_point,
                    self.portal_type(channel, i),
                ),
            }
            previous_cell = current_cell;
            previous_point = current_point;
        }
        self.add_portal_to_point(&mut portals, previous_cell, finish, PortalType::Walk);
        portals
    }

    fn add_portal_to_cell(
        &self,
        portals: &mut Vec<Portal>,
        from_cell: Option<u32>,
        from: Vec3,
        to: u32,
    ) {
        match from_cell {
            Some(fc) => {
                // AddPortalCellToCell: the edge across to it, its end on the left.
                for k in 0..3 {
                    let e = self.edge(fc, k);
                    if e.neighbour == Some(to) {
                        portals.push(Portal::new(e.end, e.start, PortalType::Walk));
                        return;
                    }
                }
            }
            None => {
                // AddPortalPointToCell: the first edge of it the point is outside.
                for k in 0..3 {
                    let e = self.edge(to, k);
                    if point_outside_edge(from, &e) {
                        portals.push(Portal::new(e.start, e.end, PortalType::Walk));
                        return;
                    }
                }
            }
        }
    }

    fn add_portal_to_point(
        &self,
        portals: &mut Vec<Portal>,
        from_cell: Option<u32>,
        to: Vec3,
        kind: PortalType,
    ) {
        if let Some(fc) = from_cell {
            // AddPortalCellToPoint: the edge of the cell the point is outside.
            for k in 0..3 {
                let e = self.edge(fc, k);
                if point_outside_edge(to, &e) {
                    portals.push(Portal::new(e.end, e.start, PortalType::Walk));
                    break;
                }
            }
        }
        portals.push(Portal::point(to, kind));
    }

    // -----------------------------------------------------------------------
    // Reach
    // -----------------------------------------------------------------------

    /// AI.ProcessCell: the cell's edges not back into a closed cell, and the cell
    /// closed.
    fn process_cell(&self, closed: &mut Vec<u32>, edges: &mut Vec<Edge>, cell: Option<u32>) {
        let Some(cell) = cell else { return };
        for k in 0..3 {
            let e = self.edge(cell, k);
            if e.neighbour.is_some_and(|n| closed.contains(&n)) {
                continue;
            }
            edges.push(e);
        }
        closed.push(cell);
    }

    /// AI.IsReachableFromCell: whether a pawn of `radius` can go straight from
    /// `departure` to `destination` from `root` without touching a border edge (none
    /// across it, or a blocked cell); else the first such edge's normal and how far
    /// the pawn's circle overlaps it.
    pub fn is_reachable_from_cell(
        &self,
        root: u32,
        departure: Vec3,
        destination: Vec3,
        radius: f32,
    ) -> Result<(), (Vec3, f32)> {
        let mut closed = Vec::new();
        let mut edges = Vec::new();
        self.process_cell(&mut closed, &mut edges, Some(root));
        let radius_sq = radius * radius;
        let mut i = 0;
        while i < edges.len() {
            let e = edges[i];
            i += 1;
            let d = closest_dist_sq_2d_segments(departure, destination, e.start, e.end);
            if d > radius_sq {
                continue;
            }
            if e.neighbour.is_none() || self.blocked(e.neighbour) {
                return Err((e.normal, radius - d.sqrt()));
            }
            if edges.len() > 4096 {
                break;
            }
            self.process_cell(&mut closed, &mut edges, e.neighbour);
        }
        Ok(())
    }

    /// AI.GetReachableDestination: `destination`, or it moved back off the border
    /// edge in the way (by the overlap and 1 uu), if that is reachable.
    pub fn reachable_destination(
        &self,
        root: u32,
        from: Vec3,
        destination: Vec3,
        radius: f32,
    ) -> Option<Vec3> {
        match self.is_reachable_from_cell(root, from, destination, radius) {
            Ok(()) => Some(destination),
            Err((normal, overlap)) => {
                let d = destination - normal * (overlap + UU);
                self.is_reachable_from_cell(root, from, d, radius)
                    .ok()
                    .map(|_| d)
            }
        }
    }

    /// AI.GetTouchingBorderEdges and GetSafeDirectionBackOnToNavMesh: the way back
    /// from the border edges a circle at `centre` touches (zero for none).
    pub fn safe_direction_back(&self, root: u32, centre: Vec3, radius: f32) -> Vec3 {
        let mut closed = Vec::new();
        let mut edges = Vec::new();
        let mut touched: Vec<Vec3> = Vec::new();
        self.process_cell(&mut closed, &mut edges, Some(root));
        let radius_sq = radius * radius;
        let mut i = 0;
        while i < edges.len() && edges.len() < 4096 {
            let e = edges[i];
            i += 1;
            let d = closest_dist_sq_2d_segments(centre, centre, e.start, e.end);
            if d > radius_sq {
                continue;
            }
            if e.neighbour.is_none() || self.blocked(e.neighbour) {
                touched.push(e.normal);
            } else {
                self.process_cell(&mut closed, &mut edges, e.neighbour);
            }
        }
        if touched.is_empty() {
            return Vec3::ZERO;
        }
        let sum: Vec3 = touched.iter().map(|n| -*n).sum();
        (sum / touched.len() as f32).normalize_or_zero()
    }

    /// NavigationCell.WalkLine (native): whether a pawn of `radius` can walk the
    /// straight line from `start` to `destination` over the mesh, and where it would
    /// stop (the destination, or short of the border edge in the way).
    pub fn walk_line(
        &self,
        root: u32,
        start: Vec3,
        destination: Vec3,
        radius: f32,
    ) -> (bool, Vec3) {
        match self.is_reachable_from_cell(root, start, destination, radius) {
            Ok(()) => (true, destination),
            Err((normal, _)) => {
                // Back along the line from where it meets the edge's line.
                let d = destination - start;
                let along = dot2d(d, normal);
                let mut t = 1.0;
                if along > 1e-6 {
                    // The distance to the edge plane from the start, less the radius.
                    let mut best = 1.0f32;
                    for s in 0..=32 {
                        let tt = s as f32 / 32.0;
                        let p = start + d * tt;
                        if self.is_reachable_from_cell(root, start, p, radius).is_err() {
                            break;
                        }
                        best = tt;
                    }
                    t = best;
                }
                (false, start + d * t)
            }
        }
    }

    /// AI.FindCellsInRadius: the cells from `root` out through the links whose
    /// nearest corner is within `radius` of `at` and not within `avoid_radius` of
    /// `avoid` (through blocked points only if `through_blocked`).
    pub fn cells_in_radius(
        &self,
        root: u32,
        at: Vec3,
        radius: f32,
        avoid: Vec3,
        avoid_radius: f32,
        through_blocked: bool,
    ) -> Vec<u32> {
        let mut cells = vec![root];
        let mut points = vec![root];
        let max = radius * radius;
        let max_avoid = avoid_radius * avoid_radius;
        let mut i = 0;
        while i < points.len() {
            let current = points[i];
            i += 1;
            for l in self.links_of(current) {
                let child = l.to;
                if !through_blocked && self.points[child as usize].blocked {
                    continue;
                }
                let is_cell = self.is_cell(child);
                if is_cell {
                    let (vertex, dist_sq) = self.closest_vertex(child, at);
                    let avoid_sq = (avoid - vertex).length_squared();
                    if dist_sq > max || avoid_sq < max_avoid {
                        continue;
                    }
                }
                if points.contains(&child) {
                    continue;
                }
                points.push(child);
                if is_cell {
                    cells.push(child);
                }
            }
        }
        cells
    }

    /// AI.FindClosestVertexToPoint: a cell's corner nearest `p`, and its distance
    /// squared.
    pub fn closest_vertex(&self, cell: u32, p: Vec3) -> (Vec3, f32) {
        let mut best = (Vec3::ZERO, f32::MAX);
        for v in self.cells[cell as usize].corners {
            let d = (v - p).length_squared();
            if d <= best.1 {
                best = (v, d);
            }
        }
        best
    }

    /// AI.FindTacticalInCells: the tactical nodes on the cells (not blocked ones).
    pub fn tactical_in_cells(&self, cells: &[u32]) -> Vec<u32> {
        let mut out = Vec::new();
        for &c in cells {
            if self.points[c as usize].blocked {
                continue;
            }
            for &n in &self.cells[c as usize].holds {
                if self.tactical(n).is_some() {
                    out.push(n);
                }
            }
        }
        out
    }
}

/// PortalBuilder.PointOutsideEdge.
fn point_outside_edge(p: Vec3, e: &Edge) -> bool {
    e.normal.dot(p - e.start) > 0.0
}

// ---------------------------------------------------------------------------
// Portals and corners (PortalBuilder, StringPulling)
// ---------------------------------------------------------------------------

/// PortalBuilder.EPortalType.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PortalType {
    #[default]
    Walk,
    Jump,
    HighJump,
}

/// A portal: its left and right ends (one point for a point portal).
#[derive(Clone, Copy, Debug)]
pub struct Portal {
    pub left: Vec3,
    pub right: Vec3,
    pub kind: PortalType,
}

impl Portal {
    pub fn new(left: Vec3, right: Vec3, kind: PortalType) -> Portal {
        Portal { left, right, kind }
    }
    pub fn point(at: Vec3, kind: PortalType) -> Portal {
        Portal {
            left: at,
            right: at,
            kind,
        }
    }
}

/// StringPulling.ECornerType.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum CornerType {
    /// A point (the destination, or a point portal walked to).
    #[default]
    Point,
    Left,
    Right,
    Jump,
    HighJump,
    /// None yet (MoveToNextCorner's start).
    None,
}

/// StringPulling.Corner.
#[derive(Clone, Copy, Debug, Default)]
pub struct Corner {
    pub kind: CornerType,
    pub portal: usize,
    pub at: Vec3,
}

impl Corner {
    pub fn new(kind: CornerType, portal: usize, at: Vec3) -> Corner {
        Corner { kind, portal, at }
    }
}

/// StringPulling.TriangleArea (twice the signed area, in the ground plane).
fn triangle_area(a: Vec3, b: Vec3, c: Vec3) -> f32 {
    let ab = b - a;
    let ac = c - a;
    ab.x * ac.z - ab.z * ac.x
}

fn point_right_of_edge(p: Vec3, begin: Vec3, end: Vec3) -> bool {
    triangle_area(p, begin, end) > AREA_EPSILON
}

fn point_left_of_edge(p: Vec3, begin: Vec3, end: Vec3) -> bool {
    triangle_area(p, begin, end) < -AREA_EPSILON
}

fn portal_behind_point(p: &Portal, point: Vec3) -> bool {
    p.left != p.right && !point_left_of_edge(point, p.right, p.left)
}

fn corner_type(kind: PortalType) -> CornerType {
    match kind {
        PortalType::Jump => CornerType::Jump,
        PortalType::HighJump => CornerType::HighJump,
        PortalType::Walk => CornerType::Point,
    }
}

/// StringPulling.Funnel: the next corner on from `start`.
pub fn funnel(start: Corner, portals: &[Portal]) -> Corner {
    let apex = start.at;
    let (mut left, mut right) = (apex, apex);
    let start_index = start.portal + 1;
    let (mut left_index, mut right_index) = (start_index, start_index);
    for (i, current) in portals.iter().enumerate().skip(start_index) {
        if portal_behind_point(current, apex) {
            continue;
        }
        if !point_left_of_edge(current.left, apex, left) {
            if !point_right_of_edge(current.left, apex, right) {
                left = current.left;
                left_index = i;
            } else {
                return Corner::new(CornerType::Right, right_index, right);
            }
        }
        if !point_right_of_edge(current.right, apex, right) {
            if !point_left_of_edge(current.right, apex, left) {
                right = current.right;
                right_index = i;
            } else {
                return Corner::new(CornerType::Left, left_index, left);
            }
        }
        if left == right {
            return Corner::new(corner_type(current.kind), left_index, left);
        }
    }
    start
}

/// StringPulling.MinDistancePointToLineSegment (the projection along the line is
/// whole uu, as the script's int Clamp makes it).
fn min_distance_point_to_segment(begin: Vec3, end: Vec3, point: Vec3) -> f32 {
    let direction = normal2d(end - begin);
    let projected = dot2d(point - begin, direction);
    let len = size2d(end - begin);
    let clamped = ((projected / UU) as i32).clamp(0, (len / UU) as i32) as f32 * UU;
    size2d(point - (begin + direction * clamped))
}

/// StringPulling.CalculateSteeringPoint: a left or right corner's point `radius`
/// out from it, where a circle round it meets the one round the start through it.
fn steering_point(start: Vec3, finish: Vec3, kind: CornerType, radius: f32) -> Vec3 {
    if matches!(kind, CornerType::Point | CornerType::Jump) {
        return finish;
    }
    match intersect_circles(start, (finish - start).length(), finish, radius) {
        None => finish,
        Some((pos, neg)) => {
            if kind == CornerType::Right {
                pos
            } else {
                neg
            }
        }
    }
}

/// StringPulling.AdjustPathForRadius.
fn adjust_for_radius(
    start: Corner,
    finish: Corner,
    radius: f32,
    portals: &[Portal],
    depth: u32,
) -> Corner {
    let mut adjusted = finish;
    adjusted.at = steering_point(start.at, finish.at, finish.kind, radius * 1.5);
    if depth > 64 {
        return adjusted;
    }
    for i in start.portal..adjusted.portal.min(portals.len()) {
        let p = &portals[i];
        if p.left == p.right {
            continue;
        }
        if min_distance_point_to_segment(start.at, adjusted.at, p.left) < radius {
            return adjust_for_radius(
                start,
                Corner::new(CornerType::Left, i, p.left),
                radius,
                portals,
                depth + 1,
            );
        }
        if min_distance_point_to_segment(start.at, adjusted.at, p.right) < radius {
            return adjust_for_radius(
                start,
                Corner::new(CornerType::Right, i, p.right),
                radius,
                portals,
                depth + 1,
            );
        }
    }
    adjusted
}

/// StringPulling.FunnelWithRadius: the next corner for a pawn of `radius`.
pub fn funnel_with_radius(start: Corner, radius: f32, portals: &[Portal]) -> Corner {
    let funneled = funnel(start, portals);
    adjust_for_radius(start, funneled, radius, portals, 0)
}

// ---------------------------------------------------------------------------
// The resource
// ---------------------------------------------------------------------------

/// The map's navigation, while one is loaded: as the map starts (`base`) and as
/// this match has it (its purchases' doors open).
#[derive(Resource, Default)]
pub struct Nav {
    base: Option<NavMesh>,
    pub mesh: Option<NavMesh>,
    /// Purchases' toggles still to come: seconds left, the toggle.
    pending: Vec<(f32, Toggle)>,
}

impl Nav {
    pub fn new(mesh: Option<NavMesh>) -> Nav {
        if mesh.is_none() {
            info!("no navigation for this map: its enemies stand still");
        }
        Nav {
            base: mesh.clone(),
            mesh,
            pending: Vec::new(),
        }
    }

    /// A console's purchase (doors.rs): its toggles, each at its time.
    pub fn bought(&mut self, console: &str) {
        if let Some(m) = &self.mesh {
            for t in m.bought(console) {
                self.pending.push((t.secs, t));
            }
        }
    }
}

/// A new match: the map's navigation as it starts.
fn reset(mut nav: ResMut<Nav>) {
    // (The flights a door can stand across, and the room round them, are the
    // map's: kept, not looked for again.)
    let door_links = nav.mesh.as_ref().and_then(|m| m.door_links.clone());
    let flight_room = nav.mesh.as_ref().map(|m| m.flight_room.clone());
    nav.mesh = nav.base.clone();
    if let Some(m) = nav.mesh.as_mut() {
        m.door_links = door_links;
        if let Some((done, narrow)) = flight_room {
            for &(i, room) in &narrow {
                m.links[i].radius = room;
            }
            m.flight_room = (done, narrow);
        }
    }
    nav.pending.clear();
}

/// The purchases' toggles as their times come.
fn run_toggles(time: Res<Time>, mut nav: ResMut<Nav>, mut doors: ResMut<crate::doors::DoorsState>) {
    for console in doors.take_purchases() {
        nav.bought(&console);
    }
    if nav.pending.is_empty() {
        return;
    }
    let dt = time.delta_secs();
    let mut due = Vec::new();
    nav.pending.retain_mut(|(left, t)| {
        *left -= dt;
        if *left <= 0.0 {
            due.push(t.clone());
            false
        } else {
            true
        }
    });
    if let Some(m) = nav.mesh.as_mut() {
        for t in &due {
            m.switch(t);
        }
    }
}

/// How often the air network's flights are looked at against the doors and
/// scaffolds standing (shut_door_links: a few rays each time).
const DOOR_LINKS_EVERY: f32 = 0.5;

/// Keeps the air network's flights through the locked doors and the scaffolds
/// still standing shut (NavMesh::shut_door_links), as the doors move and the
/// scaffolds come down.
fn shut_door_links(
    time: Res<Time>,
    mut nav: ResMut<Nav>,
    level: Res<crate::Level>,
    mut next: Local<f32>,
) {
    *next -= time.delta_secs();
    if *next > 0.0 {
        return;
    }
    *next = DOOR_LINKS_EVERY;
    if let (Some(mesh), Some(collision)) = (nav.mesh.as_mut(), level.collision.as_ref()) {
        mesh.shut_door_links(collision);
    }
}

/// How long a frame may take measuring the air network's flights (measure_flights),
/// from the map's start till all are.
const FLIGHTS_A_FRAME: std::time::Duration = std::time::Duration::from_millis(2);

/// Measures the air network's flights against the map's own surfaces, a few a
/// frame till all are (NavMesh::measure_flights).
fn measure_flights(mut nav: ResMut<Nav>, level: Res<crate::Level>) {
    if nav.mesh.as_ref().is_none_or(|m| m.flights_measured()) {
        return;
    }
    if let (Some(mesh), Some(collision)) = (nav.mesh.as_mut(), level.collision.as_ref()) {
        mesh.measure_flights(collision, FLIGHTS_A_FRAME);
    }
}

pub struct NavPlugin;

impl Plugin for NavPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Nav>()
            .add_systems(OnEnter(crate::menu::AppState::Game), reset)
            .add_systems(
                Update,
                (run_toggles, shut_door_links, measure_flights)
                    .chain()
                    .after(crate::doors::DoorsSystems)
                    .run_if(in_state(crate::menu::AppState::Game)),
            );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn two_cells() -> NavMesh {
        // Two triangles making a 10 x 10 square, wound as the game's are (clockwise
        // in x, z): (0,0) (10,10) (10,0) and (0,0) (0,10) (10,10), joined across
        // their diagonal.
        let text = "point\t0\tcell\tA\t6.6 0 3.3\t0\t0\t0\n\
point\t1\tcell\tB\t3.3 0 6.6\t0\t1\t0\n\
cell\t0\t0 0 0\t10 0 10\t10 0 0\t1\t-1\t-1\t0 1 0\t0\n\
cell\t1\t0 0 0\t0 0 10\t10 0 10\t-1\t-1\t0\t0 1 0\t0\n\
link\t0\t1\t4.7\t3.3 0 6.6\t5\t5\t1\t0\t0\t0\n\
link\t1\t0\t4.7\t6.6 0 3.3\t5\t5\t1\t0\t0\t0\n";
        NavMesh::parse(text)
    }

    #[test]
    fn finds_cells_and_paths() {
        let m = two_cells();
        assert_eq!(m.cell_at(Vec3::new(8.0, 0.0, 2.0)), Some(0));
        assert_eq!(m.cell_at(Vec3::new(2.0, 0.0, 8.0)), Some(1));
        assert_eq!(m.cell_at(Vec3::new(2.0, 5.0, 8.0)), None);
        let moves = Moves::ground(0.5, 1.0);
        let ch = m.find_channel(0, Vec3::new(2.0, 0.0, 8.0), &moves).unwrap();
        assert_eq!(ch, vec![0, 1]);
        let portals = m.build_portals(Vec3::new(8.0, 0.0, 2.0), Vec3::new(2.0, 0.0, 8.0), &ch);
        // The start, the diagonal, the finish.
        assert_eq!(portals.len(), 3);
        let c = funnel(
            Corner::new(CornerType::Point, 0, Vec3::new(8.0, 0.0, 2.0)),
            &portals,
        );
        // Straight across: the finish itself.
        assert_eq!(c.kind, CornerType::Point);
        assert!((c.at - Vec3::new(2.0, 0.0, 8.0)).length() < 1e-4);
    }

    /// A unit cube's volume (its faces wound out of it): inside it, it encompasses
    /// a point; out of it (beside it or past a corner), not.
    #[test]
    fn leap_volumes_hold_their_insides() {
        let mut text = String::from("leapvol\t0\tCube\t12\n");
        let c = |x: f32, y: f32, z: f32| format!("{x} {y} {z}");
        // Each face's two triangles, counter-clockwise seen from outside.
        let faces = [
            [(0, 0, 0), (0, 1, 0), (1, 1, 0), (1, 0, 0)],
            [(0, 0, 1), (1, 0, 1), (1, 1, 1), (0, 1, 1)],
            [(0, 0, 0), (1, 0, 0), (1, 0, 1), (0, 0, 1)],
            [(0, 1, 0), (0, 1, 1), (1, 1, 1), (1, 1, 0)],
            [(0, 0, 0), (0, 0, 1), (0, 1, 1), (0, 1, 0)],
            [(1, 0, 0), (1, 1, 0), (1, 1, 1), (1, 0, 1)],
        ];
        for f in faces {
            let p: Vec<String> = f
                .iter()
                .map(|&(x, y, z)| c(x as f32, y as f32, z as f32))
                .collect();
            text += &format!("leaptri\t0\t{}\t{}\t{}\n", p[0], p[1], p[2]);
            text += &format!("leaptri\t0\t{}\t{}\t{}\n", p[0], p[2], p[3]);
        }
        let m = NavMesh::parse(&text);
        let v = &m.leap_volumes[0];
        assert_eq!(v.tris.len(), 12);
        assert!(v.encompasses(Vec3::splat(0.5)));
        assert!(v.encompasses(Vec3::new(0.9, 0.1, 0.5)));
        assert!(!v.encompasses(Vec3::new(1.5, 0.5, 0.5)));
        assert!(!v.encompasses(Vec3::new(0.5, -0.2, 0.5)));
        // Ignition's: its floor's middle, and over the arena's wall.
        if let Some(m) = NavMesh::load(&crate::assets_dir(), "ignition") {
            let v = &m.leap_volumes[0];
            assert!(v.tris.len() > 100);
            assert!(v.encompasses(crate::ai::ue::from_ue(Vec3::new(5000.0, 0.0, 100.0))));
            assert!(!v.encompasses(crate::ai::ue::from_ue(Vec3::new(10000.0, 4000.0, 200.0))));
        }
    }

    #[test]
    fn reach_stops_at_borders() {
        let m = two_cells();
        assert!(
            m.is_reachable_from_cell(0, Vec3::new(8.0, 0.0, 2.0), Vec3::new(2.0, 0.0, 8.0), 0.5)
                .is_ok()
        );
        // Out past the square's side.
        assert!(
            m.is_reachable_from_cell(0, Vec3::new(8.0, 0.0, 2.0), Vec3::new(12.0, 0.0, 2.0), 0.5)
                .is_err()
        );
        let back = m.safe_direction_back(0, Vec3::new(9.8, 0.0, 2.0), 0.5);
        assert!(back.x < -0.9);
    }

    /// The maps' own meshes: every wave spawn point that stands on the mesh can
    /// reach the players' start (behind the doors only once they open).
    #[test]
    fn maps_paths() {
        let dir = crate::assets_dir();
        for stem in ["ignition", "downfall", "oblivion", "ancients"] {
            let Some(mesh) = NavMesh::load(&dir, stem) else {
                eprintln!("{stem}: no navigation built, skipped");
                continue;
            };
            let points =
                std::fs::read_to_string(dir.join(format!("levels/{stem}_points.txt"))).unwrap();
            let start = points
                .lines()
                .find(|l| l.starts_with("start "))
                .map(|l| {
                    let v: Vec<f32> = l
                        .split_whitespace()
                        .skip(1)
                        .filter_map(|x| x.parse().ok())
                        .collect();
                    Vec3::new(v[0], v[1], v[2])
                })
                .unwrap();
            let start_cell = mesh
                .cell_at(start)
                .or_else(|| mesh.nearest_cell(start, 3.0).map(|(c, _)| c));
            let waves =
                std::fs::read_to_string(dir.join(format!("levels/{stem}_waves.txt"))).unwrap();
            let (mut on, mut reached, mut off) = (0, 0, 0);
            let moves = Moves::ground(0.5, 2.0);
            for l in waves.lines().filter(|l| l.starts_with("point\t")) {
                let r: Vec<&str> = l.split('\t').collect();
                let p = Vec3::new(
                    r[2].parse().unwrap(),
                    r[3].parse().unwrap(),
                    r[4].parse().unwrap(),
                );
                let Some(c) = mesh.cell_at(p) else {
                    off += 1;
                    continue;
                };
                on += 1;
                if mesh.find_channel(c, start, &moves).is_some() {
                    reached += 1;
                }
            }
            eprintln!(
                "{stem}: {} cells, {} links, {} tactical; start on {:?}; spawn points on the mesh {on} \
                 (off {off}), reaching the start {reached}",
                mesh.cells.len(),
                mesh.links.len(),
                mesh.tacticals.len(),
                start_cell
            );
            assert!(start_cell.is_some(), "{stem}: the start isn't on the mesh");
            assert!(reached > 0, "{stem}: no spawn point reaches the start");
            // A path's corners: from the first reaching spawn point to the start.
            for l in waves.lines().filter(|l| l.starts_with("point\t")) {
                let r: Vec<&str> = l.split('\t').collect();
                let p = Vec3::new(
                    r[2].parse().unwrap(),
                    r[3].parse().unwrap(),
                    r[4].parse().unwrap(),
                );
                let Some(c) = mesh.cell_at(p) else { continue };
                let Some(ch) = mesh.find_channel(c, start, &moves) else {
                    continue;
                };
                let portals = mesh.build_portals(p, start, &ch);
                let mut corner = Corner::new(CornerType::None, 0, p);
                let mut corners = Vec::new();
                for _ in 0..200 {
                    let next = funnel_with_radius(corner, 0.5, &portals);
                    if next.at == corner.at && next.portal == corner.portal {
                        break;
                    }
                    corners.push(next);
                    if next.portal + 1 >= portals.len() {
                        break;
                    }
                    corner = next;
                }
                let last = corners.last().unwrap();
                eprintln!(
                    "  {} -> start: {} points, {} portals, {} corners, ends {:.2} m from it",
                    r[1],
                    ch.len(),
                    portals.len(),
                    corners.len(),
                    last.at.distance(start)
                );
                assert!(last.at.distance(start) < 0.5);
                break;
            }
        }
    }

    /// (A report, run by hand: `cargo test nav_links_through_walls -- --ignored
    /// --nocapture`, FOC_NAV_AUDIT naming the maps.) The links between neighbouring
    /// cells that are open (neither blocked) but that a body can't walk along for
    /// the level's collision (an AI's: the players-only walls let it by), the doors
    /// standing closed as gameplay starts: straight from one cell's middle over
    /// their shared edge's to the other's, a body 0.3 m round and 1.8 m high pushed
    /// out of a wall on the way. Each place's links counted on a 2 m grid.
    #[test]
    #[ignore]
    fn nav_links_through_walls() {
        let maps = std::env::var("FOC_NAV_AUDIT").unwrap_or_else(|_| "downfall".into());
        for stem in maps.split(',') {
            let dir = crate::assets_dir();
            let Some(map) = crate::MAPS.iter().find(|m| m.stem == stem) else {
                continue;
            };
            let (Some(level), Some(mesh)) =
                (crate::load_level_data(map), NavMesh::load(&dir, stem))
            else {
                eprintln!("{stem}: not built, skipped");
                continue;
            };
            let middle = |c: u32| {
                let k = &mesh.cells[c as usize].corners;
                (k[0] + k[1] + k[2]) / 3.0
            };
            let mut places: HashMap<(i32, i32, i32), (usize, Vec3, Vec3)> = HashMap::new();
            let mut open = 0;
            for (i, cell) in mesh.cells.iter().enumerate() {
                let i = i as u32;
                if mesh.points[i as usize].blocked {
                    continue;
                }
                for k in 0..3 {
                    let Some(nb) = cell.neighbours[k] else {
                        continue;
                    };
                    if nb <= i || mesh.points[nb as usize].blocked {
                        continue;
                    }
                    open += 1;
                    let e = mesh.edge(i, k);
                    let mid = (e.start + e.end) * 0.5;
                    let legs = [(i, middle(i), mid), (nb, mid, middle(nb))];
                    let mut hit = None;
                    'walk: for (c, a, b) in legs {
                        let n = ((b - a).length() / 0.1).ceil().max(1.0) as usize;
                        for s in 0..=n {
                            let p = a.lerp(b, s as f32 / n as f32);
                            let feet = Vec3::new(p.x, mesh.height_at(c, p.x, p.z), p.z);
                            if let Some(push) =
                                level.push_out_for(feet, 0.3, 1.8, crate::collision::Mover::Ai)
                                && push.length() > 0.05
                            {
                                hit = Some(feet);
                                break 'walk;
                            }
                        }
                    }
                    if let Some(at) = hit {
                        let key = (
                            (at.x / 2.0).floor() as i32,
                            (at.y / 2.0).floor() as i32,
                            (at.z / 2.0).floor() as i32,
                        );
                        let e = places.entry(key).or_insert((0, at, mid));
                        e.0 += 1;
                    }
                }
            }
            let mut list: Vec<_> = places.into_values().collect();
            list.sort_by_key(|e| std::cmp::Reverse(e.0));
            eprintln!(
                "{stem}: {open} open links between cells, {} places with links through walls",
                list.len()
            );
            for (n, at, mid) in list.iter().take(400) {
                eprintln!(
                    "  {n} at {:.1} {:.1} {:.1} (edge {:.1} {:.1} {:.1})",
                    at.x, at.y, at.z, mid.x, mid.y, mid.z
                );
            }
        }
    }

    /// (A report, run by hand as the one above: FOC_NAV_AUDIT naming the maps.) The
    /// places on open cells a body 0.3 m round and 1.8 m high can't stand for the
    /// level's collision (an AI's), the doors standing closed as gameplay starts:
    /// points 0.5 m apart over each cell, more than 0.4 m in from its edges (the
    /// game's mesh keeps clear of walls by its pawns' radius), on a 2 m grid.
    #[test]
    #[ignore]
    fn nav_cells_in_walls() {
        let maps = std::env::var("FOC_NAV_AUDIT").unwrap_or_else(|_| "downfall".into());
        for stem in maps.split(',') {
            let dir = crate::assets_dir();
            let Some(map) = crate::MAPS.iter().find(|m| m.stem == stem) else {
                continue;
            };
            let (Some(level), Some(mesh)) =
                (crate::load_level_data(map), NavMesh::load(&dir, stem))
            else {
                eprintln!("{stem}: not built, skipped");
                continue;
            };
            let mut places: HashMap<(i32, i32, i32), (usize, Vec3, u32)> = HashMap::new();
            for (i, cell) in mesh.cells.iter().enumerate() {
                let i = i as u32;
                if mesh.points[i as usize].blocked {
                    continue;
                }
                let lo = cell
                    .corners
                    .iter()
                    .fold(Vec3::splat(f32::MAX), |a, b| a.min(*b));
                let hi = cell
                    .corners
                    .iter()
                    .fold(Vec3::splat(f32::MIN), |a, b| a.max(*b));
                let mut x = lo.x;
                while x <= hi.x {
                    let mut z = lo.z;
                    while z <= hi.z {
                        let p = Vec3::new(x, 0.0, z);
                        if mesh.inside2d(i, p, -0.4) {
                            let feet = Vec3::new(x, mesh.height_at(i, x, z), z);
                            if let Some(push) =
                                level.push_out_for(feet, 0.3, 1.8, crate::collision::Mover::Ai)
                                && push.length() > 0.05
                            {
                                let key = (
                                    (x / 2.0).floor() as i32,
                                    (feet.y / 2.0).floor() as i32,
                                    (z / 2.0).floor() as i32,
                                );
                                places.entry(key).or_insert((0, feet, i)).0 += 1;
                            }
                        }
                        z += 0.5;
                    }
                    x += 0.5;
                }
            }
            let mut list: Vec<_> = places.into_values().collect();
            list.sort_by(|a, b| (a.1.x, a.1.z).partial_cmp(&(b.1.x, b.1.z)).unwrap());
            eprintln!("{stem}: {} places on open cells in walls", list.len());
            for (n, at, c) in &list {
                eprintln!("  {n} at {:.1} {:.1} {:.1} (cell {c})", at.x, at.y, at.z);
            }
        }
    }

    /// (A report, run by hand: FOC_NAV_GRID="map:x0,z0,x1,z1:y[:radius[:height]]".) A map
    /// from above of a stretch of a level 0.5 m to a character, at the floor
    /// nearest height y: '#' where a body of the radius and height (0.3 m and
    /// 1.8 m by default) can't stand for an AI's collision, '.' on an open cell of the mesh,
    /// 'b' on a blocked one, 'X' a wall on a cell, ' ' open floor off the mesh,
    /// '~' no floor; x across, z down.
    #[test]
    #[ignore]
    fn nav_grid_map() {
        let Ok(arg) = std::env::var("FOC_NAV_GRID") else {
            return;
        };
        let f: Vec<&str> = arg.split(':').collect();
        let stem = f[0];
        let r: Vec<f32> = f[1].split(',').filter_map(|v| v.parse().ok()).collect();
        let y: f32 = f[2].parse().unwrap_or(0.0);
        let radius: f32 = f.get(3).and_then(|v| v.parse().ok()).unwrap_or(0.3);
        let height: f32 = f.get(4).and_then(|v| v.parse().ok()).unwrap_or(1.8);
        let dir = crate::assets_dir();
        let map = crate::MAPS
            .iter()
            .find(|m| m.stem == stem)
            .expect("the map");
        let (Some(level), Some(mesh)) = (crate::load_level_data(map), NavMesh::load(&dir, stem))
        else {
            eprintln!("{stem}: not built");
            return;
        };
        let mut z = r[1];
        eprintln!("x from {} to {}, z from {} to {}", r[0], r[2], r[1], r[3]);
        while z <= r[3] {
            let mut line = format!("{z:7.1} ");
            let mut x = r[0];
            while x <= r[2] {
                let floor = level.ground_for(
                    Vec3::new(x, y + 1.0, z),
                    0.05,
                    y + 1.0,
                    y - 2.0,
                    crate::collision::Mover::Ai,
                );
                let c = match floor {
                    None => '~',
                    Some(fy) => {
                        let feet = Vec3::new(x, fy, z);
                        let wall = level
                            .push_out_for(feet, radius, height, crate::collision::Mover::Ai)
                            .is_some_and(|v| v.length() > 0.05);
                        let cell = mesh
                            .cell_at(feet)
                            .filter(|&c| (mesh.height_at(c, x, z) - fy).abs() < 1.0);
                        match (wall, cell) {
                            (true, Some(_)) => 'X',
                            (true, None) => '#',
                            (false, Some(c)) if mesh.points[c as usize].blocked => 'b',
                            (false, Some(_)) => '.',
                            (false, None) => ' ',
                        }
                    }
                };
                line.push(c);
                x += 0.5;
            }
            eprintln!("{line}");
            z += 0.5;
        }
    }

    /// (A report, run by hand: FOC_RAY="map:x,y,z:x,y,z[;...]".) What the level's
    /// collision has between each pair of places: the first surface a ray meets
    /// (an AI's), its distance and normal.
    #[test]
    #[ignore]
    fn ray_probe() {
        let Ok(arg) = std::env::var("FOC_RAY") else {
            return;
        };
        let (stem, rest) = arg.split_once(':').unwrap();
        let map = crate::MAPS
            .iter()
            .find(|m| m.stem == stem)
            .expect("the map");
        let level = crate::load_level_data(map).expect("the level");
        let v = |s: &str| {
            let f: Vec<f32> = s.split(',').filter_map(|x| x.parse().ok()).collect();
            Vec3::new(f[0], f[1], f[2])
        };
        for pair in rest.split(';') {
            let (a, b) = pair.split_once(':').unwrap();
            let (a, b) = (v(a), v(b));
            let d = (b - a).length();
            let hit = level.ray_hit_for(a, (b - a) / d, d, crate::collision::Mover::Ai);
            let solid = level.ray_hit(a, (b - a) / d, d);
            eprintln!("{a} to {b} ({d:.1} m): {hit:?}, the solid's {solid:?}");
        }
    }

    /// (A report, run by hand: FOC_NAV_AUDIT naming the maps.) The air network's
    /// links (the snipers' and jets' flights) that the level's collision stands in
    /// the way of, as an AI's flight meets it (a ray's: blocking volumes and all,
    /// the doors closed and the purchases' volumes on as gameplay starts), and
    /// whether rays alone (the solid geometry) are stopped too.
    #[test]
    #[ignore]
    fn air_links_through_walls() {
        let maps = std::env::var("FOC_NAV_AUDIT").unwrap_or_else(|_| "ignition".into());
        for stem in maps.split(',') {
            let dir = crate::assets_dir();
            let Some(map) = crate::MAPS.iter().find(|m| m.stem == stem) else {
                continue;
            };
            let (Some(level), Some(mesh)) =
                (crate::load_level_data(map), NavMesh::load(&dir, stem))
            else {
                continue;
            };
            let (mut all, mut volume, mut solid) = (0, 0, 0);
            for (i, p) in mesh.points.iter().enumerate() {
                for l in mesh.links_of(i as u32) {
                    if !l.fly || (l.to as usize) <= i {
                        continue;
                    }
                    all += 1;
                    let (a, b) = (p.at, mesh.points[l.to as usize].at);
                    let d = (b - a).length();
                    if d < 0.1 {
                        continue;
                    }
                    let dir_ = (b - a) / d;
                    let moving = level.ray_hit_for(a, dir_, d, crate::collision::Mover::Ai);
                    let rays = level.ray_hit(a, dir_, d);
                    match (moving, rays) {
                        (Some((t, n)), None) => {
                            volume += 1;
                            eprintln!(
                                "  volume: {} {} -> {} {} at {:.1} m ({:?}), normal {n}",
                                i,
                                p.name,
                                l.to,
                                mesh.points[l.to as usize].name,
                                t,
                                a + dir_ * t
                            );
                        }
                        (Some(_), Some(_)) => solid += 1,
                        _ => {}
                    }
                }
            }
            eprintln!(
                "{stem}: {all} air links, {volume} through blocking volumes alone, {solid} through solid geometry"
            );
        }
    }

    /// The air network's flights that Downfall's locked doors and standing
    /// scaffolds stand across are shut as gameplay starts (the Snipers' and the
    /// jets' paths go round them), and open again once those are gone.
    #[test]
    fn flights_through_the_doors_are_shut_till_they_open() {
        let Some(map) = crate::MAPS.iter().find(|m| m.stem == "downfall") else {
            return;
        };
        let dir = crate::assets_dir();
        let (Some(mut level), Some(mut mesh)) =
            (crate::load_level_data(map), NavMesh::load(&dir, "downfall"))
        else {
            eprintln!("Downfall's level or navigation not built: skipped");
            return;
        };
        mesh.shut_door_links(level.collision.as_ref().unwrap());
        let shut = mesh.links.iter().filter(|l| l.shut).count();
        eprintln!("{shut} flights shut as gameplay starts");
        assert!(shut >= 7, "only {shut} flights shut");
        assert!(mesh.links.iter().all(|l| !l.shut || l.fly));
        // The way from the hangar's middle perch to the one at its west end, through
        // a scaffold: no Sniper takes it while it stands.
        let a = mesh.point_named("TnSniperPoint_10606").expect("the perch");
        let b = mesh.point_named("TnSniperPoint_3320").expect("the other");
        let link = *mesh.link_between(a, b).expect("their flight");
        assert!(link.shut);
        assert!(!Moves::air(1.0, 2.0).supports(&link));
        // The doors open and the scaffolds down: every flight open again.
        let doors = level.doors.clone();
        let c = level.collision.as_mut().unwrap();
        for d in &doors.doors {
            if let Some(g) = d.closed {
                c.set_group(g, false);
            }
        }
        mesh.shut_door_links(level.collision.as_ref().unwrap());
        assert_eq!(mesh.links.iter().filter(|l| l.shut).count(), 0);
        assert!(Moves::air(1.0, 2.0).supports(mesh.link_between(a, b).unwrap()));
    }

    /// Downfall's flights to TnSniperPoint_2779 graze a pillar of the map's: they
    /// come out narrower than any flier, and the Snipers' and the jets' ways go round
    /// them; most flights keep the room the network gives them.
    #[test]
    fn flights_the_maps_own_surfaces_narrow_are_narrowed() {
        let Some(map) = crate::MAPS.iter().find(|m| m.stem == "downfall") else {
            return;
        };
        let dir = crate::assets_dir();
        let (Some(level), Some(mut mesh)) =
            (crate::load_level_data(map), NavMesh::load(&dir, "downfall"))
        else {
            eprintln!("Downfall's level or navigation not built: skipped");
            return;
        };
        let collision = level.collision.as_ref().unwrap();
        let flights = mesh.links.iter().filter(|l| l.fly).count();
        let mut frames = 0;
        let clock = std::time::Instant::now();
        while !mesh.flights_measured() {
            mesh.measure_flights(collision, std::time::Duration::from_millis(2));
            frames += 1;
        }
        eprintln!("measured in {:.2} s", clock.elapsed().as_secs_f32());
        for &(i, r) in &mesh.flight_room.1 {
            let from = mesh.first.partition_point(|&f| f as usize <= i) - 1;
            eprintln!(
                "  {} -> {}: {r:.2}",
                mesh.points[from].name, mesh.points[mesh.links[i].to as usize].name
            );
        }
        let narrow = mesh.flight_room.1.clone();
        eprintln!(
            "{} of {flights} flights narrowed, in {frames} steps: {:?}",
            narrow.len(),
            narrow
                .iter()
                .map(|&(i, r)| (i, (r * 100.0).round() / 100.0))
                .collect::<Vec<_>>()
        );
        assert!(narrow.len() * 20 < flights);
        let perch = mesh.point_named("TnSniperPoint_2779").expect("the perch");
        let sniper = Moves::air(1.75, 4.0);
        let jet = Moves::air(1.5, 4.4);
        for from in ["AirPathNode_14368", "AirPathNode_2293", "AirPathNode_4086"] {
            let a = mesh.point_named(from).expect("the node");
            let link = mesh.link_between(a, perch).expect("its flight");
            assert!(link.radius < 1.5, "{from}: {}", link.radius);
            assert!(!sniper.supports(link) && !jet.supports(link));
        }
        // (A match later the network is the map's again, as narrowed.)
        let nav = Nav {
            base: NavMesh::load(&dir, "downfall"),
            mesh: Some(mesh),
            pending: Vec::new(),
        };
        let mut app = App::new();
        app.insert_resource(nav);
        app.add_systems(Update, reset);
        app.update();
        let mesh = app.world().resource::<Nav>().mesh.as_ref().unwrap();
        assert!(mesh.flights_measured());
        let a = mesh.point_named("AirPathNode_14368").unwrap();
        assert!(mesh.link_between(a, perch).unwrap().radius < 1.5);
    }
}
