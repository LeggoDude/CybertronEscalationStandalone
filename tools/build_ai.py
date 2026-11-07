"""Build the enemies' AI data: assets/models/enemies/ai.txt, read from the game.

Each Escalation enemy type's behaviour is its AI description's BehaviorBlueprint (a
Tn<Kind>CharacterBlueprint, tools/enemy_data.py), whose planning domain the game
cooks into it already built (an HtnPlanningDomain subobject: what the blueprint's
OnBuildPlanningDomain registers):

  * its tasks (TaskDescriptions, indexed by the class's task symbol enum: primitive
    or compound, with their parameter names and defaults);
  * its operators (OperatorDescriptions: the operator class a primitive task runs,
    its preconditions on the world state and its effects, its Benefit and the AI
    behaviour flags it turns on);
  * its methods (MethodDescriptionSets: each compound task's methods in order, each
    with its preconditions and its task network, the subtasks with their parameter
    values);
  * the world states it uses (_UsedWorldStates) and resets (_ResettingWorldStates).

A precondition or effect compares (or sets) a world state attribute's value and its
context; its data source is a constant (0 or more) or a parameter of its task
(-(index + 1)), already resolved in the cooked domain. The Aut and Dec variants of a
kind share their class's domain; a domain is written once per class.

The blueprint's own values (Health, speeds, ranges, timings, the anim and asset
references its asset hooks resolve) are its class defaults (TransGame.xxx's
Default__ objects up the class chain) overridden by the blueprint's own, and the
subobjects it owns (its hit reaction pickers, directional anim sets, operator
blueprints) flattened under its keys, as are the hit reaction pickers it shares
with the other types (its NavigatingHitReaction, TR_AI_HitReactionPickers_p's).

ai.txt (tab-separated):
  domain <class>                                  a planning domain (the lines after it)
    task   <symbol> <name> <primitive|compound> <param names (comma)> <defaults (comma)>
    op     <symbol> <operator class> <benefit> <behaviour flags>
    method <symbol> <index>
    pre    <attribute> <comparison> <value> <context comparison> <context> <and|or>
           (the operator's or method's preconditions; comparisons by number:
           Htn.EHtnComparisonOperator)
    eff    <attribute> <operator> <value> <context operator> <context> <external 0/1>
           (Htn.EHtnEffectOperator)
    sub    <symbol> <values...>                  a method's subtask and its parameter values
    used   <attributes...>                        the world states the domain uses
    reset  <attribute> <value> <context>
    debugflags <n>
  type   <enemy type id> <behaviour class> <domain class or -> <goal symbol>
    prop <key> <value>                            the behaviour's values (scalars; a
           struct's fields as key.field, an array's items as key[i]; objects by name),
           its gun's under Weapon. (Weapon.class: its PrimaryWeapon's class)
           (a projectile gun's round under Weapon.proj.: its data's values, its class
           and its projectile mesh's, Weapon.proj.mesh.), its chassis's under
           Chassis. (its robot form's TnTransformer: speeds, mass, collision...,
           with the other form's over them for one that starts in that form:
           StartingForm TF_Alt; its momentum and acrobatics blueprints' under
           Chassis.MomentumBlueprint. and Chassis.AcrobaticsManagerBlueprint.), the
           form it doesn't start in under Other. for the types that change form as
           they fight (TRANSFORMING: Other names it) and its melee set's under
           Melee.; BehaviorBlueprintOverride names the override its values are read
           over, if it has one
  projectile <class> <key> <value>               a projectile the enemies' animations
                                                  spawn (TnAnimNotify_SpawnProjectile):
                                                  its values as a gun's round's
  enum   <name> <values...>                       enums the AI reads (world states,
                                                  location references, events...)

And their sounds, assets/models/enemies/ai_sounds.txt (tab-separated): each type's
sound event sets (its robot form's and its vehicle form's SoundEventSet, an
HmSoundEventSet: the cue each sound event plays for it, TnPawn.FindSoundCue, and the
dialog characters whose lines its dialog cues play), and the dialog events' rules
(HmSoundEvent's DialogGroup and DialogPriority, HmSoundEventDialogGroup's, as
HmDialogEventManager plays them):
  type    <enemy type id> <robot form's set or -> <vehicle form's set or ->
  set     <set> <dialog characters...>
  cue     <set> <sound event> <cue>
  dialog  <dialog event> <group or -> <priority>
  group   <group> <parent or -> <initial chance to play, %> <retrigger delay min s>
          <retrigger delay max s> <instance limit>
  default <group>          (HmDialogEventManager's DefaultDialogGroup: an event's
                           without a group of its own)
Names are the game's (packages and all, a group's its own name).

usage: python build_ai.py <CookedPC> <foc_esc/assets>
"""
import os
import struct
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.path.join(HERE, "ue3"))
sys.path.insert(0, HERE)

import class_data as cd  # noqa: E402
import enemy_data as ed  # noqa: E402
from props import read_props  # noqa: E402
from ue3pkg import Reader  # noqa: E402

ENUMS = ["AI.EWorldState", "AI.ELocationReference", "AI.EEvent", "AI.ESensor", "AI.EMotionSpeed",
         "AI.EMotionFacing", "AI.ETnRange", "AI.EEnemyForm", "AI.EAiCharacterType", "AI.EDangerClose",
         "AI.ESquadActionType", "AI.ESquadCommand", "AI.EAttackReady", "AI.EKismetPriority",
         "AI.EEnemyMovementType", "AI.EAlertToEnemyPresenceContext", "AI.ECurrentTaskInfo"]
CMP = ["CO_Equal", "CO_NotEqual", "CO_LessOrEqual", "CO_Less", "CO_GreaterOrEqual", "CO_Greater",
       "CO_BitIsSet", "CO_BitIsNotSet", "CO_BitsAreSet", "CO_BitsAreNotSet", "CO_AnyEnumSet", "CO_True"]
EOP = ["EO_Set", "EO_Add", "EO_Subtract", "EO_SetBit", "EO_ClearBit"]
# The blueprints whose characters change form as they fight (their other form's
# values as Other.<key>): the Snipers (robot, flying; insect, on their perches) and
# the jets (plane; robot).
TRANSFORMING = ("TnSniperCharacterBlueprint", "TnJetCharacterBlueprint")
# A weapon's data, by the game it is fired in: Escalation's AI fires with its
# AiSurvivalData (TnWeapon.GetWeaponData), else its AiData, else its Data.
WEAPON_DATA = ("AiSurvivalData", "AiData", "Data")
# The packages the maps' weapons and projectiles come from when they don't cook them
# (the multiplayer ones Escalation has loaded: all_shared.xxx's classes, and
# mp_shared.xxx's vehicle weapons: the jets' dogfight gun, TnWeaponRocketVehicle,
# and its rocket).
SHARED_PACKAGES = ("all_shared.xxx", "mp_shared.xxx")
# Where the dialog events and their groups are cooked (besides a map's own).
DIALOG_PACKAGES = ["TransGame.xxx", "all_shared.xxx"]
# The dialog events every pawn's incidental dialog plays (TnPawn: as it comes in,
# SpawnedDialog and its teammates' TeammateSpawnedDialog; as it dies, unless by a
# headshot, DeathRX_Verbal, DeathRX_NonVerbalDialog and its teammates'
# TeammateKilledDialog), whether its set has a cue for them or not (the dialog
# manager plays them all the same, its groups keeping quiet after them).
INCIDENTAL_DIALOG = ["SpawnedDialog", "TeammateSpawnedDialog", "DeathRX_Verbal", "DeathRX_NonVerbalDialog",
                     "TeammateKilledDialog"]
# HmDialogEventManager's DefaultDialogGroup.
DEFAULT_DIALOG_GROUP = "SoundEvents_Dialog.DefaultDialogGroup"


def ref_name(mp, idx):
    """An object's whole name (package and outers) from a reference to it: an
    import's too (Mp.full gives an import's own name only)."""
    p = mp.pkg
    parts = []
    while idx < 0:
        parts.append(p.obj_name(idx))
        idx = p.imports[-idx - 1]["outer"]
    while idx > 0:
        parts.append(p.obj_name(idx))
        idx = p.exports[idx - 1]["outer"]
    return ".".join(reversed(parts))


class Sounds:
    """The enemy types' sound sets and their dialog events, for ai_sounds.txt."""

    def __init__(self, cooked):
        self.dialog_mps = [cd.Mp(cooked, n) for n in DIALOG_PACKAGES]
        self.types = []
        self.sets = {}
        self.events = {}
        self.groups = {}

    def obj(self, mps, full):
        """An object's package and properties, from the first package with them
        (one that only refers to it, or holds an empty copy, is passed over)."""
        for m in mps:
            i = m.find(full)
            if not i:
                continue
            try:
                pr = m.props(i)
            except Exception:
                continue
            if pr:
                return m, pr
        return None, {}

    def read_set(self, mp, full):
        """A set's dialog characters and (event, cue) entries."""
        if full in self.sets:
            return
        m, pr = self.obj([mp] + self.dialog_mps, full)
        if not m:
            print("  ! no sound set", full)
            self.sets[full] = ([], [])
            return
        chars = [ref_name(m, o).split(".")[-1] for o in cd._objs(pr.get("DialogCharacters")) if o]
        entries = []
        for el in cd._structs(m.pkg, pr.get("SoundEventSet")):
            e, s = cd._obj(el.get("Event")), cd._obj(el.get("Sound"))
            if e and s:
                entries.append((ref_name(m, e), ref_name(m, s)))
        self.sets[full] = (chars, entries)
        for event, _ in entries:
            if event.startswith("SoundEvents_Dialog."):
                self.read_event(mp, event)

    def read_event(self, mp, event):
        if event in self.events:
            return
        m, pr = self.obj([mp] + self.dialog_mps, event)
        group = None
        if m:
            g = cd._obj(pr.get("DialogGroup"))
            group = ref_name(m, g) if g else None
        self.events[event] = (group.split(".")[-1] if group else None, float(pr.get("DialogPriority", 0.0)))
        while group and group.split(".")[-1] not in self.groups:
            gm, gpr = self.obj([m or mp] + self.dialog_mps, group)
            if not gm:
                # (A reference by its own name only: look for it under the groups'.)
                gm, gpr = self.obj([mp] + self.dialog_mps, "SoundEvents_Dialog.Muting_Setup_Groups." + group)
            parent = cd._obj(gpr.get("ParentDialogGroup")) if gm else 0
            parent = ref_name(gm, parent) if parent else None
            self.groups[group.split(".")[-1]] = (
                parent.split(".")[-1] if parent else None, int(gpr.get("InitialChanceToPlay", 100)),
                float(gpr.get("ReTriggerDelayMin", 0.0)), float(gpr.get("ReTriggerDelayMax", 0.0)),
                int(gpr.get("InstanceLimit", 1)))
            m = gm or m
            group = parent

    def add_type(self, mp, tid, desc_pr):
        """A type's sets (its robot form's and vehicle form's)."""
        cb = cd._struct_props(mp.pkg, desc_pr["CharacterBlueprint"]) if "CharacterBlueprint" in desc_pr else {}
        chassis = cd._obj(cb.get("ChassisType"))
        cpr = mp.props(chassis) if chassis > 0 else {}
        alt = cd._obj(cpr.get("AltFormBlueprint"))
        apr = mp.props(alt) if alt > 0 else {}
        sets = []
        for pr in (cpr, apr):
            s = cd._obj(pr.get("SoundEventSet"))
            full = ref_name(mp, s) if s else None
            if full:
                self.read_set(mp, full)
            sets.append(full or "-")
        self.types.append([tid] + sets)

    def lines(self, mp):
        for e in INCIDENTAL_DIALOG:
            self.read_event(mp, "SoundEvents_Dialog." + e)
        # (And those the animations' dialog notifies play: HmAnimNotify_Dialog's
        # and HmAnimNotify_DialogDeath's, in the shared packages' animations.)
        for m in self.dialog_mps:
            for i, e in enumerate(m.pkg.exports, 1):
                if m.pkg.class_name(e) not in ("HmAnimNotify_Dialog", "HmAnimNotify_DialogDeath"):
                    continue
                try:
                    ev = cd._obj(m.props(i).get("SoundEvent"))
                except Exception:
                    continue
                if ev:
                    self.read_event(mp, ref_name(m, ev))
        self.read_event(mp, "SoundEvents_Dialog.DefaultDialogGroup")
        self.events.pop("SoundEvents_Dialog.DefaultDialogGroup", None)
        if "DefaultDialogGroup" not in self.groups:
            m, gpr = self.obj([mp] + self.dialog_mps, DEFAULT_DIALOG_GROUP)
            self.groups["DefaultDialogGroup"] = (None, int(gpr.get("InitialChanceToPlay", 100)),
                                                 float(gpr.get("ReTriggerDelayMin", 0.0)),
                                                 float(gpr.get("ReTriggerDelayMax", 0.0)),
                                                 int(gpr.get("InstanceLimit", 1)))
        out = [["type"] + t for t in self.types]
        for full, (chars, entries) in sorted(self.sets.items()):
            out.append(["set", full] + chars)
            for event, cue in entries:
                out.append(["cue", full, event, cue])
        for event, (group, priority) in sorted(self.events.items()):
            out.append(["dialog", event, group or "-", "%g" % priority])
        for group, (parent, chance, lo, hi, limit) in sorted(self.groups.items()):
            out.append(["group", group, parent or "-", chance, "%g" % lo, "%g" % hi, limit])
        out.append(["default", DEFAULT_DIALOG_GROUP.split(".")[-1]])
        return out


def enum_names(p, full):
    """A UEnum's value names (its name list at the end of its export)."""
    idx = None
    for i in range(len(p.exports)):
        if p.full_name(i + 1) == full:
            idx = i
            break
    if idx is None:
        return None
    e = p.exports[idx]
    d = p.data[e["offset"]:e["offset"] + e["size"]]
    for st in range(0, len(d) - 4):
        n = struct.unpack_from("<i", d, st)[0]
        if 0 < n < 1000 and st + 4 + 8 * n <= len(d) and len(d) - (st + 4 + 8 * n) <= 8:
            out = []
            for k in range(n):
                ni, nn = struct.unpack_from("<ii", d, st + 4 + 8 * k)
                if not 0 <= ni < len(p.names):
                    out = None
                    break
                out.append(p.names[ni] + ("" if nn == 0 else "_%d" % (nn - 1)))
            if out:
                return out
    return None


def ds(p, v):
    """A data source's constant (variables come resolved into it)."""
    sp = cd._struct_props(p, v) if isinstance(v, tuple) else {}
    return int(sp.get("Constant", 0))


def ints(v):
    if not (isinstance(v, tuple) and v[0] == "array") or len(v[1]) < 4:
        return []
    n = struct.unpack_from("<i", v[1], 0)[0]
    if len(v[1]) == 4 + 4 * n:
        return list(struct.unpack_from("<%di" % n, v[1], 4))
    if len(v[1]) == 4 + n:
        return list(v[1][4:4 + n])
    return []


def names(p, v):
    if not (isinstance(v, tuple) and v[0] == "array") or len(v[1]) < 4:
        return []
    n = struct.unpack_from("<i", v[1], 0)[0]
    r = Reader(v[1])
    r.i32()
    return [p.fname(r) for _ in range(n)]


def enum_index(v, table):
    if isinstance(v, int):
        return v
    s = str(v)
    return table.index(s) if s in table else 0


def domain_lines(mp, dom, symbols):
    p = mp.pkg
    pr = mp.props(dom)
    out = []

    def sym_name(s):
        return symbols[s] if 0 <= s < len(symbols) else str(s)

    def pre(c):
        return ["pre", int(c.get("Attribute", 0)), enum_index(c.get("ValueComparison", "CO_Equal"), CMP),
                ds(p, c.get("Value")), enum_index(c.get("ContextComparison", "CO_Equal"), CMP),
                ds(p, c.get("Context")), "or" if str(c.get("Combiner", "BO_And")) == "BO_Or" else "and"]

    def eff(e):
        return ["eff", int(e.get("Attribute", 0)), enum_index(e.get("ValueOperator", "EO_Set"), EOP),
                ds(p, e.get("Value")), enum_index(e.get("ContextOperator", "EO_Set"), EOP),
                ds(p, e.get("Context")), 1 if e.get("WillBeSetExternally") else 0]

    for t in cd._structs(p, pr.get("TaskDescriptions")):
        s = int(t.get("Symbol", -1))
        if s < 0:
            continue
        kind = {"TT_Primitive": "primitive", "TT_Compound": "compound"}.get(str(t.get("Type")), "unknown")
        pn = names(p, t.get("ParameterNames")) if "ParameterNames" in t else []
        pd = ints(t.get("ParameterDefaults")) if "ParameterDefaults" in t else []
        out.append(["task", s, sym_name(s), kind, ",".join(pn) or "-", ",".join(str(x) for x in pd) or "-"])
    for o in cd._structs(p, pr.get("OperatorDescriptions")):
        s = int(o.get("TaskSymbol", -1))
        if s < 0:
            continue
        ot = cd._obj(o.get("OperatorType"))
        out.append(["op", s, mp.name(ot) if ot else "HtnOperator", int(o.get("Benefit", 0)),
                    int(o.get("BehaviorFlags", 0))])
        out += [pre(c) for c in cd._structs(p, o.get("Preconditions"))]
        out += [eff(e) for e in cd._structs(p, o.get("Effects"))]
    for k, ms in enumerate(cd._structs(p, pr.get("MethodDescriptionSets"))):
        for j, m in enumerate(cd._structs(p, ms.get("MethodDescriptions"))):
            out.append(["method", int(m.get("TaskSymbol", k)), j])
            out += [pre(c) for c in cd._structs(p, m.get("Preconditions"))]
            for t in cd._structs(p, m.get("Tasks")):
                n = int(t.get("NumParameters", 0))
                vals = []
                for q in range(n):
                    key = "Parameters" if q == 0 else "Parameters[%d]" % q
                    tp = cd._struct_props(p, t[key]) if key in t else {}
                    vals.append(ds(p, tp.get("Value")))
                out.append(["sub", int(t.get("Symbol", -1))] + vals)
    used = [i for i, x in enumerate(ints(pr.get("_UsedWorldStates"))) if x]
    out.append(["used"] + used)
    for w in cd._structs(p, pr.get("_ResettingWorldStates")):
        out.append(["reset", int(w.get("Attribute", 0)), int(w.get("DefaultValue", 0)), int(w.get("DefaultContext", 0))])
    out.append(["debugflags", int(pr.get("DebugFlags", 0))])
    return out


# ---------------------------------------------------------------------------
# Blueprint values
# ---------------------------------------------------------------------------

def class_chain(tg, cls):
    """The class and its parents (names), leaf first."""
    out = []
    p = tg.pkg
    i = tg.find(cls)
    while i and i > 0:
        out.append(p.full_name(i))
        i = p.exports[i - 1].get("super", 0)
    return out


# Objects of these classes are data a blueprint shares with the others (their
# package's own, not its subobjects): expanded under its key as its own are. The
# navigation hit reactions every infantry type plays (NavigatingHitReaction:
# TR_AI_HitReactionPickers_p.TR_AI_Navigation_HitReactionPicker, the stumbles
# STS_NavigationHitReaction plays as a blow lands while it moves).
SHARED_DATA = {"TnHitReactionPicker"}


def expands(mp, i, full, owner):
    """Whether a reference's object is expanded under its key: one of the owner's
    subobjects, or shared data (SHARED_DATA)."""
    if i <= 0:
        return False
    if owner and full.startswith(owner + "."):
        return True
    return mp.pkg.class_name(mp.pkg.exports[i - 1]) in SHARED_DATA


def flatten(mp, pr, prefix, out, owner, depth=0):
    """Properties as key -> text (structs and arrays flattened, the owner's own
    subobjects and the shared data it refers to expanded under their key)."""
    p = mp.pkg
    for k, v in pr.items():
        key = prefix + k
        if k.startswith("__") or k in ("ObjectArchetype", "Name", "Class", "Outer", "_PregeneratedPlanningDomain",
                                       "RigidBodyMeshCache"):
            continue
        if isinstance(v, bool):
            out[key] = "1" if v else "0"
        elif isinstance(v, int):
            out[key] = str(v)
        elif isinstance(v, float):
            out[key] = "%g" % v
        elif isinstance(v, str):
            out[key] = v
        elif isinstance(v, tuple):
            kind = v[0]
            if kind == "obj":
                i = v[1]
                if i == 0:
                    out[key] = "None"
                    continue
                full = mp.full(i)
                out[key] = full
                if depth < 4 and expands(mp, i, full, owner):
                    flatten(mp, mp.props(i), key + ".", out, owner, depth + 1)
                    out[key + ".class"] = p.class_name(p.exports[i - 1])
            elif kind == "struct":
                sname, data = v[1], v[2]
                if sname == "Vector" and len(data) == 12:
                    out[key] = "%g %g %g" % struct.unpack("<3f", data)
                elif sname == "Rotator" and len(data) == 12:
                    out[key] = "%d %d %d" % struct.unpack("<3i", data)
                elif sname in ("Vector2D",) and len(data) == 8:
                    out[key] = "%g %g" % struct.unpack("<2f", data)
                elif sname in ("Color",) and len(data) == 4:
                    out[key] = "%d %d %d %d" % tuple(data)
                elif sname in ("LinearColor",) and len(data) == 16:
                    out[key] = "%g %g %g %g" % struct.unpack("<4f", data)
                else:
                    try:
                        sp = read_props(p, Reader(data), len(data))
                    except Exception:
                        sp = None
                    if sp:
                        flatten(mp, sp, key + ".", out, owner, depth + 1)
                    elif len(data) % 4 == 0 and 0 < len(data) <= 64:
                        out[key] = " ".join("%g" % x for x in struct.unpack("<%df" % (len(data) // 4), data))
            elif kind == "array":
                data = v[1]
                if len(data) < 4:
                    continue
                n = struct.unpack_from("<i", data, 0)[0]
                out[key + ".n"] = str(n)
                if n == 0:
                    continue
                # Structs with tagged properties?
                items = []
                try:
                    r = Reader(data)
                    r.i32()
                    for _ in range(n):
                        items.append(read_props(p, r, len(data)))
                    if r.o != len(data) or not all(items):
                        items = []
                except Exception:
                    items = []
                if items:
                    for j, it in enumerate(items):
                        flatten(mp, it, "%s[%d]." % (key, j), out, owner, depth + 1)
                    continue
                if len(data) == 4 + 4 * n:
                    vals = struct.unpack_from("<%di" % n, data, 4)
                    if all(-len(p.imports) <= x <= len(p.exports) for x in vals) and any(vals) \
                            and all(x == 0 or (x > 0 and p.class_name(p.exports[x - 1]) not in ("", None)) or x < 0
                                    for x in vals):
                        for j, x in enumerate(vals):
                            out["%s[%d]" % (key, j)] = mp.full(x) if x else "None"
                            if depth < 4 and expands(mp, x, mp.full(x), owner):
                                flatten(mp, mp.props(x), "%s[%d]." % (key, j), out, owner, depth + 1)
                                out["%s[%d].class" % (key, j)] = p.class_name(p.exports[x - 1])
                        continue
                    fl = struct.unpack_from("<%df" % n, data, 4)
                    out[key] = " ".join("%g" % x for x in fl)
                    continue
                if len(data) == 4 + 8 * n:
                    try:
                        r = Reader(data)
                        r.i32()
                        names_ = [p.fname(r) for _ in range(n)]
                        # (No names, but pairs of floats: an array of Vector2Ds.)
                        if any(x.startswith("?") for x in names_):
                            fl = struct.unpack_from("<%df" % (2 * n), data, 4)
                            out[key] = " ".join("%g" % x for x in fl)
                        else:
                            out[key] = " ".join(names_)
                        continue
                    except Exception:
                        pass
                if len(data) == 4 + n:
                    out[key] = " ".join(str(b) for b in data[4:])
            elif kind == "StrProperty":
                out[key] = cd._str(v).replace("\t", " ").replace("\n", " ")
    return out


def weapon_values(mp, shared, tg, cls_full):
    """A weapon class's values (its default object's, up its class chain: the map
    package cooks the AI's own subclass, the shared packages its parents, or the
    whole class: mp_shared.xxx's TnWeaponRocketVehicle, the jets' dogfight gun,
    which the maps only name) and its data's (the first of WEAPON_DATA any of them
    names, over the data class's defaults), as (defaults, data, data's name); ({},
    {}, None) if none. `shared`: the shared packages (SHARED_PACKAGES)."""
    short = cls_full.split(".")[-1]
    chain = []
    cls = short
    seen = set()
    while cls and cls not in seen:
        seen.add(cls)
        found = None
        for m in (mp, *shared, tg):
            for pre in ("TransContent.", "TransGame."):
                d = m.find(pre + "Default__" + cls)
                if d and d > 0:
                    found = (m, d)
                    break
            if found:
                break
        if not found:
            break
        chain.append(found)
        # The class's parent (its super: an export of this package or an import).
        m = found[0]
        ci = m.find("TransContent." + cls) or m.find("TransGame." + cls)
        sup = m.pkg.exports[ci - 1].get("super", 0) if ci and ci > 0 else 0
        cls = m.pkg.obj_name(sup) if sup else None
    defaults = {}
    data = None
    for m, d in reversed(chain):
        pr = m.props(d)
        flatten(m, pr, "", defaults, None)
        for k in WEAPON_DATA:
            o = cd._obj(pr.get(k))
            if o > 0 and (data is None or WEAPON_DATA.index(k) <= data[2]):
                data = (m, o, WEAPON_DATA.index(k))
    if data is None:
        return defaults, {}, None
    m, o, _ = data
    wv = {}
    for c in reversed(class_chain(tg, m.pkg.class_name(m.pkg.exports[o - 1]))):
        di = tg.find("Default__" + c)
        if di:
            flatten(tg, tg.props(di), "", wv, None)
    flatten(m, m.props(o), "", wv, m.full(o))
    return defaults, wv, m.full(o)


def object_values(mps, full, prefix, out):
    """An object's values (an export of the first of these packages that has it, by
    its whole name), its own subobjects expanded, as prefix + key. False if none
    has it."""
    for m in mps:
        i = m.find(full)
        if i and i > 0:
            flatten(m, m.props(i), prefix, out, full)
            return True
    return False


def projectile_values(mp, shared, tg, cls_full, prefix):
    """A projectile class's values as prefix + key: its data's (the first of
    WEAPON_DATA it names, as weapon_values reads them), its class ('class'), its
    data's name ('data') and its projectile mesh's (its default Mesh, a
    TnProjectileMesh: its flight and explosion effects and sounds, its light...,
    as 'mesh.<key>'). And its data's name (None if it has none)."""
    pdefs, pv, pdata = weapon_values(mp, shared, tg, cls_full)
    out = {prefix + k: v for k, v in pv.items()}
    out[prefix + "class"] = cls_full.split(".")[-1]
    # Its class and its parents' ('supers', leaf first: a TnProjectileHoming homes).
    out[prefix + "supers"] = " ".join(class_supers((mp, *shared, tg), cls_full.split(".")[-1]))
    if pdata:
        out[prefix + "data"] = pdata
    mesh = pdefs.get("Mesh")
    if mesh and mesh != "None":
        if not object_values((mp, *shared, tg), mesh, prefix + "mesh.", out):
            print("  ! no projectile mesh", mesh, "for", cls_full)
    return out, pdata


def class_supers(mps, cls):
    """A class's name and its parents' (leaf first), each found as a class export
    of the first of these packages that has it (TransContent or TransGame)."""
    out = []
    while cls and cls not in out:
        out.append(cls)
        sup = None
        for m in mps:
            ci = m.find("TransContent." + cls) or m.find("TransGame." + cls) or m.find(cls)
            if ci and ci > 0 and m.pkg.class_name(m.pkg.exports[ci - 1]) == "Class":
                s = m.pkg.exports[ci - 1].get("super", 0)
                sup = m.pkg.obj_name(s) if s else None
                break
        cls = sup
    return out


def notify_projectiles(mp):
    """The projectile classes the package's animations spawn
    (TnAnimNotify_SpawnProjectile's ProjectileClass), by their whole names."""
    p = mp.pkg
    out = set()
    for i, e in enumerate(p.exports, 1):
        if p.class_name(e) != "TnAnimNotify_SpawnProjectile":
            continue
        c = cd._obj(mp.props(i).get("ProjectileClass"))
        if c:
            out.add(ref_name(mp, c))
    return out


def is_weapon(v):
    """A weapon class reference: by its whole name, or by its name alone (a class
    the map's package doesn't cook: the jets' DogfightWeapon)."""
    if not isinstance(v, str):
        return False
    parts = v.split(".")
    return parts[-1].startswith("TnWeapon") and (len(parts) == 1 or parts[0] in ("TransContent", "TransGame"))


def main():
    cooked, assets = sys.argv[1], sys.argv[2]
    tg = cd.Mp(cooked, "TransGame.xxx")
    shared = tuple(cd.Mp(cooked, s) for s in SHARED_PACKAGES)
    lines = []
    domains_done = set()
    types_done = set()
    sounds = Sounds(cooked)
    sound_mp = None
    # The projectiles the enemies' animations spawn, and a package that has each.
    notified = {}
    for map_name, package in ed.MAPS.items():
        mp = cd.Mp(cooked, package)
        p = mp.pkg
        for c in notify_projectiles(mp):
            notified.setdefault(c, mp)
        found = ed.roster(mp, map_name)
        for d, _ in list(found):
            if mp.name(d) == "Titan_CHARACTER":
                for o in ed.titan_drones(mp, d):
                    if o not in [x for x, _ in found]:
                        found.append((o, {}))
        for d, _ in found:
            tid = mp.name(d).replace("_CHARACTER", "")
            if tid in types_done:
                continue
            types_done.add(tid)
            pr = mp.props(d)
            sounds.add_type(mp, tid, pr)
            sound_mp = sound_mp or mp
            bb = cd._obj(pr.get("BehaviorBlueprint"))
            if bb <= 0:
                continue
            bfull = mp.full(bb)
            cls = p.class_name(p.exports[bb - 1])
            dom = None
            for i, e in enumerate(p.exports, 1):
                if p.class_name(e) == "HtnPlanningDomain" and mp.full(i).startswith(bfull + "."):
                    dom = i
            symbols = enum_names(tg.pkg, "%s.E%sTaskSymbol" % (cls, cls[2:].replace("CharacterBlueprint", ""))) or []
            if not symbols:
                for i, e in enumerate(tg.pkg.exports, 1):
                    f = tg.pkg.full_name(i)
                    if tg.pkg.class_name(e) == "Enum" and f.startswith(cls + ".") and f.endswith("TaskSymbol"):
                        symbols = enum_names(tg.pkg, f) or []
            if dom and cls not in domains_done:
                domains_done.add(cls)
                lines.append(["domain", cls])
                lines += domain_lines(mp, dom, symbols)
            # The values: class defaults up the chain, then the blueprint's own, then
            # its override's.
            values = {}
            for c in reversed(class_chain(tg, cls)):
                di = tg.find("Default__" + c)
                if di:
                    flatten(tg, tg.props(di), "", values, None)
            flatten(mp, mp.props(bb), "", values, bfull)
            ov = cd._obj(pr.get("BehaviorBlueprintOverride"))
            if ov > 0:
                flatten(mp, mp.props(ov), "", values, mp.full(ov))
                # (Which one: its class's OnApplyCharacterBlueprintOverride runs.)
                values["BehaviorBlueprintOverride"] = mp.full(ov)
            # Its guns' values: each weapon class its blueprint names (its
            # PrimaryWeapon's as Weapon.<key>, any other's as Weapon@<its key>.<key>):
            # the TnWeaponData Escalation's AI fires it with (AiSurvivalData, else
            # AiData, else Data), over that data class's defaults, and the weapon's
            # own defaults (as <prefix>def.<key>).
            for key in sorted(k for k, v in values.items() if is_weapon(v) and "." not in k):
                weapon = values[key]
                prefix = "Weapon." if key == "PrimaryWeapon" else "Weapon@%s." % key
                defaults, wv, data = weapon_values(mp, shared, tg, weapon)
                values.update({prefix + k: v for k, v in wv.items()})
                values.update({prefix + "def." + k: v for k, v in defaults.items()})
                values[prefix + "class"] = weapon.split(".")[-1]
                if data:
                    values[prefix + "data"] = data
                else:
                    print("  ! no data for", tid, key, weapon)
                # A projectile gun's round: its first fire mode's projectile class's
                # data and mesh, the same way (as <prefix>proj.<key>).
                proj = wv.get("WeaponProjectiles[0]")
                if proj and proj != "None":
                    pv, pdata = projectile_values(mp, shared, tg, proj, prefix + "proj.")
                    values.update(pv)
                    if not pdata:
                        print("  ! no projectile data for", tid, key, proj)
            # Its chassis (its robot form: its speeds, mass, eye height, melee set,
            # as Chassis.<key>) and its melee set's attacks (as Melee.<key>).
            cb = cd._struct_props(p, pr["CharacterBlueprint"]) if "CharacterBlueprint" in pr else {}
            chassis = cd._obj(cb.get("ChassisType"))
            if chassis > 0:
                cpr = mp.props(chassis)
                flatten(mp, cpr, "Chassis.", values, mp.full(chassis))
                forms = [cpr]
                # One that starts in its other form (StartingForm TF_Alt: the
                # Swarmers' insects, the jets, the drones) moves and fights as that
                # form: its AltFormBlueprint's values over the robot's, its melee
                # set and its blueprints first.
                alt = cd._obj(cpr.get("AltFormBlueprint"))
                if str(cb.get("StartingForm")) == "TF_Alt" and alt > 0:
                    apr = mp.props(alt)
                    flatten(mp, apr, "Chassis.", values, mp.full(alt))
                    values["StartingForm"] = "TF_Alt"
                    forms.insert(0, apr)
                # The types that change form as they fight (the Snipers onto their
                # perches, the jets into robots): the form they don't start in as
                # Other.<key> (its mesh, anim tree and sets, collision, speeds...).
                if cls in TRANSFORMING and alt > 0:
                    if str(cb.get("StartingForm")) == "TF_Alt":
                        flatten(mp, cpr, "Other.", values, mp.full(chassis))
                        values["Other"] = mp.full(chassis)
                    else:
                        flatten(mp, mp.props(alt), "Other.", values, mp.full(alt))
                        values["Other"] = mp.full(alt)
                ms = next((cd._obj(f.get("MeleeSet")) for f in forms if cd._obj(f.get("MeleeSet")) > 0), 0)
                if ms > 0:
                    flatten(mp, mp.props(ms), "Melee.", values, mp.full(ms))
                # Its momentum blueprint and its acrobatics manager's (shared
                # blueprints, not its own: under Chassis.<key>. too).
                for key in ("MomentumBlueprint", "AcrobaticsManagerBlueprint"):
                    o = next((cd._obj(f.get(key)) for f in forms if cd._obj(f.get(key)) > 0), 0)
                    if o > 0:
                        values["Chassis." + key] = mp.full(o)
                        flatten(mp, mp.props(o), "Chassis.%s." % key, values, mp.full(o))
            goal = values.get("_GoalTaskSymbol", "0")
            lines.append(["type", tid, cls, cls if dom else "-", goal])
            for k in sorted(values):
                lines.append(["prop", k, values[k]])
    # The animations' projectiles' values (their data and mesh, as a gun's round's).
    for c in sorted(notified):
        pv, pdata = projectile_values(notified[c], shared, tg, c, "")
        if not pdata:
            print("  ! no data for the notifies' projectile", c)
        short = c.split(".")[-1]
        for k in sorted(pv):
            lines.append(["projectile", short, k, pv[k]])
    for e in ENUMS:
        vals = enum_names(tg.pkg, e) or []
        lines.append(["enum", e] + vals)
    out = os.path.join(assets, "models", "enemies", "ai.txt")
    os.makedirs(os.path.dirname(out), exist_ok=True)
    with open(out, "w") as fh:
        fh.write("# The enemies' AI: their planning domains and behaviour values (tools/build_ai.py)\n")
        for r in lines:
            fh.write("\t".join(str(x) for x in r) + "\n")
    print(len(lines), "lines ->", out, "domains:", sorted(domains_done))
    out = os.path.join(assets, "models", "enemies", "ai_sounds.txt")
    sl = sounds.lines(sound_mp)
    with open(out, "w") as fh:
        fh.write("# The enemies' sounds: their sound sets and dialog rules (tools/build_ai.py)\n")
        for r in sl:
            fh.write("\t".join(str(x) for x in r) + "\n")
    print(len(sl), "lines ->", out, "sets:", len(sounds.sets), "dialog events:", len(sounds.events))


if __name__ == "__main__":
    main()
