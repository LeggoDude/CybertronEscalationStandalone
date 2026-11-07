//! The Snipers (TnSniperCharacterBlueprint: Downfall's Sniper_AUT, Ancients' and
//! Ignition's Sniper) and their perches, the levels' TnSniperPoints. A Sniper flies
//! in its robot form from perch to perch and fights from them in its insect form,
//! clinging to the wall (its forms: forms.rs).
//!
//! * **The points** ([`Point`]): each a patch of wall (its middle on the wall, its
//!   normal, its safe extent along the wall's axes), its navigation point out from
//!   it on the air network (717 uu: the paths' and the approach's), claimed by one
//!   Sniper at a time ([`Claims`]: TnSniperPoint.Claim, bBlocked, which keeps the
//!   other Snipers' paths out of it), and its tactical node on the wall, whose line
//!   of sight to each team the tactical system keeps (nav.rs's sniper tacticals).
//! * **Its data store** ([`Store`]: TnSniperAiDataStore): the path it flies (points
//!   of the air network, its last the sniper point it is headed for, claimed), its
//!   enemy's team, the point it is on (CurrentPoint) and the level's goal point
//!   (none in Escalation: only Kismet sets one), and the point it last passed (the
//!   pawn's Anchor, SetAnchor).
//! * **Its sensor** ([`Sensor`]: TnSniperPointSensor): every point its to use, its
//!   enemy seen for the tactical system (SawEnemy: the points' lines of sight are
//!   to him), WS_OnNavMesh once it is on a point and WS_SearchAvailable with a goal.
//! * **Its operators**: choosing a point (to start from, toward its goal, to search
//!   from, to attack from), flying there in its robot form ([`FlyTo`], its boosters
//!   lit: TnAiSniperBoosterEffects), taking its insect form on the wall
//!   ([`Attach`]), crawling about its patch ([`Crawl`]) and leaving it ([`Detach`]:
//!   its robot form, off the wall, turned towards its next point).

use std::collections::HashMap;
use std::sync::Arc;

use bevy::prelude::*;

use super::brain::{
    BEHAVIOR_AIMER, BEHAVIOR_ALLOW_MOTION, BEHAVIOR_HEAD_TRACKING, BEHAVIOR_ROTATE_TO_FOCUS,
    BEHAVIOR_WEAPON_TRACKING, Brain, Ctx, Event, EventId, LR_SUSPECTED_ENEMY,
};
use super::htn::{self, Status, Task};
use super::pawn::{Pawn, Physics};
use super::ue;
use crate::nav::{Moves, NavMesh, PointKind};

/// AI.ESensor's SENSOR_SniperPoint.
pub const S_SNIPER_POINT: u8 = 16;
/// TnSniperPoint.GetPathingRadius (uu).
pub const PATHING_RADIUS: f32 = 500.0;

// ---------------------------------------------------------------------------
// The points
// ---------------------------------------------------------------------------

/// A sniper point's claim (TnSniperPoint.Claim and Release: bBlocked, _TimeClaimed,
/// _TimeLastClaimed).
#[derive(Clone, Copy, Debug, Default)]
pub struct Claim {
    pub claimed: bool,
    pub time_claimed: f32,
    pub time_last_claimed: f32,
}

/// Every sniper point's claim (by its navigation point): the Snipers' between them.
#[derive(Clone, Debug, Default)]
pub struct Claims(pub HashMap<u32, Claim>);

impl Claims {
    pub fn get(&self, p: u32) -> Claim {
        self.0.get(&p).copied().unwrap_or_default()
    }

    /// Claimed by a sniper (Claim's bBlocked; is_claimed_in for its whole).
    pub fn is_claimed(&self, p: u32) -> bool {
        self.get(p).claimed
    }

    /// get_IsClaimed: bBlocked, which a sniper's claim sets and the map's Kismet
    /// does too (Downfall's toggles block most of its sniper points, the ones
    /// beyond its locked doors, until the door is bought: the navigation's blocked
    /// points). Taken as free, they had the snipers choosing perches on the far
    /// side of a closed door and flying into it, where they hung in the air.
    pub fn is_claimed_in(&self, nav: &NavMesh, p: u32) -> bool {
        self.is_claimed(p) || nav.points.get(p as usize).is_some_and(|pt| pt.blocked)
    }

    pub fn claim(&mut self, p: u32, now: f32) {
        let c = self.0.entry(p).or_default();
        c.time_claimed = now;
        c.claimed = true;
    }

    pub fn release(&mut self, p: u32, now: f32) {
        let c = self.0.entry(p).or_default();
        c.time_last_claimed = now;
        c.claimed = false;
    }

    /// get_TimeSinceLastClaimed (none while it is claimed).
    pub fn time_since_last_claimed(&self, p: u32, now: f32) -> f32 {
        let c = self.get(p);
        if c.claimed {
            0.0
        } else {
            now - c.time_last_claimed
        }
    }
}

/// A sniper point (Unreal space): its navigation point and where that is (out
/// from the wall), the middle of its patch of wall (_CenterLocation: the wall trace's
/// hit, its box's 50 uu off the wall), the wall's normal, its actor's up axis, its
/// patch's half sizes along the wall's axes (_SafeLocationExtent), how far out an
/// approach begins (_MaxApproachDistance) and whether its wall was found.
#[derive(Clone, Copy, Debug)]
pub struct Point {
    pub id: u32,
    pub location: Vec3,
    pub centre: Vec3,
    pub normal: Vec3,
    pub up: Vec3,
    pub extent: (f32, f32),
    pub max_approach: f32,
    pub valid: bool,
}

/// TnSniperPoint's _WallTraceExtent (its wall trace's box: the patch's middle is
/// this far off the wall along each axis).
const WALL_TRACE_EXTENT: f32 = 50.0;

impl Point {
    /// The sniper point at navigation point `id`, if it is one.
    pub fn get(nav: &NavMesh, id: u32) -> Option<Point> {
        let sp = nav.snipers.iter().find(|s| s.point == id)?;
        Some(Point {
            id,
            location: ue::to_ue(nav.points[id as usize].at),
            centre: ue::to_ue(sp.centre),
            normal: ue::normal(ue::dir_to_ue(sp.wall_normal)),
            up: ue::normal(ue::dir_to_ue(sp.up)),
            extent: (sp.extent.x * ue::UU_PER_M, sp.extent.z * ue::UU_PER_M),
            max_approach: sp.max_approach * ue::UU_PER_M,
            valid: sp.wall_valid,
        })
    }

    /// Every sniper point of the level.
    pub fn all(nav: &NavMesh) -> Vec<u32> {
        nav.snipers.iter().map(|s| s.point).collect()
    }

    /// get_Orientation (GetOrientationFromNormal): 2 a ceiling (its normal down more
    /// than 45°), 3 a floor (up more than 45°), 1 a wall.
    pub fn orientation(&self) -> u8 {
        let c45 = (45.0 * ue::DEG_TO_RAD).cos();
        if self.normal.z < -c45 {
            2
        } else if self.normal.z > c45 {
            3
        } else {
            1
        }
    }

    /// Its wall's axes (GetWallNormalAxes, as get_WallNormalRotation has them: out
    /// of the wall, across it (up crossed with the normal), up it).
    pub fn axes(&self) -> [Vec3; 3] {
        let x = self.normal;
        let y = ue::normal(self.up.cross(x));
        let z = x.cross(y);
        [x, y, z]
    }

    /// TnSniperPoint.GetRandomPointFor: a place on its patch for a pawn at `at` of
    /// `radius` and `height` (its height its radius but on a wall): along each of the
    /// wall's axes, at random on the side of it with more room (its middle's along an
    /// axis it has no room on).
    pub fn random_point_for(&self, at: Vec3, radius: f32, height: f32, rng: &mut ue::Rng) -> Vec3 {
        let [_, y, z] = self.axes();
        let h = if self.orientation() == 1 {
            height
        } else {
            radius
        };
        self.centre
            + self.random_on_axis(at, radius, y, self.extent.0, rng)
            + self.random_on_axis(at, h, z, self.extent.1, rng)
    }

    /// GetRandomPointOnAxis.
    fn random_on_axis(
        &self,
        at: Vec3,
        pawn_half: f32,
        axis: Vec3,
        axis_half: f32,
        rng: &mut ue::Rng,
    ) -> Vec3 {
        if 2.0 * pawn_half >= axis_half {
            return Vec3::ZERO;
        }
        let half = axis_half - pawn_half;
        let (left, right) = (-half, half);
        let centre = (at - self.centre).dot(axis).clamp(left, right);
        let (pl, pr) = (centre - pawn_half, centre + pawn_half);
        let r = if pl - left > right - pr {
            rng.range(left, pl)
        } else {
            rng.range(pr, right)
        };
        axis * r
    }

    /// GetSurroundingPoints: the sniper points its navigation connections lead to
    /// (the port's: those with room for a flier of `radius`, metres, and not shut).
    /// A Sniper flies to one of these straight, off its path's line only as far as
    /// it is pushed; the flights the map's surfaces leave too little room for
    /// (NavMesh::measure_flights: on Ancients those between TnSniperPoint_10859 and
    /// TnSniperPoint_7942, and from TnSniperPoint_14899 to TnSniperPoint_2368 and
    /// TnSniperPoint_16349, through the scaffolding round its middle) held it
    /// against the scaffolding for the rest of the wave, each push off it taking it
    /// back in.
    pub fn surrounding(&self, nav: &NavMesh, radius: f32) -> Vec<u32> {
        nav.links_of(self.id)
            .iter()
            .filter(|l| nav.points[l.to as usize].kind == PointKind::Sniper)
            .filter(|l| !l.shut && (l.radius <= 0.0 || l.radius >= radius))
            .map(|l| l.to)
            .collect()
    }

    /// Where an insect clings to its patch's middle: AttachToWall's move there
    /// (AdjustInteracter's MoveSmooth) stops as its cylinder (`radius`, half
    /// `height`) meets the wall, its middle's trace box 50 uu off it.
    pub fn perch(&self, radius: f32, height: f32) -> Vec3 {
        let n = self.normal;
        let box_out = WALL_TRACE_EXTENT * (n.x.abs() + n.y.abs() + n.z.abs());
        let cylinder_out = radius * (n.x * n.x + n.y * n.y).sqrt() + height * n.z.abs();
        self.centre + n * (cylinder_out - box_out)
    }
}

/// The radius and half height (uu) of the form it flies in: its robot's.
fn flight_size(p: &Pawn) -> (f32, f32) {
    p.forms.as_ref().map_or((p.radius, p.half_height), |f| {
        let c = f.caps[super::forms::ROBOT as usize];
        (c.radius, c.half_height)
    })
}

/// The radius (metres) of the form it flies in.
fn flight_radius(p: &Pawn) -> f32 {
    flight_size(p).0 / ue::UU_PER_M
}

/// A pawn's movement capabilities for its paths (its robot form's: flying,
/// _PrimaryMovementCapabilities' bCanFly).
fn moves(p: &Pawn, nav_radius: f32) -> Moves {
    Moves::air(
        nav_radius / ue::UU_PER_M,
        p.half_height * 2.0 / ue::UU_PER_M,
    )
}

/// Pathing.FindChannel over the air network: the points from `start` to `goal`,
/// the claimed sniper points (bBlocked) kept out of it but its ends.
fn find_channel(
    nav: &NavMesh,
    claims: &Claims,
    start: u32,
    goal: u32,
    moves: &Moves,
) -> Option<Vec<u32>> {
    nav.find_channel_avoiding(start, goal, moves, |p| {
        p != start && p != goal && claims.is_claimed_in(nav, p)
    })
}

// ---------------------------------------------------------------------------
// Its data store and sensor
// ---------------------------------------------------------------------------

/// TnSniperAiDataStore (and its pawn's Anchor, the point it last passed).
#[derive(Clone, Debug, Default)]
pub struct Store {
    pub path: Vec<u32>,
    pub enemy_team: i32,
    pub current: Option<u32>,
    pub goal: Option<u32>,
    pub anchor: Option<u32>,
    /// The navigation point it came in at (its pawn's SpawnPoint: an air spawn
    /// point, on the air network).
    pub spawn_point: Option<u32>,
}

impl Store {
    /// get_NextPointOnPath.
    pub fn next_point(&self) -> Option<u32> {
        self.path.first().copied()
    }

    /// get_DestinationPoint (the path's last: a sniper point).
    pub fn destination(&self) -> Option<u32> {
        self.path.last().copied()
    }

    /// ClaimPoint: its destination released, the point its path and claimed.
    pub fn claim_point(&mut self, p: u32, claims: &mut Claims, now: f32) {
        if let Some(d) = self.destination() {
            claims.release(d, now);
        }
        self.path = vec![p];
        claims.claim(p, now);
    }

    /// ClaimPath.
    pub fn claim_path(&mut self, path: Vec<u32>, claims: &mut Claims, now: f32) {
        if let Some(d) = self.destination() {
            claims.release(d, now);
        }
        self.path = path;
        if let Some(d) = self.destination() {
            claims.claim(d, now);
        }
    }

    /// Release: the point it is on released.
    pub fn release(&mut self, claims: &mut Claims, now: f32) {
        if let Some(c) = self.current.take() {
            claims.release(c, now);
        }
    }

    /// PointReached: off its path.
    pub fn point_reached(&mut self, p: u32) {
        if !self.path.is_empty() {
            debug_assert_eq!(self.path[0], p);
            self.path.remove(0);
        }
    }
}

/// TnSniperPointSensor.
#[derive(Clone, Debug, Default)]
pub struct Sensor {
    pub available: Vec<u32>,
    pub any_available: bool,
}

impl Sensor {
    /// OnInitialize: every point available, their tactical nodes its collection.
    pub fn initialize(&mut self, ctx: &Ctx) {
        self.any_available = true;
        self.available = ctx.view.nav.map(Point::all).unwrap_or_default();
    }

    /// OnUpdate: its enemy seen for the tactical system and its team kept, then
    /// WS_OnNavMesh once it is on a point, WS_SearchAvailable with a goal; its
    /// points' tactical nodes for the updater (its collection's).
    pub fn update(&mut self, b: &mut Brain, ctx: &mut Ctx) {
        let store = b.sensors.sniper_store.get_or_insert_default();
        if let Some(e) = b.core.enemy
            && let Some(a) = ctx.view.actor(e)
        {
            let at = b.core.fetch(LR_SUSPECTED_ENEMY);
            ctx.shared.tactical.saw_enemy(e, a.team, at);
            store.enemy_team = a.team as i32;
        }
        let on = store.current.is_some();
        let goal = store.goal.is_some();
        if on {
            b.core.set_ws_bool(htn::WS_ON_NAV_MESH, true);
        }
        b.core.set_ws_bool(htn::WS_SEARCH_AVAILABLE, goal);
        if let Some(nav) = ctx.view.nav {
            ctx.shared.tactical.enqueue(nav, &self.available);
        }
    }

    /// IsPointAvailable.
    pub fn is_point_available(&self, p: u32) -> bool {
        self.any_available || self.available.contains(&p)
    }

    /// OnShutdown: the point it is on and the one it was headed for released.
    pub fn shutdown(&mut self, b: &mut Brain, ctx: &mut Ctx) {
        let now = ctx.view.now;
        if let Some(store) = b.sensors.sniper_store.as_mut() {
            if let Some(c) = store.current {
                ctx.shared.sniper_claims.release(c, now);
            }
            if let Some(d) = store.destination() {
                ctx.shared.sniper_claims.release(d, now);
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Choosing a point
// ---------------------------------------------------------------------------

/// TnChooseNewSniperPointOperator.OnStart: its goal point if it has one it can get
/// to; else a path from its spawn point to the first sniper point free along it;
/// else the nearest free point it can see.
pub fn choose_point(b: &mut Brain, ctx: &mut Ctx) -> bool {
    let Some(nav) = ctx.view.nav else {
        return false;
    };
    let now = ctx.view.now;
    let store = b.sensors.sniper_store.get_or_insert_default();
    let claims = &mut ctx.shared.sniper_claims;
    let mv = moves(ctx.pawn, ctx.pawn.radius);
    // TryToMoveToGoalPoint.
    if let Some(goal) = store.goal
        && !claims.is_claimed_in(nav, goal)
    {
        if let Some(path) = store
            .spawn_point
            .and_then(|s| find_channel(nav, claims, s, goal, &mv))
        {
            store.claim_path(path, claims, now);
            return true;
        }
        if let Some(gp) = Point::get(nav, goal)
            && ctx.view.fast_trace(gp.location, ctx.pawn.loc)
        {
            store.claim_point(goal, claims, now);
            return true;
        }
    }
    // FindPathToNearbySniperPoint.
    let available = b.sensors.sniper_point.as_ref();
    if let Some(start) = store.spawn_point
        && let Some(path) = path_to_nearby_point(nav, claims, start, &mv, |p| {
            available.is_none_or(|s| s.is_point_available(p))
        })
    {
        store.claim_path(path, claims, now);
        return true;
    }
    // FindClosestPoint (the port's: of the points it could fly on from, and fly to
    // straight with room for its body). On Downfall the search from its spawn point
    // finds no point from 12 of the 20 air spawn points while the doors are locked
    // (each point beyond them taken: bBlocked), and the nearest free point a Sniper
    // there could see was TnSniperPoint_9893, which has no flights to or from it in
    // the game's map (no NavConnections either way): flown there straight, it could
    // never leave, every search for a point beside it finding none, and it clung
    // there the rest of the wave doing nothing but fire at a player who came into its
    // view. The next nearest, TnSniperPoint_14386, it could see but not fly to from
    // four of them straight: an overhang its line of sight passes under stops its
    // body, and it hung against it.
    let mut points: Vec<u32> = available.map(|s| s.available.clone()).unwrap_or_default();
    let at = ctx.pawn.loc;
    points.sort_by(|a, c| {
        let d = |p: u32| ue::to_ue(nav.points[p as usize].at).distance_squared(at);
        d(*a).total_cmp(&d(*c))
    });
    // (Its flight there measured as the air network's are, short of the point by
    // the room a Sniper leaves the line in for its approach to the wall.)
    let radius = flight_size(ctx.pawn).0;
    let fits = |p: u32| {
        let to = ue::to_ue(nav.points[p as usize].at);
        let way = to - at;
        let cut = crate::nav::PERCH_ROOM * ue::UU_PER_M;
        way.length() <= cut || ctx.view.flight_fits(at, to - way.normalize() * cut, radius)
    };
    for p in points {
        if claims.is_claimed_in(nav, p) || !nav.links_of(p).iter().any(|l| l.fly) {
            continue;
        }
        let loc = ue::to_ue(nav.points[p as usize].at);
        if !ctx.view.fast_trace(loc, at) || !fits(p) {
            continue;
        }
        store.claim_point(p, claims, now);
        return true;
    }
    false
}

/// FindPathToNearbySniperPoint: from `start` (unless it is blocked), each step the
/// first open connection (one it can take to a point not blocked, not on the path
/// yet, and if a sniper point, available), till a free sniper point; the path then
/// shortened (ShortenPath: a point skipped where the one before connects past it).
fn path_to_nearby_point(
    nav: &NavMesh,
    claims: &Claims,
    start: u32,
    mv: &Moves,
    available: impl Fn(u32) -> bool,
) -> Option<Vec<u32>> {
    let blocked = |p: u32| nav.points[p as usize].blocked || claims.is_claimed(p);
    if blocked(start) {
        return None;
    }
    let mut path = Vec::new();
    let mut point = Some(start);
    while let Some(p) = point {
        path.push(p);
        if nav.points[p as usize].kind == PointKind::Sniper && !claims.is_claimed_in(nav, p) {
            shorten_path(nav, &mut path);
            return Some(path);
        }
        point = nav.links_of(p).iter().find_map(|l| {
            let next = l.to;
            let open = mv.supports(l)
                && !blocked(next)
                && !path.contains(&next)
                && (nav.points[next as usize].kind != PointKind::Sniper || available(next));
            open.then_some(next)
        });
    }
    None
}

/// ShortenPath.
fn shorten_path(nav: &NavMesh, path: &mut Vec<u32>) {
    let mut i = 0;
    while i < path.len() {
        let mut j = i + 2;
        while j < path.len() {
            if nav.link_between(path[i], path[j]).is_some() {
                path.drain(i + 1..j);
                i = j;
                break;
            }
            j += 1;
        }
        i += 1;
    }
}

/// TnChooseNewSniperPointTowardsGoalOperator.OnStart: the next point on the way
/// from its point to its goal (none: WS_PathBlocked, and it fails).
pub fn choose_point_towards_goal(b: &mut Brain, ctx: &mut Ctx) -> bool {
    let Some(nav) = ctx.view.nav else {
        return false;
    };
    let now = ctx.view.now;
    let mv = moves(ctx.pawn, ctx.pawn.radius);
    let store = b.sensors.sniper_store.get_or_insert_default();
    let (Some(current), Some(goal)) = (store.current, store.goal) else {
        return false;
    };
    let claims = &mut ctx.shared.sniper_claims;
    // (TnSniperPoint(NavPath[1]): none if the next point is no sniper point.)
    let next = find_channel(nav, claims, current, goal, &mv)
        .and_then(|path| path.get(1).copied())
        .filter(|&n| nav.points[n as usize].kind == PointKind::Sniper);
    match next {
        Some(p) => {
            store.claim_point(p, claims, now);
            true
        }
        None => {
            b.core.set_ws_bool(htn::WS_PATH_BLOCKED, true);
            false
        }
    }
}

/// TnChooseNewSniperSearchPointOperator.OnStart: of the points round the one it is
/// on (free and available), the one longest unclaimed (none: WS_PathBlocked).
pub fn choose_search_point(b: &mut Brain, ctx: &mut Ctx) -> bool {
    let Some(nav) = ctx.view.nav else {
        return false;
    };
    let now = ctx.view.now;
    let Some(current) = b
        .sensors
        .sniper_store
        .as_ref()
        .and_then(|s| s.current)
        .and_then(|c| Point::get(nav, c))
    else {
        return false;
    };
    let claims = &mut ctx.shared.sniper_claims;
    let sensor = b.sensors.sniper_point.as_ref();
    let mut best: Option<u32> = None;
    for p in current.surrounding(nav, flight_radius(ctx.pawn)) {
        if claims.is_claimed_in(nav, p) || sensor.is_some_and(|s| !s.is_point_available(p)) {
            continue;
        }
        if let Some(s) = best
            && claims.time_since_last_claimed(p, now) < claims.time_since_last_claimed(s, now)
        {
            continue;
        }
        best = Some(p);
    }
    let store = b.sensors.sniper_store.get_or_insert_default();
    match best {
        Some(p) => {
            store.claim_point(p, claims, now);
            true
        }
        None => {
            b.core.set_ws_bool(htn::WS_PATH_BLOCKED, true);
            false
        }
    }
}

/// TnChooseNewSniperAttackPointOperator.OnStart: a point to fight its enemy from
/// (AttackRange: its vision distance), of those round the one it is on that it can
/// fly to (the next point on the way), else of every point available (straight
/// there); none: WS_AttackPointAvailable off, and it fails.
pub fn choose_attack_point(b: &mut Brain, ctx: &mut Ctx, task: &Task) -> bool {
    let Some(nav) = ctx.view.nav else {
        return false;
    };
    let now = ctx.view.now;
    let Some(current) = b
        .sensors
        .sniper_store
        .as_ref()
        .and_then(|s| s.current)
        .and_then(|c| Point::get(nav, c))
    else {
        return false;
    };
    let enemy_at = b.core.fetch(super::brain::LR_CONFIRMED_ENEMY);
    let range = task.int(0) as f32;
    let range_sq = range * range;
    let team = b.sensors.sniper_store.as_ref().map_or(0, |s| s.enemy_team);
    let mv = moves(ctx.pawn, ctx.pawn.radius);
    let radius = flight_radius(ctx.pawn);
    let claims = &ctx.shared.sniper_claims;
    let tactical = &ctx.shared.tactical;
    let sensor = b.sensors.sniper_point.as_ref();
    // IsBestPoint.
    let is_best = |p: &Point, best: Option<&Point>, check_availability: bool| -> bool {
        if claims.is_claimed_in(nav, p.id) {
            return false;
        }
        let los = nav
            .tactical_index(p.id)
            .is_some_and(|t| tactical.has_los_to_team(t, team));
        if !los {
            return false;
        }
        let d = (p.centre - enemy_at).length_squared();
        if d > range_sq {
            return false;
        }
        if check_availability && sensor.is_some_and(|s| !s.is_point_available(p.id)) {
            return false;
        }
        if let Some(best) = best
            && best.orientation() != current.orientation()
        {
            if claims.time_since_last_claimed(p.id, now)
                < claims.time_since_last_claimed(best.id, now)
            {
                return false;
            }
            if d > (best.centre - enemy_at).length_squared() {
                return false;
            }
        }
        true
    };
    // FindBestPoint over the points round it (available ones, straight there),
    // then over every point (by way of the first point on the path there: a sniper
    // point, or none).
    let find_best = |points: Vec<u32>, check_availability: bool, check_pathing: bool| {
        let mut next: Option<u32> = None;
        let mut best: Option<Point> = None;
        for id in points {
            let Some(p) = Point::get(nav, id) else {
                continue;
            };
            if !is_best(&p, best.as_ref(), check_availability) {
                continue;
            }
            if check_pathing {
                if let Some(path) = find_channel(nav, claims, current.id, id, &mv) {
                    best = Some(p);
                    next = path
                        .get(1)
                        .copied()
                        .filter(|&n| nav.points[n as usize].kind == PointKind::Sniper);
                }
            } else {
                best = Some(p);
                next = Some(id);
            }
        }
        next
    };
    let next = find_best(current.surrounding(nav, radius), true, false).or_else(|| {
        let all: Vec<u32> = sensor.map(|s| s.available.clone()).unwrap_or_default();
        find_best(all, false, true)
    });
    let Some(p) = next else {
        b.core.set_ws_bool(htn::WS_ATTACK_POINT_AVAILABLE, false);
        return false;
    };
    let store = b.sensors.sniper_store.get_or_insert_default();
    store.claim_point(p, &mut ctx.shared.sniper_claims, now);
    true
}

// ---------------------------------------------------------------------------
// Flying to a point
// ---------------------------------------------------------------------------

/// TnFlyToSniperPointOperator's state.
#[derive(Clone, Debug, Default)]
pub struct FlyTo {
    /// EFlyToSniperPointState: 0 to the next point, 1 to the approach, 2 the
    /// approach, 3 there.
    pub state: u8,
    pub destination: u32,
    pub begin_approach: f32,
    pub approach: Vec3,
    pub initial: Vec3,
    pub avoiding: bool,
    pub boosters: Option<(Arc<str>, Vec<Arc<str>>)>,
}

/// kMinPathingSpeed and kMaxPathingSpeed (uu/s), kAdditionalApproachDistance and
/// kReachedDestinationDistance (uu), kAvoidanceSpeed (uu/s).
const MIN_PATHING_SPEED: f32 = 3000.0;
const MAX_PATHING_SPEED: f32 = 5000.0;
const ADDITIONAL_APPROACH: f32 = 750.0;
const REACHED_DISTANCE: f32 = 750.0;
const AVOIDANCE_SPEED: f32 = 100.0;
/// kApproachAngle and kReachedDestinationAngle, as IsInRangeOfDestination takes
/// them (half of each the cosine's argument: in radians, though they are degrees).
const APPROACH_ANGLE: f32 = 160.0;
const REACHED_ANGLE: f32 = 180.0;
/// The path's pull towards the next point (AdjustDirectionToPath's radius).
const NEXT_POINT_PATH_RADIUS: f32 = 100.0;

impl FlyTo {
    /// OnStart: its destination's approach (MaxApproachDistance out from its middle,
    /// less its radius but no nearer than its radius; the approach begins 750 uu
    /// further out), turning to its focus, its speed along the way to its next
    /// point, flying (Acrobatics.Fly), its boosters lit; to its next point, or its
    /// destination's approach if that is next.
    pub fn start(&mut self, b: &mut Brain, ctx: &mut Ctx, task: &Task) -> bool {
        let Some(nav) = ctx.view.nav else {
            return false;
        };
        let Some(store) = b.sensors.sniper_store.as_ref() else {
            return false;
        };
        let (Some(dest), Some(next)) = (store.destination(), store.next_point()) else {
            return false;
        };
        let Some(d) = Point::get(nav, dest) else {
            return false;
        };
        let p = &mut *ctx.pawn;
        self.destination = dest;
        self.begin_approach = (d.max_approach - p.radius).max(p.radius);
        self.approach = d.centre + d.normal * self.begin_approach;
        self.begin_approach += ADDITIONAL_APPROACH;
        self.initial = p.loc;
        self.avoiding = false;
        let next_at = ue::to_ue(nav.points[next as usize].at);
        p.vel = ue::normal(next_at - p.loc) * p.vel.length();
        // Acrobatics.Fly (through the blocking volumes: pawn.rs's through_volumes).
        p.physics = Physics::Flying;
        p.through_volumes = true;
        b.set_behavior(ctx, BEHAVIOR_ROTATE_TO_FOCUS, false);
        // Its boosters (its task's BoosterEffects: its blueprint's).
        if task.int(0) == 1
            && let Some(template) = b
                .core
                .b
                .get("BoosterEffects.Template")
                .filter(|t| *t != "None")
        {
            let system: Arc<str> = Arc::from(template.rsplit('.').next().unwrap_or(template));
            let sockets: Vec<Arc<str>> = b
                .core
                .b
                .get("BoosterEffects.Sockets")
                .unwrap_or("")
                .split_whitespace()
                .map(Arc::from)
                .collect();
            for s in &sockets {
                let (at, axes) = ctx.pawn.socket_frame(s).unwrap_or_else(|| {
                    let (x, y, z) = super::weapon::axes(0.0, ctx.pawn.yaw);
                    (ctx.pawn.loc, [x, y, z])
                });
                ctx.shared.effects.push(super::shared::Effect {
                    by: b.core.entity,
                    system: system.clone(),
                    socket: s.clone(),
                    at,
                    axes,
                    offset: Vec3::ZERO,
                    turn: Vec3::ZERO,
                    attached: true,
                    only_once: false,
                    restart: false,
                    params: None,
                    auto_off: None,
                });
            }
            self.boosters = Some((system, sockets));
        }
        self.state = if next == dest { 1 } else { 0 };
        true
    }

    /// OnUpdate: its avoidance (back to its path), else its state's leg.
    pub fn update(&mut self, b: &mut Brain, ctx: &mut Ctx) -> Status {
        let Some(nav) = ctx.view.nav else {
            return Status::Failed;
        };
        let Some(d) = Point::get(nav, self.destination) else {
            return Status::Failed;
        };
        self.update_avoidance(b, ctx, nav, &d);
        if self.avoiding {
            return Status::Running;
        }
        match self.state {
            0 => self.fly_to_next_point(b, ctx, nav),
            1 => self.fly_to_destination(b, ctx, nav, &d),
            2 => self.approach_destination(b, ctx, nav, &d),
            _ => {}
        }
        if self.state == 3 {
            Status::Succeeded
        } else {
            Status::Running
        }
    }

    /// OnHandleEvent: a wall it hit (HitSurface: away from it) or a pawn it bumped
    /// (away from him), its path pulling it back.
    pub fn handle_event(&mut self, b: &mut Brain, ctx: &mut Ctx, e: &Event) {
        let away = match e.id {
            EventId::HitSurface => ctx.pawn.notes.hit_wall,
            EventId::Bump => e
                .actor
                .and_then(|a| ctx.view.actor(a))
                .map(|a| ue::normal(ctx.pawn.loc - a.loc)),
            _ => None,
        };
        if let (Some(dir), Some(nav)) = (away, ctx.view.nav)
            && let Some(d) = Point::get(nav, self.destination)
        {
            self.avoid_in_direction(b, ctx, nav, &d, dir);
        }
    }

    /// OnFinish: its boosters out (and the blocking volumes in its way again).
    pub fn finish(&mut self, b: &Brain, ctx: &mut Ctx) {
        ctx.pawn.through_volumes = false;
        if let Some((system, _)) = self.boosters.take() {
            ctx.shared.effects_stopped.push((b.core.entity, system));
        }
    }

    fn avoid_in_direction(
        &mut self,
        b: &Brain,
        ctx: &mut Ctx,
        nav: &NavMesh,
        d: &Point,
        dir: Vec3,
    ) {
        self.avoiding = true;
        let to_path = self.path_info(b, ctx.pawn, nav, d).1;
        let dir = ue::normal(dir + to_path);
        ctx.pawn.vel = dir * AVOIDANCE_SPEED;
        ctx.pawn.accel = dir;
    }

    fn update_avoidance(&mut self, b: &Brain, ctx: &mut Ctx, nav: &NavMesh, d: &Point) {
        if !self.avoiding {
            return;
        }
        let (_, to_path, distance) = self.path_info(b, ctx.pawn, nav, d);
        if distance <= PATHING_RADIUS {
            self.avoiding = false;
            return;
        }
        ctx.pawn.accel = to_path;
    }

    /// FlyToNextPoint: along its path to its next point, past which (its way to it
    /// and its last point's at right angles or more) the point is reached and its
    /// anchor; at its slowest; its destination's approach next once that is next.
    fn fly_to_next_point(&mut self, b: &mut Brain, ctx: &mut Ctx, nav: &NavMesh) {
        let Some(next) = b.sensors.sniper_store.as_ref().and_then(|s| s.next_point()) else {
            self.state = 1;
            return;
        };
        let next_at = ue::to_ue(nav.points[next as usize].at);
        let dir = ue::normal(next_at - ctx.pawn.loc);
        let dest = Point::get(nav, self.destination);
        let dir = dest.map_or(dir, |d| {
            self.adjust_to_path(b, ctx.pawn, nav, &d, dir, NEXT_POINT_PATH_RADIUS)
        });
        accelerate(ctx.pawn, dir);
        let store = b.sensors.sniper_store.get_or_insert_default();
        let anchor_at = store
            .anchor
            .map_or(Vec3::ZERO, |a| ue::to_ue(nav.points[a as usize].at));
        if (ctx.pawn.loc - next_at).dot(anchor_at - next_at) <= 0.0 {
            store.anchor = Some(next);
            store.point_reached(next);
        }
        set_max_speed(ctx.pawn, MIN_PATHING_SPEED);
        if store.next_point() == Some(self.destination) {
            self.state = 1;
        }
    }

    /// FlyToDestination: along its path to its destination's approach, faster the
    /// further it is from it; the approach once in range of it.
    fn fly_to_destination(&mut self, b: &mut Brain, ctx: &mut Ctx, nav: &NavMesh, d: &Point) {
        let dir = ue::normal(self.approach - ctx.pawn.loc);
        let dir = self.adjust_to_path(b, ctx.pawn, nav, d, dir, PATHING_RADIUS);
        accelerate(ctx.pawn, dir);
        let distance = (self.approach - ctx.pawn.loc).length();
        let pct =
            ue::clamp_pct_by_range(distance, self.begin_approach, self.begin_approach + 1000.0);
        set_max_speed(
            ctx.pawn,
            ue::lerp(MIN_PATHING_SPEED, MAX_PATHING_SPEED, pct),
        );
        if self.in_range(ctx.pawn, d, self.begin_approach, APPROACH_ANGLE) {
            self.state = 2;
        }
    }

    /// ApproachDestination: straight at its middle at its slowest, there once within
    /// 750 uu of it in front of it (out of range of the approach: back to it).
    fn approach_destination(&mut self, _b: &mut Brain, ctx: &mut Ctx, _nav: &NavMesh, d: &Point) {
        accelerate(ctx.pawn, ue::normal(d.centre - ctx.pawn.loc));
        set_max_speed(ctx.pawn, MIN_PATHING_SPEED);
        if self.in_range(ctx.pawn, d, REACHED_DISTANCE, REACHED_ANGLE) {
            self.state = 3;
        } else if !self.in_range(ctx.pawn, d, self.begin_approach, APPROACH_ANGLE) {
            self.state = 1;
        }
    }

    /// IsInRangeOfDestination: in front of its wall (within half the angle of its
    /// normal: the angle as the script takes it, its cosine of radians), further out
    /// than its radius, within `distance` of its middle.
    fn in_range(&self, p: &Pawn, d: &Point, distance: f32, angle: f32) -> bool {
        let to_pawn = ue::normal(p.loc - d.centre);
        if to_pawn.dot(d.normal) < (0.5 * angle).cos() {
            return false;
        }
        if (p.loc - d.centre).dot(d.normal) <= p.radius {
            return false;
        }
        (d.centre - p.loc).length() <= distance
    }

    /// AdjustDirectionToPath: `dir` and the way back to its path (as far off it as it
    /// is, over `radius`, at most all of it).
    fn adjust_to_path(
        &self,
        b: &Brain,
        p: &Pawn,
        nav: &NavMesh,
        d: &Point,
        dir: Vec3,
        radius: f32,
    ) -> Vec3 {
        let (_, to_path, distance) = self.path_info(b, p, nav, d);
        ue::normal(to_path * (distance / radius).clamp(0.0, 1.0) + dir)
    }

    /// GetPathInfo: the nearest point on its path's line (GetPathPoints: its
    /// approach's, from its destination's point to its middle; else its last point
    /// (or where it set off) to its next), the way there and how far.
    fn path_info(&self, b: &Brain, p: &Pawn, nav: &NavMesh, d: &Point) -> (Vec3, Vec3, f32) {
        let (begin, end) = if self.state == 2 {
            (d.location, d.centre)
        } else {
            let store = b.sensors.sniper_store.as_ref();
            let begin = store
                .and_then(|s| s.anchor)
                .map_or(self.initial, |a| ue::to_ue(nav.points[a as usize].at));
            let end = store
                .and_then(|s| s.next_point())
                .map_or(d.location, |n| ue::to_ue(nav.points[n as usize].at));
            (begin, end)
        };
        let line = ue::normal(end - begin);
        let closest = begin + line * (p.loc - begin).dot(line);
        let distance = (p.loc - closest).length();
        (closest, ue::normal(closest - p.loc), distance)
    }
}

/// AccelerateInDirection: turning to it, driven along it.
fn accelerate(p: &mut Pawn, dir: Vec3) {
    p.desired_yaw = ue::yaw_of(dir);
    p.accel = dir * p.accel_rate;
}

/// SetMaxSpeed: its air speed, and no faster now.
fn set_max_speed(p: &mut Pawn, max: f32) {
    p.air_speed = max;
    p.vel = p.vel.clamp_length_max(max);
}

// ---------------------------------------------------------------------------
// On the wall
// ---------------------------------------------------------------------------

/// TnAttachToWallOperator's state.
#[derive(Clone, Debug, Default)]
pub struct Attach {
    pub point: u32,
    pub location: (Vec3, Vec3),
    pub rotation: (Vec3, Vec3),
    pub ramp: f32,
    pub time: f32,
    pub attach_complete: bool,
    pub transform_complete: bool,
}

/// Its TransformComplete delegate's callback.
pub const CB_TRANSFORM_COMPLETE: u8 = 20;

impl Attach {
    /// OnStart: its insect form (Transform 1: it fails without the change), no
    /// behaviour, no physics; its move onto the point's middle (its perch) and its
    /// turn to the wall (forward along the wall: down it on a wall, the way it faces
    /// on a ceiling or a floor) over AdjustDuration.
    pub fn start(&mut self, b: &mut Brain, ctx: &mut Ctx, task: &Task, serial: u32) -> bool {
        let Some(nav) = ctx.view.nav else {
            return false;
        };
        let Some(store) = b.sensors.sniper_store.as_ref() else {
            return false;
        };
        let Some(point) = store.destination().and_then(|d| Point::get(nav, d)) else {
            return false;
        };
        let cb = super::anim::Callback {
            owner: serial,
            kind: CB_TRANSFORM_COMPLETE,
        };
        if !super::forms::transform(ctx.pawn, super::forms::ALT, Some(cb)) {
            return false;
        }
        b.set_behavior(ctx, 0, false);
        let p = &mut *ctx.pawn;
        p.physics = Physics::None;
        p.vel = Vec3::ZERO;
        p.accel = Vec3::ZERO;
        self.point = point.id;
        self.ramp = task.float(0);
        self.time = 0.0;
        // (Its insect's cylinder: the change has begun, its form's.)
        let caps = p.forms.as_ref().map(|f| f.caps[super::forms::ALT as usize]);
        let (r, h) = caps.map_or((p.radius, p.half_height), |c| (c.radius, c.half_height));
        self.location = (p.loc, point.perch(r, h));
        let forward = match point.orientation() {
            2 => -ue::vec_of(p.pitch, p.yaw),
            3 => ue::vec_of(p.pitch, p.yaw),
            _ => Vec3::new(0.0, 0.0, -1.0),
        };
        let right = ue::normal(point.normal.cross(forward));
        let forward = ue::normal(right.cross(point.normal));
        let to = ue::ortho_rotation(forward, right, point.normal);
        self.rotation = (Vec3::new(p.pitch, p.yaw, p.roll), to);
        self.attach_complete = false;
        self.transform_complete = false;
        true
    }

    /// OnUpdate: its move and turn (AdjustInteracter) until done, then crawling on
    /// the wall (PHYS_Spider on it, its point the one it is on); after that, its
    /// head and aim on its enemy. Done once on the wall in its insect form.
    pub fn update(&mut self, b: &mut Brain, ctx: &mut Ctx) -> Status {
        let Some(nav) = ctx.view.nav else {
            return Status::Failed;
        };
        let Some(point) = Point::get(nav, self.point) else {
            return Status::Failed;
        };
        if !self.attach_complete {
            self.time = (self.time + ctx.dt).clamp(0.0, self.ramp);
            let t = if self.ramp > 0.0 {
                self.time / self.ramp
            } else {
                0.0
            };
            let at = self
                .location
                .0
                .lerp(self.location.1, if self.ramp > 0.0 { t } else { 0.0 });
            let r = if self.ramp > 0.0 {
                ue::rlerp(self.rotation.0, self.rotation.1, t)
            } else {
                self.rotation.0
            };
            let p = &mut *ctx.pawn;
            p.loc = at;
            (p.pitch, p.yaw, p.roll) = (r.x, ue::wrap_rot(r.y), r.z);
            p.desired_yaw = p.yaw;
            self.attach_complete = self.time >= self.ramp;
            if self.attach_complete {
                set_spider(p, &point);
                b.sensors.sniper_store.get_or_insert_default().current = Some(self.point);
            }
        } else if let Some(e) = b.core.enemy {
            b.set_behavior(ctx, BEHAVIOR_HEAD_TRACKING | BEHAVIOR_AIMER, false);
            let at = super::ops::enemy_view_location(ctx, e);
            ctx.pawn.set_look_at_point(at);
        }
        if self.attach_complete && ctx.pawn.physics != Physics::Spider {
            set_spider(ctx.pawn, &point);
        }
        if self.attach_complete && self.transform_complete && ctx.pawn.physics == Physics::Spider {
            Status::Succeeded
        } else {
            Status::Running
        }
    }

    /// OnTransformComplete.
    pub fn transformed(&mut self) {
        self.transform_complete = true;
    }

    /// OnFinish: its point reached, its anchor.
    pub fn finish(&mut self, b: &mut Brain) {
        let store = b.sensors.sniper_store.get_or_insert_default();
        store.point_reached(self.point);
        store.anchor = Some(self.point);
    }
}

/// SetPhysics(PHYS_Spider, its wall, its normal) (bCrawler).
fn set_spider(p: &mut Pawn, point: &Point) {
    p.physics = Physics::Spider;
    p.spider_floor = point.normal;
    p.vel = Vec3::ZERO;
}

/// TnDetachFromWallOperator's state.
#[derive(Clone, Debug, Default)]
pub struct Detach {
    pub take_off_speed: f32,
    pub take_off_timer: f32,
    pub face_destination_duration: f32,
    pub rotation_percent: f32,
    pub take_off_rotation: Quat,
}

impl Detach {
    /// OnStart: no behaviour, its robot form (Transform 0: it fails without the
    /// change), no physics; its take-off's speed and time, and how long it turns to
    /// its next point after.
    pub fn start(&mut self, b: &mut Brain, ctx: &mut Ctx, task: &Task) -> bool {
        let ok = b
            .sensors
            .sniper_store
            .as_ref()
            .is_some_and(|s| s.destination().is_some() && s.current.is_some());
        if !ok {
            return false;
        }
        b.set_behavior(ctx, 0, false);
        if !super::forms::transform(ctx.pawn, super::forms::ROBOT, None) {
            return false;
        }
        let p = &mut *ctx.pawn;
        p.physics = Physics::None;
        p.vel = Vec3::ZERO;
        p.accel = Vec3::ZERO;
        self.take_off_speed = task.int(1) as f32;
        self.take_off_timer = task.float(2);
        self.face_destination_duration = task.float(3);
        self.rotation_percent = 0.0;
        true
    }

    /// OnUpdate: as its take-off time runs out, off the wall (falling, out along its
    /// normal at its take-off speed but from a ceiling); then turned upright to face
    /// its next point over its duration, done once facing it.
    pub fn update(&mut self, b: &mut Brain, ctx: &mut Ctx) -> Status {
        let Some(nav) = ctx.view.nav else {
            return Status::Failed;
        };
        let mut dt = ctx.dt;
        let store = b.sensors.sniper_store.get_or_insert_default();
        if self.take_off_timer > 0.0 {
            self.take_off_timer -= dt;
            if self.take_off_timer <= 0.0 {
                dt = -self.take_off_timer;
                let p = &mut *ctx.pawn;
                self.take_off_rotation = ue::quat_from_rotator(Vec3::new(p.pitch, p.yaw, p.roll));
                p.physics = Physics::Falling;
                if let Some(c) = store.current.and_then(|c| Point::get(nav, c))
                    && c.orientation() != 2
                {
                    p.vel = c.normal * self.take_off_speed;
                }
            }
        }
        if self.take_off_timer <= 0.0 {
            self.rotation_percent += dt / self.face_destination_duration.max(1e-3);
            let p = &mut *ctx.pawn;
            if let Some(next) = store.next_point() {
                let next_at = ue::to_ue(nav.points[next as usize].at);
                p.desired_yaw = ue::yaw_of(next_at - p.loc);
            }
            let upright = ue::quat_from_rotator(Vec3::new(0.0, p.desired_yaw, 0.0));
            let r = ue::quat_to_rotator(ue::quat_slerp(
                self.take_off_rotation,
                upright,
                self.rotation_percent,
            ));
            (p.pitch, p.yaw, p.roll) = (r.x, ue::wrap_rot(r.y), r.z);
            if self.rotation_percent >= 1.0 {
                p.pitch = 0.0;
                p.roll = 0.0;
                return Status::Succeeded;
            }
        }
        Status::Running
    }

    /// OnFinish: done, the point it was on released.
    pub fn finish(&mut self, b: &mut Brain, ctx: &mut Ctx, status: Status) {
        if status == Status::Succeeded
            && let Some(store) = b.sensors.sniper_store.as_mut()
        {
            store.release(&mut ctx.shared.sniper_claims, ctx.view.now);
        }
    }
}

/// TnCrawlOnSniperPointOperator's state.
#[derive(Clone, Debug, Default)]
pub struct Crawl {
    pub point: u32,
    pub destination: Vec3,
    pub stuck: f32,
}

/// kReachedDestinationRange, kStuckSpeed and kStuckDuration, kApproachDistance and
/// kApproachSpeed, kTakeOffOffset.
const CRAWL_REACHED: f32 = 25.0;
const CRAWL_STUCK_SPEED: f32 = 10.0;
const CRAWL_STUCK_SECS: f32 = 1.0;
const CRAWL_APPROACH: f32 = 200.0;
const CRAWL_APPROACH_SPEED: f32 = 100.0;
const CRAWL_TAKE_OFF_OFFSET: f32 = 100.0;

impl Crawl {
    /// OnStart: where on its point to crawl to (ESniperPointLocation: 0 its middle,
    /// 1 a random place for it, 2 above its middle, where it takes off from), on its
    /// wall's plane; moving and aiming (if it has a way to go), its gun tracking,
    /// looking at its suspected enemy; its ground speed the task's.
    pub fn start(&mut self, b: &mut Brain, ctx: &mut Ctx, task: &Task) -> bool {
        let Some(nav) = ctx.view.nav else {
            return false;
        };
        let Some(point) = b
            .sensors
            .sniper_store
            .as_ref()
            .and_then(|s| s.current)
            .and_then(|c| Point::get(nav, c))
        else {
            return false;
        };
        self.point = point.id;
        let p = &*ctx.pawn;
        let dest = match task.int(0) {
            1 => point.random_point_for(p.loc, p.radius, p.half_height, &mut b.core.rng),
            2 => point.centre + Vec3::Z * CRAWL_TAKE_OFF_OFFSET,
            _ => point.centre,
        };
        self.destination = on_plane(dest, point.centre, point.normal);
        self.stuck = 0.0;
        if (self.destination - on_plane(p.loc, point.centre, point.normal)).length() > CRAWL_REACHED
        {
            b.set_behavior(
                ctx,
                BEHAVIOR_ALLOW_MOTION | BEHAVIOR_AIMER | BEHAVIOR_HEAD_TRACKING,
                false,
            );
        }
        b.add_behavior(ctx, BEHAVIOR_WEAPON_TRACKING);
        let at = b.core.fetch(LR_SUSPECTED_ENEMY);
        ctx.pawn.set_look_at_point(at);
        ctx.pawn.ground_speed = task.int(1) as f32;
        true
    }

    /// OnUpdate: there (within 25 uu: stopped), stuck (under 10 uu/s for a second:
    /// WS_PathBlocked, it fails), else on along its wall, slowing to 100 uu/s over
    /// the last 200 uu.
    pub fn update(&mut self, b: &mut Brain, ctx: &mut Ctx) -> Status {
        let Some(point) = ctx.view.nav.and_then(|n| Point::get(n, self.point)) else {
            return Status::Failed;
        };
        let p = &mut *ctx.pawn;
        let to = self.destination - on_plane(p.loc, point.centre, point.normal);
        let distance = to.length();
        let dir = ue::normal(to);
        if distance <= CRAWL_REACHED {
            p.accel = Vec3::ZERO;
            p.vel = Vec3::ZERO;
            return Status::Succeeded;
        }
        let mut speed = p.vel.length();
        if speed < CRAWL_STUCK_SPEED {
            self.stuck += ctx.dt;
            if self.stuck >= CRAWL_STUCK_SECS {
                b.core.set_ws_bool(htn::WS_PATH_BLOCKED, true);
                return Status::Failed;
            }
        }
        if speed > CRAWL_APPROACH_SPEED {
            let pct = ue::clamp_pct_by_range(distance, CRAWL_REACHED, CRAWL_APPROACH);
            speed = ue::lerp(CRAWL_APPROACH_SPEED, speed, pct);
        }
        p.accel = dir * p.accel_rate;
        p.vel = dir * speed;
        Status::Running
    }

    /// OnFinish: stopped where it is on its wall, however the crawl ended. (A Sniper
    /// has no motion controller to take its acceleration off it, and nothing after
    /// a crawl on its wall drives it: a crawl cut short, as the alert every five
    /// seconds changed its plan, left it pushed along its wall for good, and up a
    /// wall to its take-off spot that carried it off the wall's top into the sky,
    /// still clinging, at up to its 75 m/s crawling speed.)
    pub fn finish(&mut self, ctx: &mut Ctx) {
        let p = &mut *ctx.pawn;
        p.accel = Vec3::ZERO;
        if p.physics == Physics::Spider {
            p.vel = Vec3::ZERO;
        }
    }
}

/// Core.Math.ClosestPointOnPlane.
fn on_plane(p: Vec3, origin: Vec3, normal: Vec3) -> Vec3 {
    p - normal * (p - origin).dot(normal)
}

// ---------------------------------------------------------------------------
// Its aim
// ---------------------------------------------------------------------------

/// TnSniperAimAnimator's kAimSpeed (FInterpTo's) and kAimRangeInRadians (its whole
/// aim each way: the script's 1.570796, a quarter turn).
const AIM_SPEED: f32 = 6.0;
const AIM_RANGE: f32 = std::f32::consts::FRAC_PI_2;

/// TnSniperAimAnimator.OnGetAimOffset: while it aims (its behaviour's aimer) on a
/// perch, where it aims (its base aim rotation: its aimer's way, its controller's
/// view point: TnHtnAiController.GetPlayerViewPoint) from its perch's wall: the
/// azimuth and elevation (GetAngularDistance) in the frame of the wall's normal
/// (X), its back (Z: less its facing) and Z cross X, over a quarter turn; else
/// none (kNoAim); eased in from its last (FInterpTo at kAimSpeed).
pub fn aim_offset(b: &Brain, p: &Pawn, nav: Option<&NavMesh>, last: Vec2, dt: f32) -> Vec2 {
    let point = b
        .sensors
        .sniper_store
        .as_ref()
        .and_then(|s| s.current)
        .zip(nav)
        .and_then(|(id, nav)| Point::get(nav, id));
    let want = match point {
        Some(pt) if b.core.behavior & BEHAVIOR_AIMER != 0 => {
            let facing = ue::rotator_axes(p.pitch, p.yaw, p.roll)[0];
            let x = pt.normal;
            let z = -facing;
            let y = z.cross(x);
            let aim = if p.aimer.active {
                super::weapon::aim_direction(p, p.muzzle)
            } else {
                facing
            };
            ue::angular_distance(aim, x, y, z) / AIM_RANGE
        }
        _ => Vec2::ZERO,
    };
    Vec2::new(
        ue::finterp_to(last.x, want.x, dt, AIM_SPEED),
        ue::finterp_to(last.y, want.y, dt, AIM_SPEED),
    )
}
