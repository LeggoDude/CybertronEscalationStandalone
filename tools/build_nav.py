"""Build an Escalation map's navigation for the enemies' AI: assets/levels/<map>_nav.txt.

Fall of Cybertron's AI walks a navigation mesh of its own (High Moon's, not UE3's
pylons): NavigationCell actors, each a triangle (its three _Edges: Start, End, an
outward Normal in the ground plane and the Neighbor cell across it), whose
NavConnections join it to the cells next to it and to the PathNodes that start and
end its jumps (a PathNode's own connections: bWalkable to its cell, bJumpable, with
HighJump for a high one, to the PathNode it lands at). The air units fly between
AirPathNodes and the Snipers' TnSniperPoints (bFlyable). A NavigationCell's
_RegisteredActors are the TnTacticalPathNodes on it: the positions the soldiers
fight from, each with a TnTactical (its _CoverType: TC_LeftCorner, TC_RightCorner,
TC_LowLeftCorner, TC_LowRightCorner, TC_Ledge, or none for an open one; its
_Occlusion, a 32-bit mask of the directions round it that cover hides it from,
segment i centred on (i + 0.5) x 2 pi / 32 in the ground plane; its _ClaimGroup,
which nodes too close to be held at once share).

Pathing.FindChannelToPoint (native) finds a channel through these points;
PortalBuilder and StringPulling (HM_Engine, script) turn it into the corners a robot
walks to: src/nav.rs does both, as the game's script does.

A point's bBlocked keeps the paths out of it: each map's Kismet blocks the cells and
jump nodes behind its locked doors when gameplay starts (SeqAct_Toggle Turn Off)
and opens them when a purchase's door has finished transforming (Turn On); the
toggle lines below are those, from tools/doors_kismet.py's offline run.

<map>_nav.txt (tab-separated; Bevy metres: (ue x, ue z, ue y) x 0.01):
  point  <i> <kind> <name> <x y z> <yaw> <anchor> <blocked 0/1>
         every navigation point, cells first (kind: cell, path, air, sniper,
         tactical); yaw in radians (the demo's heading); anchor: the cell it stands
         on (its StaticAnchor), -1 for none
  cell   <i> <ax ay az> <bx by bz> <cx cy cz> <n0> <n1> <n2> <normal xyz> <tactical 0/1>
         a cell's corners (its edges' Starts: edge k runs from corner k to corner
         k + 1) and the cell across each edge (-1 for none)
  link   <from> <to> <cost m> <end xyz> <radius m> <height m> <walk> <jump> <fly> <high>
         a NavConnection (cost: its Distance; end: its EndLocation; the biggest
         pawn that fits: radius, height)
  tac    <i> <cover> <occlusion> <claim group>   a tactical node's TnTactical
         (cover: none, low, left, right, lowleft, lowright, ledge)
  holds  <cell> <i...>                           a cell's tactical nodes
  sniper <i> <centre xyz> <extent xyz> <wall normal xyz> <wall valid 0/1> <max approach m> <up xyz>
         a TnSniperPoint's wall (its normal as converted: a hundredth of a unit
         long), how far out from it a Sniper may begin its approach, and its
         Rotation's Z axis (a unit vector: the wall's axes are the normal, up
         crossed with the normal, and up: GetWallNormalAxes)
  toggle <when> <seconds> <on|off|toggle> <i...>
         Kismet blocking (off) or opening (on) points: when gameplay starts
         ("start") or a console's purchase (its name), seconds after it
  leapvol <v> <name> <triangles>
         a TnLeaperSmashVolume (Ignition's one): the floor a Leaper may leap
         onto its enemy across (TnLeaperSmashVolumeSensor: it and the place it
         lands both inside it), its brush's surface in the leaptri lines after it
  leaptri <v> <ax ay az> <bx by bz> <cx cy cz>
         a triangle of volume v's surface (its BSP's polygons, placed by the
         volume; wound to face out of it, as their planes do), for its
         EncompassesPoint (src/nav.rs: the triangles' winding number round a point)

usage: python build_nav.py <CookedPC> <foc_esc/assets> [map...]
"""
import math
import os
import struct
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.path.join(HERE, "ue3"))
sys.path.insert(0, HERE)

import class_data as cd  # noqa: E402
import doors_kismet as dk  # noqa: E402
from build_map import _clean, actor_props as _any_props  # noqa: E402
from ue3pkg import Reader  # noqa: E402
from props import read_props  # noqa: E402


def actor_props(pkg, e, maxoff=120):
    """A navigation point's tagged properties: build_map's longest clean parse, but
    one that has the actor's Location first (an AirPathNode's NavConnections hold
    more clean-looking properties than the actor's own list, which a parse starting
    inside them finds)."""
    best = None
    for st in range(0, maxoff):
        try:
            pr = read_props(pkg, Reader(pkg.data, e["offset"] + st), e["offset"] + e["size"])
        except Exception:
            continue
        if pr and "Location" in pr and all(_clean(k) for k in pr):
            if best is None or len(pr) > len(best):
                best = pr
    return best or _any_props(pkg, e, maxoff)
from enemy_data import MAPS  # noqa: E402
from ue3pkg import Package  # noqa: E402

KINDS = {
    "NavigationCell": "cell",
    "PathNode": "path",
    "AirPathNode": "air",
    "TnSniperPoint": "sniper",
    "TnTacticalPathNode": "tactical",
    # (Escalation's air spawn points are AirPathNodes on the air network: a
    # Sniper's spawn point, the start of its first flight.)
    "TnSurvivalAirSpawnPoint": "air",
}
COVER = {
    "TC_None": "none",
    "TC_Low": "low",
    "TC_LeftCorner": "left",
    "TC_RightCorner": "right",
    "TC_LowLeftCorner": "lowleft",
    "TC_LowRightCorner": "lowright",
    "TC_Ledge": "ledge",
}


def conv(v):
    return (v[0] * 0.01, v[2] * 0.01, v[1] * 0.01)


def heading(yaw):
    t = yaw * 2 * math.pi / 65536.0
    return math.atan2(-math.cos(t), -math.sin(t))


def f(x):
    return "%.4f" % x if isinstance(x, float) else str(x)


def v3(v):
    return " ".join("%.4f" % x for x in v)


def nav_ref(p, v):
    """A NavReference's point (its Nav), as an export index."""
    sp = cd._struct_props(p, v)
    return cd._obj(sp.get("Nav"))


def read_map(cooked, package):
    p = Package(os.path.join(cooked, package))
    names = {}
    order = []
    for kind_class in KINDS:
        for i, e in enumerate(p.exports, 1):
            if p.class_name(e) != kind_class or not p.full_name(i).startswith("TheWorld.PersistentLevel."):
                continue
            order.append(i)
    index = {x: k for k, x in enumerate(order)}
    props = {}
    for i in order:
        props[i] = actor_props(p, p.exports[i - 1])
        names[i] = p.full_name(i).split(".")[-1]

    lines = []
    links = []
    holds = []
    for i in order:
        pr = props[i]
        kind = KINDS[p.class_name(p.exports[i - 1])]
        at = conv(cd._vec(pr.get("Location")))
        yaw = heading(cd._rot(pr.get("Rotation"))[1])
        anchor = index.get(nav_ref(p, pr.get("StaticAnchor")), -1) if "StaticAnchor" in pr else -1
        if kind == "cell":
            anchor = index[i]
        blocked = 1 if pr.get("bBlocked") else 0
        lines.append(["point", index[i], kind, names[i], v3(at), "%.5f" % yaw, anchor, blocked])
        if kind == "cell":
            edges = cd._structs(p, pr.get("_Edges"))
            if len(edges) != 3:
                raise SystemExit("%s: %d edges" % (names[i], len(edges)))
            corners = [conv(cd._vec(e.get("Start"))) for e in edges]
            ends = [conv(cd._vec(e.get("End"))) for e in edges]
            for k in range(3):
                if max(abs(ends[k][j] - corners[(k + 1) % 3][j]) for j in range(3)) > 1e-3:
                    raise SystemExit("%s: edge %d doesn't end where the next starts" % (names[i], k))
            nbs = [index.get(nav_ref(p, e.get("Neighbor")), -1) for e in edges]
            normal = cd._vec(pr.get("_Normal"), (0.0, 0.0, 1.0))
            lines.append(["cell", index[i]] + [v3(c) for c in corners] + nbs
                         + [v3((normal[0], normal[2], normal[1])), 1 if pr.get("_IsTacticalCell") else 0])
            held = [index[x] for x in cd._objs(pr.get("_RegisteredActors")) if x in index]
            if held:
                holds.append(["holds", index[i]] + held)
        if kind == "tactical":
            t = cd._obj(pr.get("Tactical"))
            tp = cd._props(p, t) if t > 0 else {}
            cover = COVER.get(str(tp.get("_CoverType", "TC_None")), "none")
            lines.append(["tac", index[i], cover, int(tp.get("_Occlusion", 0)), int(tp.get("_ClaimGroup", 0))])
        if kind == "sniper":
            # (Its _MaxApproachDistance is saved only where the level cut it short
            # of TnSniperPoint's 1000: RecalculateMaxApproachDistance.)
            pitch, yaw_u, roll = (x * math.tau / 65536.0 for x in cd._rot(pr.get("Rotation")))
            sp, cp = math.sin(pitch), math.cos(pitch)
            sy, cy = math.sin(yaw_u), math.cos(yaw_u)
            sr, cr = math.sin(roll), math.cos(roll)
            up = (-(cr * sp * cy + sr * sy), cy * sr - cr * sp * sy, cr * cp)
            lines.append(["sniper", index[i], v3(conv(cd._vec(pr.get("_CenterLocation")))),
                          v3(conv(cd._vec(pr.get("_SafeLocationExtent")))),
                          v3(conv(cd._vec(pr.get("_WallNormal")))), 1 if pr.get("_IsWallValid") else 0,
                          f(float(pr.get("_MaxApproachDistance", 1000.0)) * 0.01),
                          v3((up[0], up[2], up[1]))])
        for c in cd._structs(p, pr.get("NavConnections")):
            to = index.get(nav_ref(p, c.get("End")), -1)
            if to < 0:
                continue
            links.append(["link", index[i], to, f(c.get("Distance", 0) * 0.01), v3(conv(cd._vec(c.get("EndLocation")))),
                          f(c.get("CollisionRadius", 0) * 0.01), f(c.get("CollisionHeight", 0) * 0.01),
                          1 if c.get("bWalkable") else 0, 1 if c.get("bJumpable") else 0,
                          1 if c.get("bFlyable") else 0, 1 if c.get("HighJump") else 0])

    # The Kismet's toggles of the points.
    p.path = os.path.join(cooked, package)
    level = dk.read_level(p)
    data = dk.interpret(level, {"nodes": []})
    by_name = {names[i]: index[i] for i in order}
    toggles = []
    timelines = [("start", data["start"] if isinstance(data["start"], list) else data["start"].get("timeline", []))]
    timelines += [(c["id"], c["timeline"]) for c in data["consoles"]]
    for when, log in timelines:
        for e in log:
            if e["cls"] != "SeqAct_Toggle" or not e["do"].startswith("toggle "):
                continue
            verb = {"Turn On": "on", "Turn Off": "off", "Toggle": "toggle"}.get(e["do"][7:])
            ids = [by_name[x] for x in e.get("targets") or [] if x in by_name]
            if verb and ids:
                toggles.append(["toggle", when, f(float(e["t"])), verb] + ids)
    return lines + links + holds + toggles + leap_volumes(p, p.path), len(order)


def leap_volumes(p, path):
    """The level's TnLeaperSmashVolumes: each its leapvol line and its surface's
    leaptri lines (its brush's BSP polygons, fanned into triangles and placed by
    the volume actor; each wound so that it faces the way its node's plane does,
    out of the volume)."""
    import numpy as np
    import bsp
    from build_map import actor_matrix
    rows = []
    v = 0
    for i, e in enumerate(p.exports, 1):
        if p.class_name(e) != "Model" or e["outer"] <= 0:
            continue
        owner = p.exports[e["outer"] - 1]
        if p.class_name(owner) != "TnLeaperSmashVolume":
            continue
        M = actor_matrix(actor_props(p, owner))
        _, _, points, nodes, _, verts = bsp.extract(path, pkg=p, index=i)
        tris = []
        for vpool, _, nv, plane in nodes:
            if nv < 3:
                continue
            idx = verts[vpool:vpool + nv]
            if idx.max() >= len(points):
                continue
            P = np.concatenate([points[idx].astype(np.float64), np.ones((nv, 1))], 1) @ M
            n = np.array(plane[:3], dtype=np.float64) @ M[:3, :3]
            for k in range(1, nv - 1):
                a, b, c = P[0, :3], P[k, :3], P[k + 1, :3]
                if np.dot(np.cross(b - a, c - a), n) < 0:
                    b, c = c, b
                tris.append((a, b, c))
        rows.append(["leapvol", v, p.full_name(e["outer"]).split(".")[-1], len(tris)])
        # (conv swaps y and z, which turns a triangle over: wound back the other way
        # to face out in the demo's axes too.)
        for a, b, c in tris:
            rows.append(["leaptri", v] + [v3(conv(x)) for x in (a, c, b)])
        v += 1
    return rows


def main():
    cooked, assets = sys.argv[1], sys.argv[2]
    maps = sys.argv[3:] or list(MAPS)
    for m in maps:
        rows, n = read_map(cooked, MAPS[m])
        out = os.path.join(assets, "levels", m + "_nav.txt")
        os.makedirs(os.path.dirname(out), exist_ok=True)
        with open(out, "w") as fh:
            fh.write("# %s navigation (tools/build_nav.py): see its header\n" % (m.capitalize() + ("'" if m.endswith("s") else "'s")))
            for r in rows:
                fh.write("\t".join(str(x) for x in r) + "\n")
        print(m, n, "points", len(rows), "lines ->", out)


if __name__ == "__main__":
    main()
