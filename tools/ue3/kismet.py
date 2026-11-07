"""The state a level's Kismet puts its actors in, read from a FoC map package.

Escalation maps open their areas with Kismet: a team pays at a console
(TnInteractCurrencyReceptacle), whose "Minigame Success" plays Matinees (SeqAct_Interp)
that slide doors away, lower barriers and so on; other Matinees play as gameplay starts.
The demo has every area open, so build_map.py places the actors those Matinees move
where the Matinees leave them, and leaves out what the level hides as it starts.

    hidden_at_start(pkg)  -> export indices of the actors hidden as gameplay starts
    opened_state(pkg)     -> {actor export index: Move or Anim} after every Matinee that
                             a console, a remote event or the start of gameplay plays,
                             evaluated at its end (with the animation sets an Anim
                             comes from)
    transformed(pkg)      -> {actor export index: True (transformed) or False}: the
                             transforming actors (doors) those events transform or
                             untransform (TnSeqAct_TransformWorldProp; Ancients' doors
                             open this way)
"""
import math
import struct
from collections import namedtuple

import numpy as np

from props import read_props
from ue3pkg import Reader
import layout as lay

# A Matinee's effect on an actor: a move relative to where it starts
# (IMF_RelativeToInitial: offset in the actor's own frame, then a rotation in
# degrees: roll, pitch, yaw), or an animation pose (sequence name, time in seconds).
Move = namedtuple("Move", "offset euler relative")
Anim = namedtuple("Anim", "sequence time")

# Kismet ops whose outputs are followed from these roots: events the demo treats as
# having happened (every console bought, every remote event fired, gameplay started).
ROOTS = ("TnSeqEvent_Interact", "SeqEvent_RemoteEvent", "SeqEvent_GameplayStarted", "SeqEvent_LevelLoaded")
START = ("SeqEvent_GameplayStarted", "SeqEvent_LevelLoaded")


def _clean(k):
    return all(c.isalnum() or c in "_[]" for c in k)


def _props(pkg, idx, maxoff=16):
    """An object's tagged properties (the longest clean parse from the start)."""
    e = pkg.exports[idx - 1]
    best = {}
    for st in range(0, maxoff):
        try:
            r = Reader(pkg.data, e["offset"] + st)
            pr = read_props(pkg, r, e["offset"] + e["size"])
        except Exception:
            continue
        if pr and all(_clean(k) for k in pr) and len(pr) > len(best):
            best = pr
    return best


def _objs(v):
    d = v[1]
    n = struct.unpack_from("<i", d)[0]
    return list(struct.unpack_from(f"<{n}i", d, 4))


def _structs(pkg, v):
    try:
        return lay.read_struct_array(pkg, v[1])
    except Exception:
        return []


def _vec(v):
    return np.array(struct.unpack("<3f", v[2][:12]), np.float64)


class Graph:
    """A package's Kismet: ops with their output links and variable links."""

    def __init__(self, pkg):
        self.pkg = pkg
        self.ops = {}
        for i, e in enumerate(pkg.exports, 1):
            c = pkg.class_name(e)
            if not (c.startswith(("Seq", "TnSeq")) or c in ("TnInteractActionSequence",)):
                continue
            if c.startswith("SeqVar") or c in ("Sequence", "SequenceFrame"):
                continue
            pr = _props(pkg, i)
            outs = []
            for el in _structs(pkg, pr.get("OutputLinks")) if "OutputLinks" in pr else []:
                links = []
                for link in _structs(pkg, el["Links"]) if "Links" in el else []:
                    op = link.get("LinkedOp")
                    if op and op[1]:
                        links.append((op[1], link.get("InputLinkIdx", 0)))
                outs.append(links)
            varl = {}
            for el in _structs(pkg, pr.get("VariableLinks")) if "VariableLinks" in pr else []:
                desc = el.get("LinkDesc")
                name = desc[1].decode("latin-1", "replace") if isinstance(desc, tuple) and isinstance(desc[1], bytes) else str(desc)
                name = "".join(ch for ch in name if ch.isprintable()).strip()
                if "LinkedVariables" in el:
                    varl[name] = _objs(el["LinkedVariables"])
            self.ops[i] = {"class": c, "props": pr, "outs": outs, "vars": varl}

    def objects(self, var):
        """The actors a variable holds (SeqVar_Object, SeqVar_ObjectList), or the
        object linked directly."""
        pkg = self.pkg
        if var <= 0:
            return []
        c = pkg.class_name(pkg.exports[var - 1])
        if c == "SeqVar_Object":
            v = _props(pkg, var).get("ObjValue")
            return [v[1]] if isinstance(v, tuple) and v[0] == "obj" and v[1] else []
        if c == "SeqVar_ObjectList":
            v = _props(pkg, var).get("ObjList")
            return [x for x in _objs(v) if x] if isinstance(v, tuple) and v[0] == "array" else []
        if c.startswith("SeqVar"):
            return []
        return [var]

    def reached(self, roots):
        """(op, input index) pairs reached from ops of the root classes, following
        output links through any op."""
        seen, todo = set(), [(i, None) for i, op in self.ops.items() if op["class"] in roots]
        hits = set()
        while todo:
            i, inp = todo.pop()
            if (i, inp) in seen:
                continue
            seen.add((i, inp))
            if inp is not None:
                hits.add((i, inp))
            op = self.ops.get(i)
            if op is None:
                continue
            for links in op["outs"]:
                for target, k in links:
                    todo.append((target, k))
        return hits


def hidden_at_start(pkg, graph=None):
    """Actors a SeqAct_ToggleHidden hides (its Hide input) as gameplay starts."""
    g = graph or Graph(pkg)
    out = set()
    for i, inp in g.reached(START):
        op = g.ops.get(i)
        if op and op["class"] == "SeqAct_ToggleHidden" and inp == 0:
            for vars_ in op["vars"].values():
                for v in vars_:
                    out.update(g.objects(v))
    return out


def _curve_points(pkg, curve):
    """An InterpCurveVector's points: [(time, value, arrive, leave, mode)]."""
    if not (isinstance(curve, tuple) and curve[0] == "struct"):
        return []
    sub = read_props(pkg, Reader(curve[2]), len(curve[2]))
    pts = []
    for el in _structs(pkg, sub["Points"]) if "Points" in sub else []:
        pts.append((el.get("InVal", 0.0), _vec(el["OutVal"]) if "OutVal" in el else np.zeros(3),
                    _vec(el["ArriveTangent"]) if "ArriveTangent" in el else np.zeros(3),
                    _vec(el["LeaveTangent"]) if "LeaveTangent" in el else np.zeros(3),
                    el.get("InterpMode", "CIM_CurveAuto")))
    return pts


def eval_curve(points, t):
    """An InterpCurveVector at time t (Hermite between keys, as UE3's curves)."""
    if not points:
        return np.zeros(3)
    if t <= points[0][0]:
        return points[0][1]
    if t >= points[-1][0]:
        return points[-1][1]
    for (t0, v0, _a0, l0, mode), (t1, v1, a1, _l1, _m) in zip(points, points[1:]):
        if t0 <= t <= t1:
            span = t1 - t0
            s = (t - t0) / span if span > 0 else 0.0
            if mode == "CIM_Constant":
                return v0
            if mode == "CIM_Linear":
                return v0 + (v1 - v0) * s
            h00, h10 = 2 * s ** 3 - 3 * s ** 2 + 1, s ** 3 - 2 * s ** 2 + s
            h01, h11 = -2 * s ** 3 + 3 * s ** 2, s ** 3 - s ** 2
            return h00 * v0 + h10 * span * l0 + h01 * v1 + h11 * span * a1
    return points[-1][1]


def matinee(pkg, g, interp):
    """A SeqAct_Interp's groups: {group name: [tracks]} and its length. Tracks are
    ("move", pos points, euler points, relative), ("anim", keys, the group's animation
    sets), ("event", keys)."""
    op = g.ops[interp]
    data = None
    for vars_ in op["vars"].values():
        for v in vars_:
            if v > 0 and pkg.class_name(pkg.exports[v - 1]) == "InterpData":
                data = v
    if data is None:
        return {}, 0.0
    dpr = _props(pkg, data)
    length = dpr.get("InterpLength", 5.0)  # UInterpData's default
    groups = {}
    for gi in _objs(dpr["InterpGroups"]) if "InterpGroups" in dpr else []:
        gpr = _props(pkg, gi)
        name = str(gpr.get("GroupName", ""))
        # The animation sets the group's anim tracks take their sequences from.
        sets = [pkg.full_name(x) for x in _objs(gpr["GroupAnimSets"]) if x] if "GroupAnimSets" in gpr else []
        tracks = []
        for ti in _objs(gpr["InterpTracks"]) if "InterpTracks" in gpr else []:
            tc = pkg.class_name(pkg.exports[ti - 1])
            tpr = _props(pkg, ti)
            if tc == "InterpTrackMove":
                tracks.append(("move", _curve_points(pkg, tpr.get("PosTrack")), _curve_points(pkg, tpr.get("EulerTrack")),
                               tpr.get("MoveFrame", "IMF_World") == "IMF_RelativeToInitial"))
            elif tc == "InterpTrackAnimControl":
                tracks.append(("anim", _structs(pkg, tpr["AnimSeqs"]) if "AnimSeqs" in tpr else [], sets))
            elif tc == "InterpTrackEvent":
                tracks.append(("event", _structs(pkg, tpr["EventTrack"]) if "EventTrack" in tpr else []))
        groups[name] = tracks
    return groups, length


def _group_actors(g, op, name):
    """The actors bound to a Matinee group: the SeqAct_Interp's variable link of the
    group's name."""
    for desc, vars_ in op["vars"].items():
        if desc.strip().lower() == name.strip().lower():
            out = []
            for v in vars_:
                out.extend(g.objects(v))
            return out
    return []


def opened_state(pkg, graph=None):
    """{actor: Move or Anim}: where the Matinees played by the consoles, remote events
    and the start of gameplay leave the actors (each Matinee evaluated at its end).
    An animated actor's Anim is keyed ("anim", actor), and the animation sets its
    Matinee group plays it from ("animsets", actor): [full names]."""
    g = graph or Graph(pkg)
    played = {i for i, inp in g.reached(ROOTS) if inp == 0 and g.ops.get(i, {}).get("class") == "SeqAct_Interp"}
    out = {}
    for interp in sorted(played):
        groups, length = matinee(pkg, g, interp)
        t = length
        for name, tracks in groups.items():
            actors = _group_actors(g, g.ops[interp], name)
            for track in tracks:
                if track[0] == "move":
                    _, pos, euler, relative = track
                    if not pos and not euler:
                        continue
                    m = Move(eval_curve(pos, t), eval_curve(euler, t), relative)
                    if np.allclose(m.offset, 0) and np.allclose(m.euler, 0) and relative:
                        continue
                    for a in actors:
                        out[a] = m
                elif track[0] == "anim":
                    keys = [k for k in track[1] if k.get("StartTime", 0.0) <= t]
                    if not keys:
                        continue
                    k = keys[-1]
                    run = (t - k.get("StartTime", 0.0)) * k.get("AnimPlayRate", 1.0)
                    anim = Anim(str(k.get("AnimSeqName")), (k.get("AnimStartOffset", 0.0), k.get("AnimEndOffset", 0.0),
                                                           run, bool(k.get("bReverse")), bool(k.get("bLooping"))))
                    for a in actors:
                        out.setdefault(("anim", a), anim)
                        if track[2]:
                            out.setdefault(("animsets", a), track[2])
    return out


def transformed(pkg, graph=None):
    """{actor: bool}: the TnTransformingWorldActors that the consoles, remote events
    and the start of gameplay transform (TnSeqAct_TransformWorldProp's "Transform"
    input, 1: True) or untransform ("Untransform", 0: False); transforming wins."""
    g = graph or Graph(pkg)
    out = {}
    for i, inp in sorted(g.reached(ROOTS), key=lambda x: (x[0], -x[1])):
        op = g.ops.get(i)
        if not op or op["class"] != "TnSeqAct_TransformWorldProp" or inp not in (0, 1):
            continue
        for vars_ in op["vars"].values():
            for v in vars_:
                for a in g.objects(v):
                    out[a] = out.get(a, False) or inp == 1
    return out


def anim_time(anim, length):
    """Seconds into a clip of `length` for an Anim's (start offset, end offset, run,
    reverse, looping)."""
    start, end, run, reverse, looping = anim.time
    span = max(length - start - end, 0.0)
    if looping and span > 0:
        run = math.fmod(run, span)
    run = min(max(run, 0.0), span)
    return (length - end - run) if reverse else (start + run)


def euler_matrix(euler):
    """InterpTrackMove's EulerTrack value (roll, pitch, yaw in degrees) as a row-vector
    rotation matrix (Unreal axes)."""
    roll, pitch, yaw = euler
    k = 65536.0 / 360.0
    return lay.rotator_matrix(int(round(pitch * k)), int(round(yaw * k)), int(round(roll * k)))[:3, :3]
