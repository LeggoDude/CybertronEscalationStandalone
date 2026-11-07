"""Escalation's enemies as the game builds them, read from the maps' packages.

Each Escalation map's gameplay package (ESC_<map>_BASE_m.xxx) cooks in its spawn
list (TR_Survival30_m.SurvivalSpawnList_<Map>Tuned_p, a TnSurvivalSpawnList): its 15
spawn groups name the AI types that map fields (their TnAiCharacterDescription
objects), with what each costs of a wave's budget. A description is

  * its behaviour (BehaviorBlueprint, a Tn<Kind>CharacterBlueprint: its Health,
    which OnApplyCharacterProperties gives the pawn over its chassis' own, and its
    class's _AiCharacterType, which Escalation's scoring pays for a kill by:
    TR_Survival_m.SurvivalScoring_p's AiScores);
  * its look (CharacterBlueprint): the faction, its energon colour, its starting
    form (TF_Alt: a jet, an orb, an insect) and its chassis (ChassisType, a
    TnTransformer robot definition: its mesh set, physics asset, animation sets,
    collision, size, damage multiplier, death set and sound events; for a
    character that starts in its other form, AltFormBlueprint's own);
  * its colour pairs (CharacterColorPairs): the game paints each one it spawns in a
    random pair (TnAiCharacterDescription.ApplyCharacterBlueprintOverride).

A mesh set is a base skeleton (BaseMesh: TR_Skeletons_p.RB_NpcAi_SKEL for the
soldiers, RB_Iconic_SKEL, the players', for the Titan and the Heavy Gunner) with
parts (TnMeshPart: a skinned mesh following the skeleton, or a static mesh hung on
a socket) at its MeshScale; a TnMeshSetRandom has slots of parts and picks one of
each slot's at random (the soldiers' backpacks, wheels and wings, some slots with
"none" among them). Some chassis are one skeletal mesh instead (SkelMesh: the
Sniper, the Guard, the Leaper).

The physics asset's bodies (RB_BodySetup: a bone and its boxes, capsules and
spheres, AggGeom) are what shots strike; src/enemies.rs tests them. Its joints
(RB_ConstraintSetup: each body's bone held to its parent's, within its swing and
twist limits) make the ragdoll a dead one falls as (src/ragdoll.rs).

usage: python enemy_data.py <CookedPC> [out.json]
"""
import json
import os
import struct
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.path.join(HERE, "ue3"))
sys.path.insert(0, HERE)

import class_data as cd  # noqa: E402
from props import read_props  # noqa: E402
from ue3pkg import Reader  # noqa: E402

# The demo's maps and the game's packages they are (main.rs's MAPS).
MAPS = {
    "downfall": "ESC_Iacon_BASE_m.xxx",
    "oblivion": "ESC_Ark_BASE_m.xxx",
    "ancients": "ESC_Temple_BASE_m.xxx",
    "ignition": "ESC_Gladiator_BASE_m.xxx",
}
SPAWN_LIST = "TR_Survival30_m.SurvivalSpawnList_{}Tuned_p"
SPAWN_LIST_MAP = {"downfall": "Iacon", "oblivion": "Ark", "ancients": "Temple", "ignition": "Gladiator"}
# Escalation's scoring (TransGame.xxx): what a kill pays, by the killed AI's type.
SCORING = "TR_Survival_m.SurvivalScoring_p"
SHARED = "all_shared.xxx"


def _sprops(mp, v):
    if isinstance(v, tuple) and v[0] == "struct":
        try:
            return read_props(mp.pkg, Reader(v[2]), len(v[2]))
        except Exception:
            return {}
    return {}


def _cls(mp, i):
    return mp.pkg.class_name(mp.pkg.exports[i - 1]) if i > 0 else None


def _fcolor(v):
    """A Color (BGRA bytes, as UE3 stores an FColor) as sRGB [r, g, b]."""
    if isinstance(v, tuple) and v[0] == "struct" and len(v[2]) == 4:
        b, g, r, _a = v[2]
        return [r, g, b]
    return None


def _floats(v, n):
    if isinstance(v, tuple) and v[0] == "struct" and len(v[2]) >= 4 * n:
        return [round(x, 5) for x in struct.unpack("<%df" % n, v[2][:4 * n])]
    return None


def _matrix(v):
    """An FMatrix (row-major, translation in the last row) as 16 floats."""
    return _floats(v, 16)


def scoring(cooked):
    tg = cd.Mp(cooked, "TransGame.xxx")
    pr = tg.props(tg.find(SCORING))
    scores = {str(e.get("Type")): float(e.get("Score", 0.0)) for e in cd._structs(tg.pkg, pr.get("AiScores"))}
    events = {tg.name(cd._obj(e.get("EventType"))): float(e.get("Value", 0.0))
              for e in cd._structs(tg.pkg, pr.get("ScoreEvents"))}
    mults = {tg.name(cd._obj(e.get("EventType"))): float(e.get("Value", 0.0))
             for e in cd._structs(tg.pkg, pr.get("KillMultipliers"))}
    diff = pr.get("DifficultyMultipliers")
    n = struct.unpack_from("<i", diff[1])[0] if isinstance(diff, tuple) else 0
    return {"kill": float(pr.get("KillScoreAmount", 0.0)), "ai": scores, "events": events, "kill_multipliers": mults,
            "difficulty": list(struct.unpack_from("<%df" % n, diff[1], 4)) if n else [1.0]}


def blueprint_defaults(cooked):
    """Each behaviour class's defaults (TransGame.xxx's Default__<class>): its
    _AiCharacterType and Health."""
    tg = cd.Mp(cooked, "TransGame.xxx")
    out = {}
    for full, i in tg.by_full.items():
        if full.startswith("Default__") and (full.endswith("CharacterBlueprint") or full.endswith("Character")):
            pr = tg.props(i)
            if "_AiCharacterType" in pr:
                out[full[len("Default__"):]] = {"type": str(pr["_AiCharacterType"]), "health": pr.get("Health")}
    return out


def _array_floats(v, n):
    """A TArray of n-float structs (FVector, FPlane) as lists."""
    if not (isinstance(v, tuple) and v[0] == "array" and isinstance(v[1], (bytes, bytearray)) and len(v[1]) >= 4):
        return []
    count = struct.unpack_from("<i", v[1])[0]
    if count <= 0 or len(v[1]) < 4 + count * 4 * n:
        return []
    return [[round(x, 4) for x in struct.unpack_from("<%df" % n, v[1], 4 + i * 4 * n)] for i in range(count)]


def physics(mp, idx):
    """A physics asset's bodies: [{bone, boxes: [{tm, x, y, z}], sphyls: [{tm, radius,
    length}], spheres: [{tm, radius}], convex: [{points, planes}], mass}] (Unreal
    units, in the bone's space; a convex hull, as the jets' one body is, by its
    corners and its faces' planes: normal and distance; mass: High Moon's
    RB_BodySetup.Mass, PhysX's units, None where the body's shapes and density give
    it)."""
    out = []
    for b in cd._objs(mp.props(idx).get("BodySetup")) if idx else []:
        if not b:
            continue
        pr = mp.props(b)
        agg = _sprops(mp, pr.get("AggGeom"))
        mass = pr.get("Mass")
        body = {"bone": str(pr.get("BoneName")), "boxes": [], "sphyls": [], "spheres": [], "convex": [],
                "mass": float(mass) if isinstance(mass, (int, float)) and mass > 0 else None}
        for e in cd._structs(mp.pkg, agg.get("BoxElems")):
            body["boxes"].append({"tm": _matrix(e.get("TM")), "x": e.get("X"), "y": e.get("Y"), "z": e.get("Z")})
        for e in cd._structs(mp.pkg, agg.get("SphylElems")):
            body["sphyls"].append({"tm": _matrix(e.get("TM")), "radius": e.get("Radius"), "length": e.get("Length")})
        for e in cd._structs(mp.pkg, agg.get("SphereElems")):
            body["spheres"].append({"tm": _matrix(e.get("TM")), "radius": e.get("Radius")})
        for e in cd._structs(mp.pkg, agg.get("ConvexElems")):
            points, planes = _array_floats(e.get("VertexData"), 3), _array_floats(e.get("FacePlaneData"), 4)
            if points and planes:
                body["convex"].append({"points": points, "planes": planes})
        if body["boxes"] or body["sphyls"] or body["spheres"] or body["convex"]:
            out.append(body)
    return out


def _vec3(v, default):
    f = _floats(v, 3)
    return f if f else list(default)


def constraints(mp, idx):
    """A physics asset's joints (RB_ConstraintSetup): [{child, parent, pos1, pri1,
    sec1 (the joint's frame in the child's bone space: its place and its twist and
    secondary axes; Unreal's axes, the place in metres, as the setup has it), pos2,
    pri2, sec2 (the same in the parent's), swing1, swing2, twist (each limit in
    degrees, None where the axis is free)}].

    The frames line up in the skeleton's reference pose. Twist is the turn about
    the primary axis; Swing2 about the secondary one and Swing1 about the third
    (primary x secondary): the elbows and knees only swing (Swing2, 45 degrees
    each way about a primary axis halfway round their bend), the spine and the
    neck a little each way. A limited axis with no angle given is locked."""
    out = []
    for c in cd._objs(mp.props(idx).get("ConstraintSetup")) if idx else []:
        if c <= 0:
            continue
        pr = mp.props(c)
        swing, twist = bool(pr.get("bSwingLimited", False)), bool(pr.get("bTwistLimited", False))
        out.append({
            "child": str(pr.get("ConstraintBone1")), "parent": str(pr.get("ConstraintBone2")),
            "pos1": _vec3(pr.get("Pos1"), (0, 0, 0)), "pri1": _vec3(pr.get("PriAxis1"), (1, 0, 0)),
            "sec1": _vec3(pr.get("SecAxis1"), (0, 1, 0)),
            "pos2": _vec3(pr.get("Pos2"), (0, 0, 0)), "pri2": _vec3(pr.get("PriAxis2"), (1, 0, 0)),
            "sec2": _vec3(pr.get("SecAxis2"), (0, 1, 0)),
            "swing1": float(pr.get("Swing1LimitAngle", 0.0)) if swing else None,
            "swing2": float(pr.get("Swing2LimitAngle", 0.0)) if swing else None,
            "twist": float(pr.get("TwistLimitAngle", 0.0)) if twist else None,
        })
    return out


def mesh_set(mp, idx):
    """A mesh set: its base skeleton, scale and slots of parts (each part a skinned
    mesh or a static one on a socket; None where a slot may be left empty)."""
    pr = mp.props(idx) if idx > 0 else {}
    slots = [cd._objs(r.get("MeshParts")) for r in cd._structs(mp.pkg, pr.get("RandomMeshParts"))]
    slots += [[o] for o in cd._objs(pr.get("MeshParts"))]
    out = []
    for alts in slots:
        slot = []
        for o in alts:
            if not o:
                slot.append(None)
                continue
            p = mp.props(o) if o > 0 else {}
            sk, st = cd._obj(p.get("SkeletalMeshAttachment")), cd._obj(p.get("StaticMeshAttachment"))
            if sk:
                slot.append({"mesh": mp.name(sk), "package": mp.full(sk).split(".")[0]})
            elif st:
                slot.append({"static": mp.name(st), "socket": cd._str(p.get("AttachmentSocket")),
                             "package": mp.full(st).split(".")[0]})
        if slot:
            out.append(slot)
    base = cd._obj(pr.get("BaseMesh"))
    return {"base": mp.name(base) if base else None, "scale": float(pr.get("MeshScale", 1.0)), "slots": out}


def form(mp, chassis, alt):
    """A character's form: its meshes (a mesh set's slots, or its one skeletal mesh),
    physics, animation sets and anim tree, collision, damage multiplier, death set,
    sound events."""
    pr = mp.props(alt if alt else chassis)
    cpr = mp.props(chassis)
    out = {"kind": _cls(mp, alt) if alt else "TnTransformer"}
    ms = cd._obj(pr.get("TransformerMeshSet")) or cd._obj(pr.get("FormMeshSet"))
    sk = cd._obj(pr.get("SkelMesh"))
    if ms:
        out.update(mesh_set(mp, ms))
    elif sk:
        out.update({"base": None, "scale": 1.0, "slots": [[{"mesh": mp.name(sk), "package": mp.full(sk).split(".")[0]}]]})
    col = _sprops(mp, (pr if "Collision" in pr else cpr).get("Collision"))
    out["collision"] = [col.get("CollisionRadius"), col.get("CollisionHeight")]
    out["translation"] = _floats(pr.get("MeshTranslation") or cpr.get("MeshTranslation"), 3)
    pa = cd._obj(pr.get("PhysicsAsset"))
    out["physics_asset"] = mp.full(pa) if pa else None
    out["physics"] = physics(mp, pa)
    out["joints"] = constraints(mp, pa)
    out["anim_sets"] = [mp.name(o) for o in cd._objs(pr.get("AnimSets")) if o]
    out["anim_tree"] = mp.full(cd._obj(pr.get("AnimTree"))) if cd._obj(pr.get("AnimTree")) else None
    out["damage_multiplier"] = float(pr.get("DamageMultiplier", 1.0))
    out["death_set"] = mp.full(cd._obj(pr.get("DeathSet"))) if cd._obj(pr.get("DeathSet")) else None
    out["sounds"] = mp.full(cd._obj(pr.get("SoundEventSet"))) if cd._obj(pr.get("SoundEventSet")) else None
    if not alt:
        out["merge_scale"] = float(cpr.get("RobotMeshMergeScale", 1.0))
        out["health"] = float(cpr.get("Health", 100.0))
    return out


def description(mp, idx, defaults, scores):
    pr = mp.props(idx)
    bb = cd._obj(pr.get("BehaviorBlueprint"))
    bpr = mp.props(bb) if bb > 0 else {}
    over = cd._obj(pr.get("BehaviorBlueprintOverride"))
    opr = mp.props(over) if over > 0 else {}
    bcls = _cls(mp, bb)
    base = defaults.get(bcls, {})
    cb = _sprops(mp, pr.get("CharacterBlueprint"))
    chassis = cd._obj(cb.get("ChassisType"))
    cpr = mp.props(chassis)
    alt = cd._obj(cpr.get("AltFormBlueprint"))
    start_alt = str(cb.get("StartingForm")) == "TF_Alt"
    robot = form(mp, chassis, 0)
    out = {
        "desc": mp.full(idx),
        "id": mp.name(idx).replace("_CHARACTER", ""),
        "name": cd._str(cb.get("CharacterName")),
        "faction": str(cb.get("Faction", "")),
        "behaviour": bcls,
        "ai_type": base.get("type"),
        "energon": _floats(cb.get("EnergonColor"), 4),
        "colours": [[_fcolor(c.get("PrimaryColor")), _fcolor(c.get("SecondaryColor"))]
                    for c in cd._structs(mp.pkg, pr.get("CharacterColorPairs"))],
        "own_colours": [_fcolor(cb.get("PrimaryColor")), _fcolor(cb.get("SecondaryColor"))],
        "start_alt": start_alt,
        "robot": robot,
        "alt": form(mp, chassis, alt) if alt else None,
        "chassis": mp.full(chassis),
        "loot": str(pr.get("LootClassification", "")),
    }
    # Health: the behaviour's (its override's, its class's), else the chassis'.
    health = opr.get("Health", bpr.get("Health", base.get("health")))
    out["health"] = float(health) if isinstance(health, (int, float)) else robot["health"]
    out["score"] = scores["ai"].get(out["ai_type"], scores["kill"])
    return out


def roster(mp, map_name):
    """The AI types a map's spawn list fields, in the order its groups first name
    them, each with its cost and most at once (the first group's)."""
    sl = mp.props(mp.find(SPAWN_LIST.format(SPAWN_LIST_MAP[map_name])))
    seen, out = set(), []
    for g in cd._structs(mp.pkg, sl.get("SpawnGroups")):
        for a in cd._structs(mp.pkg, g.get("AITypes")):
            d = cd._obj(a.get("Description"))
            if d and d not in seen:
                seen.add(d)
                out.append((d, {"cost": a.get("Cost"), "max": a.get("MaxCount"), "weight": a.get("Weight")}))
    return out


def titan_drones(mp, titan):
    """The drones the Titan sends out: the AI descriptions its behaviour's drone
    sensor names (TnTitanCharacterBlueprint's Drones, a TnDroneSensorBlueprint: its
    waves' Drones, two Support Drones at a time and three Explosive Drones when it
    can't reach its enemy), in the order they are first named."""
    out = []

    def visit(pr, depth):
        for v in pr.values():
            if not isinstance(v, tuple):
                continue
            if v[0] == "obj":
                o = cd._obj(v)
                if o > 0 and _cls(mp, o) == "TnAiCharacterDescription":
                    if o not in out:
                        out.append(o)
                elif o > 0 and depth > 0 and _cls(mp, o) and "Blueprint" in _cls(mp, o):
                    visit(mp.props(o), depth - 1)
            elif v[0] == "struct" and depth > 0:
                visit(_sprops(mp, v), depth - 1)
            elif v[0] == "array":
                objs = cd._objs(v)
                if objs and all(0 <= o <= len(mp.pkg.exports) for o in objs):
                    for o in objs:
                        if o > 0 and _cls(mp, o) == "TnAiCharacterDescription" and o not in out:
                            out.append(o)
                elif depth > 0:
                    for e in cd._structs(mp.pkg, v):
                        visit(e, depth - 1)

    visit(mp.props(cd._obj(mp.props(titan).get("BehaviorBlueprint"))), 3)
    return out


def read(cooked):
    defaults = blueprint_defaults(cooked)
    scores = scoring(cooked)
    types, maps = {}, {}
    for map_name, package in MAPS.items():
        mp = cd.Mp(cooked, package)
        ids = []
        found = roster(mp, map_name)
        for d, _spawn in list(found):
            if mp.name(d) == "Titan_CHARACTER":
                for o in titan_drones(mp, d):
                    if o not in [x for x, _ in found]:
                        found.append((o, {"cost": 0, "max": 0, "weight": 0, "titan_drone": True}))
        for d, spawn in found:
            t = description(mp, d, defaults, scores)
            t.setdefault("maps", [])
            if t["id"] not in types:
                t["package"] = package
                types[t["id"]] = t
            types[t["id"]]["maps"].append(map_name)
            types[t["id"]].setdefault("spawn", {})[map_name] = spawn
            ids.append(t["id"])
        maps[map_name] = ids
    return {"maps": maps, "types": types, "scoring": scores}


if __name__ == "__main__":
    data = read(sys.argv[1])
    text = json.dumps(data, indent=1)
    if len(sys.argv) > 2:
        open(sys.argv[2], "w").write(text)
    else:
        print(text)
