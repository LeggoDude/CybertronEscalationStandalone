"""What an Escalation map's BASE package says about its locked doors, for
tools/build_doors.py: its unlock consoles, its Kismet, the Matinees its purchases play
and the actors those move, read from the package and then run offline.

    read_level(pkg)        -> the package's consoles, Kismet ops, Matinees, actors,
                              spawn clusters, spawn points and attachments (dict)
    interpret(level, glb)  -> each console's purchase as a timeline (an offline run of
                              the Kismet from its "Minigame Success", the state of
                              gameplay start carried in), gameplay start's own, the
                              Matinees they play (their moves in world space), and
                              the actors involved, each matched to the node of the
                              map's model (models/<map>.glb) that draws it

Positions are Bevy metres ((ue x, ue z, ue y) x 0.01), rotations quaternions in Bevy
axes (P R P, P swapping y and z), as tools/build_map.py places the maps' meshes.

The offline run models UE3's SeqAct_Interp as the game runs it: Play resumes from
where it is (a finished Matinee that doesn't loop starts again from 0), Pause toggles,
Reverse plays back, event keys fire their outputs as they are passed and Completed
fires at the end; SeqAct_Delay waits its Duration (1 s if none); Toggles and
outline toggles fire Out at once; TnSeqAct_TransformWorldProp's Finished fires after
its longest target's animation; remote events are followed. Random switches and the
pawn factories' outputs are not.
"""
import heapq
import json
import math
import struct
import sys
import os

import numpy as np

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.path.join(HERE, "ue3"))
import anim as AN  # noqa: E402
import kismet as K  # noqa: E402
import layout as lay  # noqa: E402
from props import read_props  # noqa: E402
from ue3pkg import Reader  # noqa: E402

P = np.array([[1, 0, 0], [0, 0, 1], [0, 1, 0]], float)


# ---------------------------------------------------------------------------
# Reading the package
# ---------------------------------------------------------------------------

def read_level(p):
    """The consoles, Kismet, Matinees and actors of a BASE package (Package)."""

    def N(i):
        return p.full_name(i).split(".")[-1] if i > 0 else (p.obj_name(i) if i else None)

    def props(i):
        return K._props(p, i)

    def vec(v):
        if isinstance(v, tuple) and v[0] == "struct":
            return [round(x, 3) for x in struct.unpack("<3f", v[2][:12])]
        return None

    def rot(v):
        if isinstance(v, tuple) and v[0] == "struct":
            return list(struct.unpack("<3i", v[2][:12]))
        return None

    def objname(v):
        if isinstance(v, tuple) and v[0] == "obj":
            return N(v[1]) if v[1] > 0 else (p.full_name(v[1]) if v[1] < 0 else None)
        return None

    def objfull(v):
        if isinstance(v, tuple) and v[0] == "obj" and v[1]:
            return p.full_name(v[1])
        return None

    def scal(v):
        if isinstance(v, tuple):
            if v[0] == "obj":
                return objname(v)
            if v[0] == "struct":
                if v[1] == "Vector":
                    return vec(v)
                if v[1] == "Rotator":
                    return rot(v)
                return v[1]
            if v[0] == "array":
                return "array"
            if len(v) > 1 and isinstance(v[1], bytes):
                return v[1][4:].decode("latin-1", "replace").rstrip("\x00")
            return str(v)
        return v

    def cls_of(i):
        return p.class_name(p.exports[i - 1]) if i > 0 else None

    def float_points(curve):
        # An InterpCurveFloat's points: [(time, value, interp mode, arrive, leave)].
        if not (isinstance(curve, tuple) and curve[0] == "struct"):
            return []
        try:
            sub = read_props(p, Reader(curve[2]), len(curve[2]))
        except Exception:
            return []
        pts = []
        for el in K._structs(p, sub["Points"]) if "Points" in sub else []:
            num = lambda k: round(float(el.get(k, 0.0) or 0.0), 4)  # noqa: E731
            pts.append((num("InVal"), num("OutVal"), str(scal(el.get("InterpMode", "CIM_CurveAuto"))),
                        num("ArriveTangent"), num("LeaveTangent")))
        return pts

    actors = {}

    def actor_props(i):
        # (The longest clean parse, preferring one with a Location: an actor's.)
        e = p.exports[i - 1]
        best, bestloc = {}, {}
        for st in range(0, 41):
            try:
                r = Reader(p.data, e["offset"] + st)
                pr = read_props(p, r, e["offset"] + e["size"])
            except Exception:
                continue
            if pr and all(K._clean(k) for k in pr):
                if "Location" in pr and len(pr) > len(bestloc):
                    bestloc = pr
                if len(pr) > len(best):
                    best = pr
        return bestloc or best

    def actor(i):
        if i in actors:
            return actors[i]
        pr = actor_props(i)
        a = {"name": N(i), "class": cls_of(i), "index": i}
        for k in ("Location", "Rotation", "DrawScale", "DrawScale3D", "Cost", "Increment",
                  "LocalizedCommandDescription", "DataStoreOverride", "Cooldown", "Tag", "bHidden",
                  "bCollideActors", "bBlockActors", "InitialTransformState", "Base", "bHardAttach",
                  "BaseBoneName", "PrePivot",
                  # (A pain-causing volume's: PhysicsVolume's, and Volume's bAffectPlayer.)
                  "DamagePerSec", "DamageType", "bPainCausing", "bAffectPlayer", "bAffectsLivingPawns",
                  # (A line of sound's: HmAmbientSoundLineEmitter's.)
                  "LineLength"):
            if k in pr:
                v = pr[k]
                if k == "Location":
                    a[k] = vec(v)
                elif k == "Rotation":
                    a[k] = rot(v)
                elif k == "DrawScale3D":
                    a[k] = vec(v)
                elif k == "DamageType":
                    a[k] = objfull(v)
                else:
                    a[k] = scal(v)
        for ck in ("StaticMeshComponent", "SkeletalMeshComponent", "BaseMesh", "Mesh", "CollisionComponent",
                   "BrushComponent", "ParticleSystemComponent", "LightComponent"):
            v = pr.get(ck)
            if isinstance(v, tuple) and v[0] == "obj" and v[1] > 0:
                cpr = actor_props(v[1])
                sub = {"index": v[1]}
                for kk in ("StaticMesh", "SkeletalMesh", "Materials", "Scale", "Scale3D", "Translation", "Rotation",
                           "CollideActors", "BlockActors", "BlockRigidBody", "HiddenGame", "AnimSets",
                           # (An emitter's system, whether the game lights it, and a light's.)
                           "Template", "bAutoActivate", "bAcceptsLights", "LightEnvironment",
                           "Radius", "Brightness", "LightColor", "bEnabled", "FalloffExponent"):
                    if kk in cpr:
                        vv = cpr[kk]
                        if kk in ("Materials", "AnimSets"):
                            try:
                                sub[kk] = [p.full_name(x).split(".")[-1] if x > 0 else p.full_name(x)
                                           for x in K._objs(vv) if x]
                            except Exception:
                                sub[kk] = []
                        elif kk in ("StaticMesh", "SkeletalMesh", "Template"):
                            sub[kk] = objfull(vv)
                        elif kk == "LightColor" and isinstance(vv, tuple) and vv[0] == "struct":
                            b, g_, r_, al = vv[2][:4]
                            sub[kk] = [r_, g_, b, al]
                        else:
                            sub[kk] = scal(vv)
                an = cpr.get("Animations")
                if isinstance(an, tuple) and an[0] == "obj" and an[1] > 0:
                    apr = actor_props(an[1])
                    sub["AnimNode"] = {k: scal(apr[k]) for k in ("AnimSeqName", "Rate", "bPlaying", "bLooping",
                                                                 "CurrentTime") if k in apr}
                a[ck + "_"] = sub
        actors[i] = a
        return a

    out = {"consoles": [], "lootboxes": [], "matinees": {}}
    for i, e in enumerate(p.exports, 1):
        c = p.class_name(e)
        if not p.full_name(i).startswith("TheWorld."):
            continue
        if c in ("TnInteractCurrencyReceptacle", "TnInteractCurrencyReceptacleMultiUse"):
            out["consoles"].append(actor(i))
        elif c == "TnInteractLootBox":
            out["lootboxes"].append(actor(i))

    # The Kismet ops: their outputs (links and delays), inputs and variable links.
    def desc_of(el, key="LinkDesc"):
        d = el.get(key)
        if isinstance(d, tuple) and len(d) > 1 and isinstance(d[1], bytes):
            s = d[1][4:].decode("latin-1", "replace")
        else:
            s = str(d)
        return "".join(ch for ch in s if ch.isprintable()).strip().strip("'")

    ops = {}
    for i, e in enumerate(p.exports, 1):
        c = p.class_name(e)
        if not c.startswith(("Seq", "TnSeq")) or c.startswith("SeqVar") or c in ("Sequence", "SequenceFrame"):
            continue
        pr = props(i)
        outs = []
        for el in K._structs(p, pr.get("OutputLinks")) if "OutputLinks" in pr else []:
            links = []
            for link in K._structs(p, el["Links"]) if "Links" in el else []:
                op = link.get("LinkedOp")
                if op and op[1]:
                    links.append((op[1], link.get("InputLinkIdx", 0)))
            outs.append((desc_of(el), links, el.get("ActivateDelay", 0.0)))
        ins = [desc_of(el) for el in K._structs(p, pr.get("InputLinks"))] if "InputLinks" in pr else []
        varl = []
        for el in K._structs(p, pr.get("VariableLinks")) if "VariableLinks" in pr else []:
            vs = K._objs(el["LinkedVariables"]) if "LinkedVariables" in el else []
            varl.append((desc_of(el), vs))
        ops[i] = dict(cls=c, pr=pr, outs=outs, ins=ins, vars=varl)

    def var_objs(v):
        if v <= 0:
            return []
        c = cls_of(v)
        if c == "SeqVar_Object":
            ov = props(v).get("ObjValue")
            return [ov[1]] if isinstance(ov, tuple) and ov[0] == "obj" and ov[1] > 0 else []
        if c == "SeqVar_ObjectList":
            ov = props(v).get("ObjList")
            return [x for x in K._objs(ov) if x > 0] if isinstance(ov, tuple) and ov[0] == "array" else []
        if c and c.startswith("SeqVar"):
            return []
        return [v]

    def var_val(v):
        c = cls_of(v)
        pr = props(v)
        if c == "SeqVar_Float":
            return pr.get("FloatValue", 0.0)
        if c == "SeqVar_Int":
            return pr.get("IntValue", 0)
        if c == "SeqVar_Bool":
            return pr.get("bValue", 0)
        return None

    def matinee_info(i):
        if N(i) in out["matinees"]:
            return
        o = ops[i]
        data = None
        binds = {}
        for d, vs in o["vars"]:
            objs = []
            for v in vs:
                if v > 0 and cls_of(v) == "InterpData":
                    data = v
                else:
                    objs += var_objs(v)
            if d != "Data":
                binds[d] = [N(x) for x in objs]
            for x in objs:
                actor(x)
        info = {"binds": binds,
                "props": {k: scal(v) for k, v in o["pr"].items()
                          if k in ("bLooping", "PlayRate", "bRewindOnPlay", "bNoResetOnRewind", "bForceStartPos",
                                   "ForceStartPosition", "ObjComment")}}
        if data:
            dpr = props(data)
            info["length"] = dpr.get("InterpLength", 5.0)
            groups = []
            for gi in K._objs(dpr["InterpGroups"]) if "InterpGroups" in dpr else []:
                gpr = props(gi)
                g = {"name": str(scal(gpr.get("GroupName", ""))), "tracks": []}
                if "GroupAnimSets" in gpr:
                    g["animsets"] = [p.full_name(x) for x in K._objs(gpr["GroupAnimSets"]) if x]
                for ti in K._objs(gpr["InterpTracks"]) if "InterpTracks" in gpr else []:
                    tc = cls_of(ti)
                    tpr = props(ti)
                    t = {"class": tc}
                    if tc == "InterpTrackMove":
                        t["pos"] = [(round(k[0], 4), [round(x, 3) for x in k[1]], k[4], [round(x, 3) for x in k[2]],
                                     [round(x, 3) for x in k[3]]) for k in K._curve_points(p, tpr.get("PosTrack"))]
                        t["euler"] = [(round(k[0], 4), [round(x, 3) for x in k[1]], k[4],
                                       [round(x, 3) for x in k[2]], [round(x, 3) for x in k[3]])
                                      for k in K._curve_points(p, tpr.get("EulerTrack"))]
                        t["frame"] = scal(tpr.get("MoveFrame", "IMF_World"))
                    elif tc == "InterpTrackSound":
                        t["keys"] = [{"time": el.get("Time", 0.0), "sound": objfull(el.get("Sound")),
                                      "volume": el.get("Volume", 1.0), "pitch": el.get("Pitch", 1.0)}
                                     for el in (K._structs(p, tpr.get("Sounds")) if "Sounds" in tpr else [])]
                        for k in ("bContinueSoundOnMatineeEnd", "bPlayOnReverse"):
                            if k in tpr:
                                t[k] = scal(tpr[k])
                    elif tc == "InterpTrackAnimControl":
                        t["keys"] = [{k: scal(v) for k, v in el.items()}
                                     for el in K._structs(p, tpr.get("AnimSeqs"))] if "AnimSeqs" in tpr else []
                    elif tc == "InterpTrackEvent":
                        t["keys"] = [{k: scal(v) for k, v in el.items()}
                                     for el in K._structs(p, tpr.get("EventTrack"))] if "EventTrack" in tpr else []
                    elif tc == "InterpTrackFloatProp":
                        t["prop"] = str(scal(tpr.get("PropertyName", "")) or "")
                        t["keys"] = float_points(tpr.get("FloatTrack"))
                    elif tc == "InterpTrackToggle":
                        # (Its keys: an emitter or light turned on, off or toggled.)
                        t["keys"] = [{"Time": el.get("Time", 0.0), "ToggleAction": scal(el.get("ToggleAction"))}
                                     for el in (K._structs(p, tpr.get("ToggleTrack")) if "ToggleTrack" in tpr
                                                else [])]
                    g["tracks"].append(t)
                groups.append(g)
            info["groups"] = groups
        out["matinees"][N(i)] = info

    gops = {}
    for i, o in ops.items():
        vs = []
        for d, vl in o["vars"]:
            names, vals = [], []
            for v in vl:
                if v <= 0:
                    continue
                if cls_of(v) == "InterpData":
                    continue
                ob = var_objs(v)
                names += [N(x) for x in ob]
                for x in ob:
                    if cls_of(x) not in ("NavigationCell", "PathNode", "TnSniperPoint", "TnSpawnClusterSurvival",
                                         "TnTacticalPathNode"):
                        actor(x)
                vv = var_val(v)
                if vv is not None:
                    vals.append(vv)
            vs.append([d, names, vals])
        gops[N(i)] = {"cls": o["cls"],
                      # (A sound by its full name: its package's, as the cues are named.)
                      "props": {k: (objfull(v) if k == "PlaySound" else scal(v)) for k, v in o["pr"].items()
                                if k not in ("OutputLinks", "VariableLinks", "InputLinks", "EventLinks", "ObjPosX",
                                             "ObjPosY", "DrawWidth", "DrawHeight", "ParentSequence", "ObjName",
                                             "MaxWidth")},
                      "outs": [[d, [[N(t), k] for t, k in links], delay] for d, links, delay in o["outs"]],
                      "ins": o["ins"], "vars": vs,
                      # (The variables themselves, by name: what a factory's Spawned and
                      # an action's targets share, filled as the level runs.)
                      "rawvars": [[d, [N(v) for v in vl if v > 0 and cls_of(v) != "InterpData"]]
                                  for d, vl in o["vars"]]}
        # A factory's sentry (TnActorFactorySentryAI's Sentry: its TnSentry archetype)
        # and side.
        if o["cls"] in ("SeqAct_TnPawnFactory", "SeqAct_ActorFactory"):
            fac = o["pr"].get("Factory")
            if isinstance(fac, tuple) and fac[0] == "obj" and fac[1] > 0:
                fpr = props(fac[1])
                gops[N(i)]["factory"] = {"class": cls_of(fac[1]), "Sentry": objfull(fpr.get("Sentry")),
                                         "FactionTeam": objfull(fpr.get("FactionTeam"))}
        if o["cls"] == "SeqAct_Interp":
            matinee_info(i)
    out["ops"] = gops

    # Spawn clusters and their spawn points, and every survival spawn point (in
    # package order, as build_map.py's points file lists them).
    clusters = {}
    for i, e in enumerate(p.exports, 1):
        if p.class_name(e) != "TnSpawnClusterSurvival" or not p.full_name(i).startswith("TheWorld."):
            continue
        pr = actor_props(i)
        pts = []
        sp = pr.get("SpawnPoints")
        for x in (K._objs(sp) if isinstance(sp, tuple) and sp[0] == "array" else []):
            if x > 0:
                pts.append([N(x), vec(actor_props(x).get("Location"))])
        clusters[N(i)] = {"points": pts}
    out["clusters"] = clusters
    out["spawnpoints"] = [[N(i), vec(actor_props(i).get("Location"))] for i, e in enumerate(p.exports, 1)
                          if p.class_name(e) == "TnSurvivalSpawnPoint" and p.full_name(i).startswith("TheWorld.")]

    # The level's actors that ride on another (Base), hard-attached or not.
    attached = {}
    lvl = {i for i, e in enumerate(p.exports, 1) if p.class_name(e) == "Level"}
    for i, e in enumerate(p.exports, 1):
        if e["outer"] not in lvl:
            continue
        if p.class_name(e) in ("NavigationCell", "PathNode", "TnSniperPoint", "TnTacticalPathNode", "CoverLink",
                               "CoverSlotMarker"):
            continue
        pr = actor_props(i)
        b = pr.get("Base")
        if isinstance(b, tuple) and b[0] == "obj" and b[1] > 0:
            attached[N(i)] = {"base": N(b[1]), "hard": bool(pr.get("bHardAttach")),
                              "bone": scal(pr.get("BaseBoneName")) if "BaseBoneName" in pr else None}
            actor(i)
    out["attached"] = attached

    # The lengths of the sequences the doors and Matinees play.
    lens = {}

    def anim_len(aset, seq):
        key = aset + "." + seq
        if key not in lens:
            try:
                _, pr, _ = AN._sequence(p, aset.split(".")[-1], seq)
                lens[key] = {"length": pr.get("SequenceLength"), "frames": pr.get("NumFrames")}
            except Exception as ex:
                lens[key] = {"error": str(ex)}
        return lens[key]

    for a in actors.values():
        for ck in ("SkeletalMeshComponent_", "CollisionComponent_"):
            c = a.get(ck)
            if c and c.get("AnimNode") and c.get("AnimSets"):
                for aset in c["AnimSets"]:
                    anim_len(aset, c["AnimNode"].get("AnimSeqName", ""))
    for m in out["matinees"].values():
        for g in m.get("groups", []):
            for t in g["tracks"]:
                if t["class"] == "InterpTrackAnimControl":
                    for k in t.get("keys", []):
                        for aset in g.get("animsets") or []:
                            anim_len(aset, str(k.get("AnimSeqName")))
    out["anim_lengths"] = lens
    out["actors"] = {v["name"]: v for v in actors.values()}
    return out


# ---------------------------------------------------------------------------
# Conversions
# ---------------------------------------------------------------------------

def bevy_pos(v):
    return [round(v[0] * 0.01, 4), round(v[2] * 0.01, 4), round(v[1] * 0.01, 4)]


def ue_rot(r):
    return lay.rotator_matrix(*(r or [0, 0, 0]))[:3, :3]


def quat_from_matrix(m):
    """(x, y, z, w) of a column-vector rotation matrix."""
    t = m[0, 0] + m[1, 1] + m[2, 2]
    if t > 0:
        s = math.sqrt(t + 1.0) * 2
        w, x, y, z = 0.25 * s, (m[2, 1] - m[1, 2]) / s, (m[0, 2] - m[2, 0]) / s, (m[1, 0] - m[0, 1]) / s
    elif m[0, 0] > m[1, 1] and m[0, 0] > m[2, 2]:
        s = math.sqrt(1.0 + m[0, 0] - m[1, 1] - m[2, 2]) * 2
        w, x, y, z = (m[2, 1] - m[1, 2]) / s, 0.25 * s, (m[0, 1] + m[1, 0]) / s, (m[0, 2] + m[2, 0]) / s
    elif m[1, 1] > m[2, 2]:
        s = math.sqrt(1.0 + m[1, 1] - m[0, 0] - m[2, 2]) * 2
        w, x, y, z = (m[0, 2] - m[2, 0]) / s, (m[0, 1] + m[1, 0]) / s, 0.25 * s, (m[1, 2] + m[2, 1]) / s
    else:
        s = math.sqrt(1.0 + m[2, 2] - m[0, 0] - m[1, 1]) * 2
        w, x, y, z = (m[1, 0] - m[0, 1]) / s, (m[0, 2] + m[2, 0]) / s, (m[1, 2] + m[2, 1]) / s, 0.25 * s
    return [round(x, 6), round(y, 6), round(z, 6), round(w, 6)]


def bevy_transform(a):
    """An actor's place: {translation, rotation (x, y, z, w), scale}, as build_map.py's
    glTF places it."""
    R = ue_rot(a.get("Rotation"))
    ds = a.get("DrawScale", 1.0) or 1.0
    s3 = a.get("DrawScale3D") or [1.0, 1.0, 1.0]
    return {"translation": bevy_pos(a.get("Location") or [0, 0, 0]),
            "rotation": quat_from_matrix((P @ R @ P).T),
            "scale": [round(ds * s3[0], 5), round(ds * s3[2], 5), round(ds * s3[1], 5)]}


def mesh_of(a):
    for ck in ("StaticMeshComponent_", "SkeletalMeshComponent_", "BaseMesh_", "CollisionComponent_"):
        c = a.get(ck) or {}
        m = c.get("StaticMesh") or c.get("SkeletalMesh")
        if m:
            return m, c
    return None, {}


# ---------------------------------------------------------------------------
# The offline Kismet run
# ---------------------------------------------------------------------------

class Run:
    """One run of the level's Kismet from an event, recording what it does when (a
    timeline of {t, op, cls, do, ...})."""

    def __init__(self, level, state=None):
        self.ops, self.mats = level["ops"], level["matinees"]
        self.level = level
        self.q, self.n, self.log = [], 0, []
        self.mat = {} if state is None else json.loads(json.dumps(state["mat"]))
        self.gate = {} if state is None else dict(state["gate"])
        self.gen, self.delay_gen = {}, {}

    def push(self, t, kind, *args):
        self.n += 1
        heapq.heappush(self.q, (round(t, 6), self.n, kind, args))

    def fire(self, t, op, out_name):
        o = self.ops.get(op)
        if not o:
            return
        for name, links, delay in o["outs"]:
            if name == out_name:
                for tgt, k in links:
                    self.push(t + (delay or 0.0), "in", tgt, k)

    def var(self, o, name):
        for d_, names, vals in o["vars"]:
            if d_ == name:
                return names, vals
        return [], []

    def targets(self, o):
        out = []
        for d_, names, _vals in o["vars"]:
            if d_ in ("Target", "Targets"):
                out += names
        return out

    def rec(self, t, op, what, **kw):
        e = {"t": round(t, 4), "op": op, "cls": self.ops[op]["cls"], "do": what}
        e.update(kw)
        self.log.append(e)

    def mstate(self, op):
        return self.mat.setdefault(op, {"pos": 0.0, "playing": False, "dir": 1, "paused": False, "t0": 0.0, "p0": 0.0})

    def length(self, op):
        return float((self.mats.get(op) or {}).get("length") or 0.0)

    def rate(self, op):
        return float((self.mats.get(op) or {}).get("props", {}).get("PlayRate", 1.0) or 1.0)

    def mpos(self, op, t):
        s = self.mstate(op)
        if s["playing"] and not s["paused"]:
            p = s["p0"] + s["dir"] * (t - s["t0"]) * self.rate(op)
            return min(max(p, 0.0), self.length(op))
        return s["pos"]

    def events(self, op):
        ev = []
        for g in (self.mats.get(op) or {}).get("groups", []):
            for t in g["tracks"]:
                if t["class"] == "InterpTrackEvent":
                    for k in t.get("keys", []):
                        ev.append((float(k.get("Time", 0.0)), str(k.get("EventName"))))
        return sorted(ev)

    def mschedule(self, op, t):
        s = self.mstate(op)
        g = self.gen.get(op, 0) + 1
        self.gen[op] = g
        if not s["playing"] or s["paused"]:
            return
        rate, L, p = self.rate(op), self.length(op), s["pos"]
        for et, name in self.events(op):
            if s["dir"] > 0 and p < et <= L:
                self.push(t + (et - p) / rate, "mevent", op, g, name)
            if s["dir"] < 0 and 0 <= et < p:
                self.push(t + (p - et) / rate, "mevent", op, g, name)
        end = (L - p) / rate if s["dir"] > 0 else p / rate
        self.push(t + end, "mend", op, g)

    def run(self, until=90.0, cap=4000):
        steps = 0
        while self.q and steps < cap:
            t, _, kind, args = heapq.heappop(self.q)
            if t > until:
                break
            steps += 1
            if kind == "in":
                self.input(t, *args)
            elif kind == "mevent":
                op, g, name = args
                if self.gen.get(op) == g:
                    self.rec(t, op, "matinee event", event=name)
                    self.fire(t, op, name)
            elif kind == "mend":
                op, g = args
                if self.gen.get(op) != g:
                    continue
                s = self.mstate(op)
                L = self.length(op)
                loop = bool((self.mats.get(op) or {}).get("props", {}).get("bLooping"))
                if s["dir"] > 0:
                    if loop:
                        s.update(pos=0.0, t0=t, p0=0.0)
                        self.rec(t, op, "matinee loops")
                        self.mschedule(op, t)
                        continue
                    s.update(pos=L, playing=False)
                    self.rec(t, op, "matinee completed")
                    self.fire(t, op, "Completed")
                else:
                    s.update(pos=0.0, playing=False)
                    self.rec(t, op, "matinee reversed to start")
                    self.fire(t, op, "Reversed")
            elif kind == "delay":
                op, g = args
                if self.delay_gen.get(op) == g:
                    self.rec(t, op, "delay finished")
                    self.fire(t, op, "Finished")
            elif kind == "finish":
                (op,) = args
                self.rec(t, op, "transform finished")
                self.fire(t, op, "Finished")
        return self

    def anim_length(self, actor_name):
        a = self.level["actors"].get(actor_name, {})
        for ck in ("SkeletalMeshComponent_", "CollisionComponent_"):
            c = a.get(ck) or {}
            node = c.get("AnimNode") or {}
            seq = node.get("AnimSeqName")
            for aset in c.get("AnimSets") or []:
                for key, v in self.level["anim_lengths"].items():
                    if key.split(".")[-2:] == [aset.split(".")[-1], seq] and "length" in v:
                        return v["length"] / abs(node.get("Rate", 1.0) or 1.0), seq
        return None, None

    def input(self, t, op, k):
        o = self.ops.get(op)
        if o is None:
            return
        c = o["cls"]
        iname = o["ins"][k] if k < len(o["ins"]) else str(k)
        pr = o["props"]
        if c == "SeqAct_Interp":
            s = self.mstate(op)
            s["pos"] = self.mpos(op, t)
            L = self.length(op)
            if iname == "Play":
                if s["playing"] and s["dir"] > 0 and not s["paused"]:
                    return
                if not s["playing"] and s["pos"] >= L - 1e-6:
                    s["pos"] = 0.0
                s.update(playing=True, dir=1, paused=False, t0=t, p0=s["pos"])
                self.rec(t, op, "matinee play", from_pos=round(s["pos"], 4), length=L)
                self.mschedule(op, t)
            elif iname == "Reverse":
                s.update(playing=True, dir=-1, paused=False, t0=t, p0=s["pos"])
                self.rec(t, op, "matinee reverse", from_pos=round(s["pos"], 4), length=L)
                self.mschedule(op, t)
            elif iname == "Pause":
                if s["playing"]:
                    s["paused"] = not s["paused"]
                    s["t0"], s["p0"] = t, s["pos"]
                    self.rec(t, op, "matinee pause" if s["paused"] else "matinee resume", at_pos=round(s["pos"], 4))
                    self.mschedule(op, t)
            elif iname == "Stop":
                s.update(playing=False, paused=False)
                self.rec(t, op, "matinee stop", at_pos=round(s["pos"], 4))
                self.mschedule(op, t)
            return
        if c in ("SeqAct_Toggle", "TnSeqAct_ToggleMaterialOutline"):
            self.rec(t, op, ("outline " if c != "SeqAct_Toggle" else "toggle ") + iname, targets=self.targets(o))
            self.fire(t, op, "Out")
            return
        if c == "SeqAct_ToggleHidden":
            self.rec(t, op, "hidden " + iname, targets=self.targets(o))
            self.fire(t, op, "Out")
            return
        if c == "SeqAct_Delay":
            _names, vals = self.var(o, "Duration")
            dur = vals[0] if vals else pr.get("Duration", 1.0)
            if iname == "Start":
                g = self.delay_gen.get(op, 0) + 1
                self.delay_gen[op] = g
                self.push(t + float(dur), "delay", op, g)
            elif iname == "Stop":
                self.delay_gen[op] = self.delay_gen.get(op, 0) + 1
            return
        if c == "SeqAct_ActivateRemoteEvent":
            name = pr.get("EventName")
            self.rec(t, op, "remote event", event=name)
            for n2, o2 in self.ops.items():
                if (o2["cls"] == "SeqEvent_RemoteEvent" and o2["props"].get("EventName") == name
                        and o2["props"].get("bEnabled", True) is not False):
                    self.fire(t, n2, "Out")
            self.fire(t, op, "Out")
            return
        if c == "TnSeqAct_TransformWorldProp":
            tg = self.targets(o)
            longest, info = 0.0, []
            for a in tg:
                L, seq = self.anim_length(a)
                info.append({"actor": a, "seconds": L, "anim": seq})
                longest = max(longest, L or 0.0)
            self.rec(t, op, "transform " + iname, targets=tg, anims=info)
            self.push(t + longest, "finish", op)
            return
        if c == "SeqAct_Gate":
            open_ = self.gate.get(op, pr.get("bOpen", True))
            if iname == "In":
                if open_:
                    self.fire(t, op, "Out")
            elif iname == "Open":
                self.gate[op] = True
            elif iname == "Close":
                self.gate[op] = False
            elif iname == "Toggle":
                self.gate[op] = not open_
            return
        if c == "SeqAct_PlaySound":
            self.rec(t, op, "sound " + iname, sound=pr.get("PlaySound"), at=self.targets(o))
            self.fire(t, op, "Out")
            return
        if c == "TnSeqAct_PlayAnnouncerDialog":
            self.rec(t, op, "announcer", cue=pr.get("DialogCue"))
            self.fire(t, op, "Out")
            return
        if c == "SeqAct_RandomSwitch":
            return
        # The sentries' factories (each spawns one, which the level then sets
        # dormant and wakes: the trap sentries), what attaches one to the gunship, and
        # what sets them dormant or wakes them (TnAiSentryController.SetDormant), the
        # sentries by their factories.
        if c in ("SeqAct_TnPawnFactory", "SeqAct_ActorFactory"):
            if iname == "Spawn Actor":
                self.rec(t, op, "spawn", at=self.var(o, "Spawn Point")[0])
                for out_name in ("Spawned", "Finished", "All Pawns Spawned"):
                    self.fire(t, op, out_name)
            return
        if c == "SeqAct_AttachToActor":
            self.rec(t, op, "attach", targets=self.targets(o), sentries=self.sentries(o, "Attachment"))
            self.fire(t, op, "Out")
            return
        if c == "SeqAct_TnAiSentrySetDormant":
            self.rec(t, op, "dormant" if iname == "Dormant" else "awake", sentries=self.sentries(o, "Target"))
            self.fire(t, op, "Out")
            return
        self.rec(t, op, c, input=iname, targets=self.targets(o))
        self.fire(t, op, "Out")

    def sentries(self, o, link):
        """The sentries an op's variable link `link` holds as the level runs: the
        factories whose Spawned variable it is."""
        out = []
        for d_, raw in o.get("rawvars") or []:
            if d_ != link:
                continue
            for v in raw:
                for n, o2 in self.ops.items():
                    if o2["cls"] not in ("SeqAct_TnPawnFactory", "SeqAct_ActorFactory"):
                        continue
                    if any(d2 == "Spawned" and v in raw2 for d2, raw2 in o2.get("rawvars") or []):
                        out.append(n)
        return sorted(set(out))


# ---------------------------------------------------------------------------
# The doors, from the run
# ---------------------------------------------------------------------------

def glb_nodes(g):
    """The map model's nodes that draw something: (index, name, translation)."""
    out = []
    for i, nd in enumerate(g["nodes"]):
        if "mesh" not in nd:
            continue
        if "matrix" in nd:
            m = nd["matrix"]
            out.append((i, nd.get("name", ""), [m[12], m[13], m[14]]))
        else:
            out.append((i, nd.get("name", ""), nd.get("translation", [0, 0, 0])))
    return out


def glb_match(nodes, mesh, positions):
    """The node drawing this mesh nearest any of the positions (within 0.5 m)."""
    short = mesh.split(".")[-1]
    best = None
    for i, name, t in nodes:
        if name.split("@")[0] != short:
            continue
        for label, p in positions:
            d = math.dist(t, p)
            if d < 0.5 and (best is None or d < best[0]):
                best = (d, i, name, label)
    return None if best is None else {"node": best[1], "name": best[2], "state": best[3]}


def matinee_tracks(level, mname):
    """A Matinee's groups, its move tracks turned into world-space offsets (Bevy
    metres) per bound actor: [[t, offset, interp mode, arrive, leave]]."""
    m = level["matinees"].get(mname)
    if not m:
        return None
    A = level["actors"]
    out = {"length": m.get("length"), "props": m.get("props", {}), "groups": []}
    for g in m.get("groups", []):
        binds = []
        for bk, bv in m["binds"].items():
            if bk.replace(" ", "").lower() == g["name"].replace(" ", "").lower():
                binds = bv
        G = {"name": g["name"], "actors": binds, "tracks": []}
        for t in g["tracks"]:
            c = t["class"]
            if c == "InterpTrackMove":
                rel = t["frame"] == "IMF_RelativeToInitial"
                # (Relative to the actor as the Matinee starts, UE3's InitialTM: the
                # track's own transform at its start, its first key's, is the actor's
                # as it stands, so a first key that turns it (the gunship's, 45
                # degrees of yaw) turns the whole path the other way by as much: the
                # gunship circles the arena, as in the game.)
                euler = t.get("euler") or []
                e0 = np.array(euler[0][1], float) if rel and euler else np.zeros(3)
                p0 = np.array(t["pos"][0][1], float) if rel and t.get("pos") else np.zeros(3)
                if abs(e0[0]) > 1e-3 or abs(e0[1]) > 1e-3:
                    print(f"  ({mname} {g['name']}: its first key rolls or pitches: taken as its yaw only)")
                Z = K.euler_matrix((0.0, 0.0, e0[2]))
                per = {}
                for an in binds:
                    R = Z.T @ ue_rot(A.get(an, {}).get("Rotation"))
                    keys = []
                    for (tm, off, mode, arr, lea) in t["pos"]:
                        o = np.array(off, float)
                        w = (o - p0) @ R if rel else o
                        ta = (np.array(arr, float) @ R if rel else np.array(arr, float)) @ P * 0.01
                        tl = (np.array(lea, float) @ R if rel else np.array(lea, float)) @ P * 0.01
                        keys.append([tm, [round(x, 4) for x in (w @ P * 0.01)], mode,
                                     [round(x, 4) for x in ta], [round(x, 4) for x in tl]])
                    per[an] = keys
                G["tracks"].append({"type": "move", "relative": rel, "keys": per})
                # (Its turn: roll, pitch and yaw in degrees, relative to the actor's
                # own as it starts, when it turns at all: the gunship's; its first
                # key's yaw taken out, as above.)
                if rel and any(any(abs(x) > 1e-3 for x in k[1]) for k in euler):
                    G["tracks"].append({"type": "turn", "keys": [[k[0], [k[1][0], k[1][1], k[1][2] - e0[2]], k[2],
                                                                  list(k[3]), list(k[4])] for k in euler]})
            elif c == "InterpTrackToggle":
                G["tracks"].append({"type": "toggle", "keys": t.get("keys") or []})
            elif c == "InterpTrackFloatProp" and str(t.get("prop", "")).endswith(".Brightness"):
                # (A light's brightness: Oblivion's alarm light pulsing.)
                G["tracks"].append({"type": "brightness", "keys": t.get("keys") or []})
            elif c == "InterpTrackSound":
                G["tracks"].append({"type": "sound", "keys": t["keys"]})
            elif c == "InterpTrackAnimControl":
                G["tracks"].append({"type": "anim", "animsets": g.get("animsets"), "keys": t.get("keys")})
            elif c == "InterpTrackFloatProp" and t.get("prop") == "DrawScale":
                # (Its keys as the actor's DrawScale: the AirStrike station's force
                # field collapsing.)
                G["tracks"].append({"type": "scale", "keys": t.get("keys") or []})
        out["groups"].append(G)
    return out


def merge_audio(level, audio, prefix="AUDIO:"):
    """Adds a map's AUDIO package's Kismet (read_level of it) to `level`: what the
    BASE package's remote events set off there (Oblivion's combustion chamber's
    blast, AUDIO_AIRLOCK_CHAMBER_START), the ops and the actors they name, their
    names prefixed so as not to meet the BASE package's. Its own events besides
    remote ones (its ambience starting with the level) are left out."""
    def ren(n):
        return prefix + n if n else n

    for n, o in audio["ops"].items():
        if o["cls"].startswith("SeqEvent_") and o["cls"] != "SeqEvent_RemoteEvent":
            continue
        o2 = json.loads(json.dumps(o))
        o2["outs"] = [[d, [[ren(t), k] for t, k in links], delay] for d, links, delay in o2["outs"]]
        o2["vars"] = [[d, [ren(x) for x in names], vals] for d, names, vals in o2["vars"]]
        o2["rawvars"] = [[d, [ren(x) for x in raw]] for d, raw in o2.get("rawvars", [])]
        level["ops"][ren(n)] = o2
    for n, a in audio["actors"].items():
        a2 = dict(a)
        a2["name"] = ren(n)
        a2["index"] = None
        level["actors"][ren(n)] = a2
    for n, m in audio["matinees"].items():
        level["matinees"][ren(n)] = m
    return level


def interpret(level, glb):
    """The consoles' timelines, gameplay start's, the Matinees they play, the actors
    they touch (matched to the map model's nodes), the spawn clusters they switch."""
    ops = level["ops"]
    nodes = glb_nodes(glb)
    start = Run(level)
    for n, o in ops.items():
        if o["cls"] in ("SeqEvent_GameplayStarted", "SeqEvent_LevelLoaded"):
            start.fire(0.0, n, "Out")
    start.run(until=30.0, cap=3000)
    # (The state as gameplay settles: every Matinee where it is at 30 s.)
    for op, s in start.mat.items():
        s["pos"] = start.mpos(op, 30.0)
        s["t0"], s["p0"] = 0.0, s["pos"]
        loop = (level["matinees"].get(op) or {}).get("props", {}).get("bLooping")
        if s["playing"] and not loop and s["pos"] >= start.length(op) - 1e-6 and not s["paused"]:
            s["playing"] = False

    consoles = []
    for c in level["consoles"]:
        ev = [n for n, o in ops.items() if o["cls"] == "TnSeqEvent_Interact" and o["props"].get("Originator") == c["name"]]
        r = Run(level, {"mat": start.mat, "gate": start.gate})
        for e in ev:
            r.fire(0.0, e, "Minigame Success")
        r.run(until=120.0, cap=3000)
        R = ue_rot(c.get("Rotation"))
        spot = np.array(c.get("Location") or [0, 0, 0], float) + np.array([500.0, 0, 0]) @ R
        consoles.append({"id": c["name"], "class": c["class"], "transform": bevy_transform(c),
                         "cost": c.get("Cost"), "increment": c.get("Increment"),
                         "prompt": c.get("LocalizedCommandDescription"), "total": c.get("DataStoreOverride"),
                         "cooldown": c.get("Cooldown"), "spot": bevy_pos(spot), "timeline": r.log})

    # The actors the timelines touch, and those riding on them.
    touched = set()
    for tl in [start.log] + [c["timeline"] for c in consoles]:
        for e in tl:
            for key in ("targets", "at"):
                touched.update(e.get(key) or [])
            if e["cls"] == "SeqAct_Interp":
                for bv in (level["matinees"].get(e["op"]) or {}).get("binds", {}).values():
                    touched.update(bv)
            for x in e.get("anims") or []:
                touched.add(x["actor"])
    riders = {}
    for n, att in level["attached"].items():
        if att["base"] in touched:
            riders.setdefault(att["base"], []).append({"actor": n, "hard": att["hard"]})
    for lst in riders.values():
        for r_ in lst:
            touched.add(r_["actor"])

    played = sorted({e["op"] for tl in [start.log] + [c["timeline"] for c in consoles] for e in tl
                     if e["cls"] == "SeqAct_Interp"})
    mats = {m: matinee_tracks(level, m) for m in played}
    mats = {m: v for m, v in mats.items() if v}

    # Where each touched actor ends up (for finding it in the open map model).
    final = {}
    for mt in mats.values():
        for g in mt["groups"]:
            for tr in g["tracks"]:
                if tr["type"] == "move" and tr["relative"]:
                    for an, keys in tr["keys"].items():
                        if keys:
                            final[an] = keys[-1][1]
    for base, lst in riders.items():
        if base in final:
            for r_ in lst:
                if r_["hard"] and r_["actor"] not in final:
                    final[r_["actor"]] = final[base]
    actors = {}
    for n in sorted(touched):
        a = level["actors"].get(n)
        if not a or a["class"] in ("NavigationCell", "PathNode", "TnSniperPoint", "TnSpawnClusterSurvival"):
            continue
        mesh, comp = mesh_of(a)
        tr = bevy_transform(a)
        e = {"class": a["class"], "transform": tr, "index": a["index"]}
        if mesh:
            e["mesh"] = mesh
            for k in ("Materials", "AnimSets", "AnimNode"):
                if k in comp:
                    e[k] = comp[k]
            pos = [("closed", tr["translation"])]
            if n in final:
                pos.append(("open", [round(tr["translation"][i] + final[n][i], 4) for i in range(3)]))
            e["glb"] = glb_match(nodes, mesh, pos)
            e["final"] = final.get(n)
        for k in ("InitialTransformState", "Tag", "bHidden", "BrushComponent_"):
            if k in a:
                e[k] = a[k]
        if n in riders:
            e["riders"] = riders[n]
        actors[n] = e

    clusters = {}
    for tl in [start.log] + [c["timeline"] for c in consoles]:
        for e in tl:
            for x in e.get("targets") or []:
                if x.startswith("TnSpawnClusterSurvival") and x in level["clusters"]:
                    clusters[x] = [pt[0] for pt in level["clusters"][x]["points"]]
    return {"consoles": consoles, "start": start.log, "matinees": mats, "actors": actors, "clusters": clusters,
            "spawnpoints": [pt[0] for pt in level["spawnpoints"]], "anim_lengths": level["anim_lengths"]}
