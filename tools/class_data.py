"""A class's characters as the game builds them, read from its data.

A multiplayer character is a set of parts on one skeleton (TR_Skeletons_p.
RB_Iconic_SKEL, shared by every class): the TransCustomization.ini data providers
list each class's parts and armor sets (the Infiltrator's are the "Scout"
specialty's, the Destroyer's "Leader", the Titan's "Soldier", the Scientist's
"Scientist"), and each part names its TnMeshPart objects (PartPaths). Those are
cooked into every multiplayer map; MP_Array_BASE_m.xxx has them all. A TnMeshPart is

  * a skeletal mesh following the robot's skeleton (SkeletalMeshAttachment,
    UseParentAnimComponent): the head, chest, legs, upper arms and shoulder caps,
    forearms. A forearm part appends its character's arm animations
    (AnimSetToAppend: <name>_Arm_ROBO_ANIM, Jazz_Arm_ROBO_ANIM...), a shoulder cap
    its shoulder ones;
  * or a static mesh hung on sockets (StaticMeshAttachment on every socket whose
    name starts with AttachmentSocket): the wheels ("Wheel": WheelSocket_L on a
    forearm, WheelSocket_L_LegLo behind a knee...). IsVehiclePart: the vehicle's
    wheel sockets take it too; IsRobotPart false: only they do. A jet's wings are
    two of these, the robot's (RB_Airraid_PART_Wing: RB_Airraid_STAT_Wing,
    standing up on its back) and the jet's (VH_Airraid_PART_Wing, IsRobotPart
    false: VH_Airraid_STAT_Wing, the jet's own wing); a wheel is one, both's.

The chest decides the vehicle (ImpliesAltPath: the character's VehicleForm, a
TnCarFormBlueprint or TnTruckFormBlueprint): its mesh set (FormMeshSet: the
vehicle's skeletal mesh and its MeshScale), its animation sets (AnimSets: the
vehicle's own clips; RobotAnimSets: the robot's halves of the transformations), its
visual wheels, handling and sounds. A chest can disable another part type
(DisablePart: Snarl's chest, Wheels).

A car is a rigid body (its PhysicsAsset, VH_Car_PHYSSYS, has one body, on the
skeleton's root bone C_Reference_XR): physics, not the clips, places that bone, so
whatever a clip does with it (the sets' animators left it at different heights)
the car stays where it is, on its wheels or hovering.

The clips' notifies time what the clips can't: TnAnimNotify_ToggleHidden in the
transformations (when the robot vanishes into the car and the car appears, and
the other way round), TnAnimNotify_ScaleBone in a forearm set's
ADD_Weapon_MirrorReverse_ARM (the forearm pieces shrinking away and growing back
as the gun moves to the other arm; each lerps bones from StartValue to EndValue
over RampTime, 1 when not given) and on the car's gun bones, and the
transformations' sounds: HmAnimNotify_Sound plays a cue (BL_TRANSFORM.
BREAKDOWN_BOT2VEH...), HmAnimNotify_SoundEvent an event that the class's sound
event set (its robot definition's SoundEventSet) gives a cue (Bumblebee's).

A vehicle sounds as its form's HmSoundEventSet says (SoundEventSet: VEH_SPORT_01
for Jazz's and Sideswipe's cars, VEH_SPORT_02, VEH_MUSCLE_01, VEH_DINOBOT for
Snarl and Slug, VEH_JEEP_01, VEH_TRUCK_01 and VEH_TRUCK_02 for the trucks): each of
the events its audio component plays (Auto_Engine_Gear_1_OnLoad, Auto_Boost_Start...)
has a cue there, or none; its DeathSound is a cue. Which events it plays at all is
its audio component's (AudioComp, an HmPlayerVehicleAudioComponent: its engine's
loops, boost, jumps and landings, over the class defaults: audio_events): the
walking Dinobots' play none of the engine's, jumps' or landings' (Grimlock only
his boost's start). Its own looping clips may play sounds too (VEHICLE_LOOP_CLIPS:
Snarl's footsteps, tail and growls, HmAnimNotify_Sound on the Snarl_MP_VEH_ANIM
clips), and a walker's jump and landing clips theirs (its stamp as it lands).

The weapon its vehicles carry is its multiplayer character data's (TransGame.xxx's
TR_MPPlayerCharacterData_p.<specialty>_PCD_MP, a TnPlayerCharacterData: its
VehicleWeapons, unique ids of TransWeapons.ini's weapons): the Scout's cars'
AssaultRifleVehicle (the car guns), the Leader's trucks' RocketVehicle, the
Soldier's tanks' TankCannon, the Scientist's jets' HomingRocketVehicle.

The class's robot is its chassis' robot definition (TR_Custom<...>_ROBODEF, a
TnTransformer): its size (RobotMeshMergeScale, Collision), speeds, animation sets
(a later one overriding an earlier), melee set (its MELEE_WeaponAttack's clip),
camera set and sound event set. The camera set (Car_MP_CAMSET, Truck_MP_CAMSET)
names its strategies on foot, driving and hovering (camera_set): how far out each
orbits and where it aims, some with their own for a camera state a vehicle form
puts the player in (FormPCS: TnPCS_Snarl, TnPCS_Slug, TnPCS_G1Optimus).

`read(cooked_dir, transgame_dir, class)` returns all of it as plain data (see the
end of the file); `python class_data.py <CookedPC> <TransGame> [<class>]` prints it.
"""
import json
import os
import struct
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.path.join(HERE, "ue3"))
sys.path.insert(0, HERE)

from ue3pkg import Package, Reader  # noqa: E402
from props import read_props  # noqa: E402

MP_PACKAGE = "MP_Array_BASE_m.xxx"
# The multiplayer package every match loads besides its map: Jazz's transformation
# clips (TR_Iconic_Car_ANM_p.Jazz_ROBO_ANIM_NEW) are cooked there, not into the maps.
SHARED_PACKAGE = "mp_shared.xxx"
# The classes, by the demo's name for them: each one's specialty (the ini's
# SpecialtyRestriction) and robot definition (its chassis' ObjectPath: its
# animation sets, size, speeds, camera and sound events).
CLASSES = {
    "infiltrator": ("Scout", "TR_CustomScout_ROBODEF_p.CustomScout_ROBODEF"),
    "destroyer": ("Leader", "TR_CustomLeader_ROBODEF_p.CustomLeader_ROBODEF"),
    "titan": ("Soldier", "TR_CustomSoldier_ROBODEF_p.CustomSoldier_ROBODEF"),
    "scientist": ("Scientist", "TR_CustomScientist_ROBODEF_p.CustomScientist_ROBODEF"),
}
SPECIALTY = "Scout"
# The package with the classes' multiplayer character data, and its objects' names.
PCD_PACKAGE = "TransGame.xxx"
PCD_NAME = "TR_MPPlayerCharacterData_p.{}_PCD_MP"
# The Infiltrator's own sound events (its pawn's HmSoundEventSet), which a clip's
# HmAnimNotify_SoundEvent plays when its robot definition names none.
CLASS_SOUND_EVENTS = "SoundEvents.Multiplayer.CHR_MP_INFILTRATOR_1"
# A vehicle's looping clips (driving, hovering and its directions: the ones
# car.rs keeps playing), whose sound notifies are read.
VEHICLE_LOOP_CLIPS = ["Nav_Boost_VEH", "Nav_Hover_VEH", "Nav_Hover_Pose_VEH", "Nav_Hover_F_VEH",
                      "Nav_Hover_B_VEH", "Nav_Hover_L_VEH", "Nav_Hover_R_VEH"]

# The footstep component's sound event for each footstep Type (EFootstepType, as
# HmFootstepComponent.GetFootStepSoundFromPhysicalMaterialAndFootstepType takes
# them without a physical material: Default__HmFootstepComponent's DefaultWalkSound,
# DefaultScuffSound...). The class's sound event set gives each its cue.
FOOTSTEP_EVENTS = {"kFootstepWalk": "FS_DEFAULT_WALK", "kScuff": "FS_DEFAULT_SCUFF", "kJump": "FS_DEFAULT_JUMP",
                   "kLand": "FS_DEFAULT_LAND", "kFootstepRun": "FS_DEFAULT_RUN", "kFootstepJog": "FS_DEFAULT_JOG",
                   "kFootstepCrouchRun": "FS_DEFAULT_CROUCH_RUN", "kFootstepCrouchWalk": "FS_DEFAULT_CROUCH_WALK",
                   "kFootstepCrouchJog": "FS_DEFAULT_CROUCH_JOG", "kCrouchScuff": "FS_DEFAULT_CROUCH_SCUFF",
                   "kHardLand": "FS_DEFAULT_HARD_LAND"}

# The vehicle audio components' class defaults (HM_Engine.xxx's
# Default__HmVehicleAudioComponent; Default__HmPlayerVehicleAudioComponent names no
# sound event), for a component that leaves them as they are: a property it gives,
# even None, stands instead.
AUDIO_PACKAGE = "HM_Engine.xxx"
AUDIO_DEFAULTS = ("Default__HmVehicleAudioComponent", "Default__HmPlayerVehicleAudioComponent")


def _props(pkg, idx):
    """An export's tagged properties (the longest clean parse near its start)."""
    e = pkg.exports[idx - 1]
    best = {}
    for st in (4, 0, 8, 12, 16):
        try:
            pr = read_props(pkg, Reader(pkg.data, e["offset"] + st), e["offset"] + e["size"])
        except Exception:
            continue
        if pr and len(pr) > len(best) and all(k.replace("[", "").replace("]", "").replace("_", "").isalnum()
                                               for k in pr):
            best = pr
    return best


def _obj(v):
    return v[1] if isinstance(v, tuple) and v and v[0] == "obj" else 0


def _str(v):
    """A StrProperty's text."""
    if isinstance(v, tuple) and v[0] == "StrProperty":
        d = v[1]
        n = struct.unpack_from("<i", d, 0)[0]
        if n > 0:
            return d[4:4 + n].decode("latin-1").rstrip("\x00")
    return ""


def _strs(v):
    """An array of strings' texts."""
    if not (isinstance(v, tuple) and v[0] == "array") or len(v[1]) < 4:
        return []
    d = v[1]
    out, at = [], 4
    for _ in range(struct.unpack_from("<i", d, 0)[0]):
        n = struct.unpack_from("<i", d, at)[0]
        at += 4
        text = d[at:at - 2 * n].decode("utf-16-le") if n < 0 else d[at:at + n].decode("latin-1")
        at += -2 * n if n < 0 else n
        out.append(text.rstrip("\x00"))
    return out


def _objs(v):
    if not (isinstance(v, tuple) and v[0] == "array"):
        return []
    d = v[1]
    n = struct.unpack_from("<i", d, 0)[0]
    return list(struct.unpack_from("<%di" % n, d, 4)) if len(d) == 4 + 4 * n else []


def _structs(pkg, v):
    if not (isinstance(v, tuple) and v[0] == "array") or len(v[1]) < 4:
        return []
    r = Reader(v[1])
    n = r.i32()
    out = []
    for _ in range(n):
        try:
            out.append(read_props(pkg, r, len(v[1])))
        except Exception:
            break
    return out


def _vec(v, default=(0.0, 0.0, 0.0)):
    if isinstance(v, tuple) and v[0] == "struct" and len(v[2]) == 12:
        return tuple(round(x, 5) for x in struct.unpack("<3f", v[2]))
    return default


def _rot(v):
    if isinstance(v, tuple) and v[0] == "struct" and len(v[2]) == 12:
        return struct.unpack("<3i", v[2])
    return (0, 0, 0)


def _color(v):
    if isinstance(v, tuple) and v[0] == "struct" and len(v[2]) >= 16:
        return tuple(round(x, 5) for x in struct.unpack("<4f", v[2][:16]))
    return None


class Mp:
    """The multiplayer map package (or another cooked package), its exports by full
    name."""

    def __init__(self, cooked, package=MP_PACKAGE):
        self.pkg = Package(os.path.join(cooked, package))
        p = self.pkg
        self.by_full = {p.full_name(i + 1): i + 1 for i in range(len(p.exports))}
        # Unreal's names ignore case, and the ini spells some paths differently from
        # the packages (TR_Starscream_ROBO_p for TR_StarScream_ROBO_p).
        self.by_lower = {}
        for full, i in self.by_full.items():
            self.by_lower.setdefault(full.lower(), i)
        self.by_name = {}
        for i, e in enumerate(p.exports):
            self.by_name.setdefault(e["name"], []).append(i + 1)
        self._cache = {}

    def props(self, idx):
        if idx not in self._cache:
            self._cache[idx] = _props(self.pkg, idx) if idx > 0 else {}
        return self._cache[idx]

    def name(self, idx):
        return self.pkg.obj_name(idx) if idx else None

    def full(self, idx):
        return self.pkg.full_name(idx) if idx > 0 else self.pkg.obj_name(idx)

    def find(self, full):
        return self.by_full.get(full) or self.by_lower.get(full.lower(), 0)

    def sockets(self, mesh_idx):
        """A skeletal mesh's sockets: (name, bone, location, rotation, scale), in
        Unreal's units and axes (rotation in 65536ths of a turn)."""
        out = []
        for s in _objs(self.props(mesh_idx).get("Sockets")):
            if s <= 0:
                continue
            sp = self.props(s)
            out.append({"name": str(sp.get("SocketName", "")), "bone": str(sp.get("BoneName", "")),
                        "location": _vec(sp.get("RelativeLocation")), "rotation": _rot(sp.get("RelativeRotation")),
                        "scale": _vec(sp.get("RelativeScale"), (1.0, 1.0, 1.0))})
        return out

    def material(self, name):
        """A material instance (by object name; where several share it, the first: see
        mesh_material) as material_at gives it."""
        return self.material_at(self.by_name.get(name, [0])[0])

    def mesh_material(self, mesh, name):
        """The material `name` as the mesh `mesh` (a SkeletalMesh or StaticMesh, by
        object name) uses it: where several of the package's materials share the
        name (VH_Interior_MAT: Swoop's own material, and the other vehicles'
        instances of Interior_DIF__TEXSET), the one the mesh's own data (its
        materials list) refers to."""
        candidates = self.by_name.get(name, [])
        if len(candidates) > 1:
            for m in self.by_name.get(mesh, []):
                e = self.pkg.exports[m - 1]
                if self.pkg.class_name(e) not in ("SkeletalMesh", "StaticMesh"):
                    continue
                data = self.pkg.data[e["offset"]:e["offset"] + e["size"]]
                hits = [c for c in candidates if struct.pack("<i", c) in data]
                if len(hits) == 1:
                    return self.material_at(hits[0])
        return self.material(name)

    def material_at(self, idx):
        """A material instance (export `idx`): its texture set (or its textures, by
        parameter), its customization colours (Cust_Color_A, Cust_COLOR_B, linear)
        and metallic offset, following its Parent chain (the most derived value
        wins); and a plain material's (no parameters: the chain ends in a Material)
        textures by what they feed (plain_textures)."""
        info = {"texset": None, "textures": {}, "texture_paths": {}, "vectors": {}, "scalars": {}, "chain": [],
                "plain": {}}
        seen = set()
        cur = idx
        while cur and cur not in seen:
            seen.add(cur)
            info["chain"].append(self.name(cur))
            pr = self.props(cur)
            for el in _structs(self.pkg, pr.get("VectorParameterValues")):
                c = _color(el.get("ParameterValue"))
                if c is not None:
                    info["vectors"].setdefault(str(el.get("ParameterName")), c)
            for el in _structs(self.pkg, pr.get("ScalarParameterValues")):
                v = el.get("ParameterValue")
                if isinstance(v, float):
                    info["scalars"].setdefault(str(el.get("ParameterName")), round(v, 5))
            # A material of separate pictures instead of a texture set (Soundwave's
            # wheels: Iconic_Trucks_MAT's Diffuse Map, Normal_XY_Map, CustA_Ruff_CustB
            # and GlowA_Coeff_GlowB): each parameter's texture, by name.
            for el in _structs(self.pkg, pr.get("TextureParameterValues")):
                if _obj(el.get("ParameterValue")):
                    param = str(el.get("ParameterName"))
                    info["textures"].setdefault(param, self.name(_obj(el.get("ParameterValue"))))
                    info["texture_paths"].setdefault(param, self.pkg.full_name(_obj(el.get("ParameterValue"))))
            for el in _structs(self.pkg, pr.get("TextureSetParameterValues")):
                if str(el.get("ParameterName")) == "TextureSet" and _obj(el.get("ParameterValue")) and not info["texset"]:
                    info["texset"] = self.name(_obj(el.get("ParameterValue")))
            if self.pkg.class_name(self.pkg.exports[cur - 1]) == "Material":
                info["plain"] = self.plain_textures(cur)
            parent = _obj(pr.get("Parent"))
            if parent <= 0:
                if parent < 0:
                    info["chain"].append(self.name(parent))
                break
            cur = parent
        return info

    def plain_textures(self, idx):
        """A Material's (export `idx`) textures by what they feed: {"diffuse",
        "normal", "specular", "emissive": the full name of the first texture its
        DiffuseColor, Normal, SpecularColor or EmissiveColor input reaches through
        its expressions} (InteriorAlt_Energon_MAT's Interior_Color, Interior_Norm and
        Interior_Emiss; the Swarmer's VH_SwarmerCut_CLR, _NORM and _SPEC)."""
        pr = self.props(idx)
        out = {}

        def textures(v):
            # (Breadth first from the input: the nearest texture first.)
            found, seen = [], set()
            sub = read_props(self.pkg, Reader(v[2]), len(v[2])) if isinstance(v, tuple) and v[0] == "struct" else {}
            todo = [_obj(sub.get("Expression"))]
            while todo:
                e = todo.pop(0)
                if e <= 0 or e in seen:
                    continue
                seen.add(e)
                ep = self.props(e)
                t = _obj(ep.get("Texture"))
                if "TextureSample" in self.pkg.class_name(self.pkg.exports[e - 1]) and t:
                    if self.pkg.full_name(t) not in found:
                        found.append(self.pkg.full_name(t))
                    continue
                for k in ("A", "B", "Input", "Alpha"):
                    w = ep.get(k)
                    if isinstance(w, tuple) and w[0] == "struct":
                        todo.append(_obj(read_props(self.pkg, Reader(w[2]), len(w[2])).get("Expression")))
            return found

        for role, key in (("diffuse", "DiffuseColor"), ("normal", "Normal"), ("specular", "SpecularColor"),
                          ("emissive", "EmissiveColor")):
            try:
                found = textures(pr.get(key))
            except Exception:
                found = []
            # (The specular's own texture rather than the diffuse it multiplies: the
            # Swarmer's VH_SwarmerCut_SPEC.)
            if role == "specular":
                found = [t for t in found if t != out.get("diffuse")] + found
            if found:
                out[role] = found[0]
        return out

    def sequence(self, anim_set, name):
        """The AnimSequence `name` of the AnimSet `anim_set` (export index, or 0)."""
        for s in self.by_name.get(anim_set, []):
            for q in _objs(self.props(s).get("Sequences")):
                if q > 0 and str(self.props(q).get("SequenceName")) == name:
                    return q
        return 0

    def names(self, v):
        """An array of FNames (an ArrayProperty of NameProperty)."""
        if not (isinstance(v, tuple) and v[0] == "array") or len(v[1]) < 4:
            return []
        d = v[1]
        out = []
        for k in range(struct.unpack_from("<i", d, 0)[0]):
            idx, num = struct.unpack_from("<ii", d, 4 + 8 * k)
            out.append(self.pkg.names[idx] if num == 0 else f"{self.pkg.names[idx]}_{num - 1}")
        return out

    def notifies(self, seq):
        """A sequence's notifies: (time, class, properties), in the sequence's order."""
        out = []
        for el in _structs(self.pkg, self.props(seq).get("Notifies")):
            n = _obj(el.get("Notify"))
            if n <= 0:
                continue
            cls = self.pkg.full_name(n).split(".")[-1].rsplit("_", 1)[0]
            out.append((float(el.get("Time", 0.0)), cls, self.props(n)))
        return out

    def clip_steps(self, seq):
        """A clip's footsteps: (seconds as it plays, sound event), from its
        AnimNotify_Footstep notifies, each its Type's default event of the footstep
        component (HmFootstepComponent's DefaultLandSound...: FOOTSTEP_EVENTS) that
        its owner's sound event set gives a cue. The game plays one only where the
        foot finds the ground below it (FindPhysicalMaterial): a robot landing as he
        comes out of his jet, or jumping into his car."""
        rate = self.props(seq).get("RateScale")
        rate = float(rate) if isinstance(rate, float) and rate > 0 else 1.0
        out = []
        for t, cls, pr in self.notifies(seq):
            if cls == "AnimNotify_Footstep":
                event = FOOTSTEP_EVENTS.get(str(pr.get("Type", "kFootstepWalk")))
                if event:
                    out.append((round(t / rate, 4), event))
        return out

    def toggle_hidden(self, seq, hides=False):
        """When a transformation clip hides its model (`hides`: a
        TnAnimNotify_ToggleHidden whose Option is Toggle_Hide) or shows it (one
        without), in seconds of the clip: the first of that kind (a few vehicles'
        folding up first hide the vehicle at its start, then show it: Onslaught's, and
        the Titan's Brawl and Hardshell), else the first of any; None without one."""
        toggles = [(round(t, 4), str(pr.get("Option", "")) == "Toggle_Hide")
                   for t, cls, pr in self.notifies(seq) if cls == "TnAnimNotify_ToggleHidden"]
        return next((t for t, hide in toggles if hide == hides), toggles[0][0] if toggles else None)

    def scale_bones(self, seq):
        """A clip's TnAnimNotify_ScaleBone notifies: each bone's scale lerps from
        `start` to `end` over `ramp` seconds from `time` (HmValueLerper; values not
        given are 1, a ramp not given is instant)."""
        out = []
        for t, cls, pr in self.notifies(seq):
            if cls != "TnAnimNotify_ScaleBone":
                continue
            lp = self.props(_obj(pr.get("Lerper")))
            num = lambda k, d: float(lp[k]) if isinstance(lp.get(k), float) else d  # noqa: E731
            for bone in self.names(pr.get("BoneNames")):
                out.append({"bone": bone, "time": round(t, 4), "start": round(num("StartValue", 1.0), 4),
                            "end": round(num("EndValue", 1.0), 4), "ramp": round(num("RampTime", 0.0), 4)})
        return out

    def sound_events(self, set_idx):
        """An HmSoundEventSet's events: {event name: cue (full name)}, the events it
        gives no cue left out."""
        out = {}
        if not set_idx:
            return out
        for el in _structs(self.pkg, self.props(set_idx).get("SoundEventSet")):
            event, cue = _obj(el.get("Event")), _obj(el.get("Sound"))
            if event and cue:
                out.setdefault(self.pkg.obj_name(event), self.pkg.full_name(cue))
        return out

    def clip_sounds(self, seq, events):
        """A clip's sounds: (seconds, cue), from its HmAnimNotify_Sound notifies (a cue)
        and HmAnimNotify_SoundEvent ones (an event, as `events` plays it). The seconds
        are as the clip plays, at its RateScale (umodel's export, and so the demo's
        clip, plays it so: the Dinobots' runs, 1.2 to 1.4 times as fast, put their
        second footfall past the end of the clip at their own time)."""
        rate = self.props(seq).get("RateScale")
        rate = float(rate) if isinstance(rate, float) and rate > 0 else 1.0
        out = []
        for t, cls, pr in self.notifies(seq):
            cue = None
            if cls == "HmAnimNotify_Sound" and _obj(pr.get("SoundCue")):
                cue = self.pkg.full_name(_obj(pr.get("SoundCue")))
            elif cls == "HmAnimNotify_SoundEvent" and _obj(pr.get("SoundEvent")):
                cue = events.get(self.pkg.obj_name(_obj(pr.get("SoundEvent"))))
            if cue:
                out.append((round(t / rate, 4), cue))
        return out

    def sequence_info(self, anim_set, name, events=None):
        """A clip's length, frame count and notifies of interest, or None (its sound
        events played as `events` gives them cues)."""
        q = self.sequence(anim_set, name)
        if not q:
            return None
        pr = self.props(q)
        # (The robot folding up and the vehicle unfolding hide their models; the
        # others show theirs.)
        hides = name.endswith(("_ToVehicle_ROBO", "_ToRobot_VEH"))
        return {"set": anim_set, "length": round(float(pr.get("SequenceLength", 0.0)), 4),
                "frames": int(pr.get("NumFrames", 0)), "toggle": self.toggle_hidden(q, hides),
                "scale_bones": self.scale_bones(q), "sounds": self.clip_sounds(q, events or {}),
                "steps": self.clip_steps(q)}


def _ini_parts(transgame, specialty=SPECIALTY):
    import build_menu
    import build_customize as bc
    cfg = build_menu.coalesced(os.path.join(transgame, "Config", "PC", "Cooked", "Coalesced.ini"))
    loc = build_menu.coalesced(os.path.join(transgame, "Localization", "Cooked", "PC", "Coalesced.int"))
    cust = bc.ini_sections(cfg["TransGame/Config/PC/Cooked/TransCustomization.ini"])
    names = bc.ini_sections(loc["TransGame/Localization/INT/TransGame.int"])

    def friendly(uid, kind):
        return bc.first(names.get(f"{uid} {kind}", {}), "FriendlyName", uid)

    def ours(f):
        return specialty in bc.first(f, "SpecialtyRestriction").split(",")

    parts = []
    for uid, f in bc.providers(cust, "TnDataProvider_Part"):
        # (Decal sets are left out: the demo draws no decals yet.)
        if not ours(f) or bc.first(f, "PartType") == "DecalSet":
            continue
        parts.append({"id": uid, "type": bc.first(f, "PartType"), "name": friendly(uid, "TnDataProvider_Part"),
                      "paths": [p for v in f.get("PartPaths", []) for p in v.split(",") if p],
                      "disable": [p for v in f.get("DisablePart", []) for p in v.split(",") if p],
                      "dlc": bc.first(f, "LockedForDLC") == "true"})
    sets = []
    for uid, f in bc.providers(cust, "TnDataProvider_ArmorSet"):
        if not ours(f):
            continue
        sets.append({"id": uid, "name": friendly(uid, "TnDataProvider_ArmorSet"),
                     "parts": bc.first(f, "PartsInSet").split(","),
                     "dlc": bc.first(f, "LockedForDLC") == "true"})
    return parts, sets


# The blueprints' class defaults (TransGame.xxx's Default__TnCarPhysicsBlueprint,
# Default__TnHoverCarSimulationBlueprint, Default__TnHoverTankSimulationBlueprint,
# Default__TnSuspensionBlueprint): what a blueprint that doesn't set a value has.
CAR_DEFAULTS = {"MaxSpeed": 500.0, "MaxAcceleration": 500.0, "JumpAngularVelocity": 2.0,
                "JumpingGravityScale": 1.0, "Mass": 1.0}
CAR_JUMP_DEFAULT = (1200.0, 0.0, 1200.0)
# Default__TnCarFormBlueprint's JumpingShakeParams and LandingShakeParams.
SHAKE_DEFAULTS = {"Intensity": 0.35, "Roll": 1.0, "RollFalloffDuration": 0.25, "ShakeFalloffDuration": 0.65}
CAR_DRIVE_DEFAULTS = {"AngularDamping": 5.0, "TurningBankValue": 20.0, "TurningBankSpeed": 0.1,
                      "AirControlTurnAcceleration": 8.0, "AirControlStrafeAcceleration": 2000.0}
HOVER_DEFAULTS = {"MaxLinearSpeed": 1500.0, "MaxLinearAcceleration": 3000.0, "DriftDuration": 0.5,
                  "JumpLinearSpeed": 1200.0, "JumpAngularSpeed": 1.0, "JumpingGravityScale": 1.0,
                  "SuspensionRadius": 200.0}
TANK_DEFAULTS = {"MaxLinearSpeed": 1500.0, "MaxBoostSpeed": 3500.0, "MaxLinearAcceleration": 3000.0,
                 "DriftDuration": 0.5, "JumpLinearSpeed": 1200.0, "JumpAngularSpeed": 1.0,
                 "SuspensionRadius": 200.0, "ChassisOffset": 0.0}
SUSPENSION_DEFAULTS = {"Stiffness": 10000.0, "Damping": 0.0, "RestingLength": 30.0, "InAirRestingLength": 75.0}
# The jets' (Default__TnHoverPlaneSimulationBlueprint, hovering, and
# Default__TnPlaneSimulationBlueprint, flying).
HOVER_PLANE_DEFAULTS = {"MaxLinearSpeed": 1500.0, "MaxLinearAcceleration": 3000.0, "DriftDuration": 0.25,
                        "VerticalDashSpeed": 0.0, "VerticalDashDuration": 0.0, "DashSpeed": 5000.0,
                        "DashDuration": 0.3, "RollDuration": 0.6, "RollLinearSpeed": 3000.0,
                        "MotionPhysicsScale": 1.0, "AllowRoll": 0.0}
PLANE_DEFAULTS = {"MaxSpeed": 1000.0, "MaxAcceleration": 1000.0, "DragCoefficient": 0.5,
                  "PitchDueToPitchValue": 27.0, "YawDueToYawValue": 16.0, "RollDueToYawValue": 77.0,
                  "ExtraRotationLerpValue": 0.1, "MaxPitchValue": 60.0, "FullPitchThreshold": 45.0,
                  "RollDuration": 0.8, "RollAngularSpeed": 8.0, "AllowRoll": 0.0}


def handling(mp, idx):
    """A vehicle physics blueprint's numbers the demo drives by (Unreal units): a
    TnCarPhysicsBlueprint's (on its wheels: its speed, acceleration and jump, its
    front wheels' radius and suspension), a TnHoverCarSimulationBlueprint's
    (hovering: its speed, acceleration, jump, drift and suspension) or a
    TnHoverTankSimulationBlueprint's (a tank, which only hovers: its speed, its speed
    boosting, acceleration, jump, drift and suspension), a
    TnHoverPlaneSimulationBlueprint's (a jet hovering: its speed, acceleration, drift,
    climb and dash) or a TnPlaneSimulationBlueprint's (a jet flying: its speed,
    acceleration and drag, and how it banks)."""
    pr = mp.props(idx)
    cls = mp.pkg.class_name(mp.pkg.exports[idx - 1]) if idx > 0 else ""
    num = lambda p, k, d: float(p[k]) if isinstance(p.get(k), (int, float)) else d  # noqa: E731
    if cls == "TnHoverPlaneSimulationBlueprint":
        out = {k: num(pr, k, d) for k, d in HOVER_PLANE_DEFAULTS.items()}
        out["kind"] = "hover_plane"
        return out
    if cls == "TnPlaneSimulationBlueprint":
        out = {k: num(pr, k, d) for k, d in PLANE_DEFAULTS.items()}
        out["kind"] = "plane"
        return out

    def suspension(p):
        sp = mp.props(_obj(p.get("SuspensionBlueprint")))
        return {k: num(sp, k, d) for k, d in SUSPENSION_DEFAULTS.items()}
    if cls == "TnHoverCarSimulationBlueprint":
        out = {k: num(pr, k, d) for k, d in HOVER_DEFAULTS.items()}
        out["kind"] = "hover"
        out["suspension"] = suspension(pr)
        return out
    if cls == "TnHoverTankSimulationBlueprint":
        out = {k: num(pr, k, d) for k, d in TANK_DEFAULTS.items()}
        out["kind"] = "tank"
        out["suspension"] = suspension(pr)
        return out
    out = {k: num(pr, k, d) for k, d in CAR_DEFAULTS.items()}
    out["kind"] = "car"
    out["jump"] = _vec(pr.get("JumpLinearVelocity"), CAR_JUMP_DEFAULT)
    wheel = mp.props(_obj(pr.get("WheelPhysicsBlueprints")))
    out["wheel_radius"] = num(wheel, "WheelRadius", 1.0)
    out["suspension"] = suspension(wheel)
    # How it drives on its wheels (TnCarSimulation, its TnWheelAssemblies' tyres):
    # its body's turning, its centre of mass and inertia, its banking into turns and
    # its air control, and each wheel's place, steering and grip.
    out.update({k: num(pr, k, d) for k, d in CAR_DRIVE_DEFAULTS.items()})
    out["inertia"] = _vec(pr.get("InertiaTensor"), (1.0, 1.0, 1.0))
    out["centre"] = _vec(pr.get("CenterOfMass"))
    out["wheels"] = []
    for key in ("WheelPhysicsBlueprints", "WheelPhysicsBlueprints[1]", "WheelPhysicsBlueprints[2]",
                "WheelPhysicsBlueprints[3]"):
        w = mp.props(_obj(pr.get(key)))
        out["wheels"].append({"at": _vec(w.get("LocalPosition")), "steer": num(w, "MaxSteeringAngle", 0.0),
                              "grip": num(w, "TireFrictionCoefficient", 0.0),
                              "radius": num(w, "WheelRadius", 1.0), "suspension": suspension(w)})
    return out


# The strategies of a camera set (HmCameraStrategySet.Strategies, by type) the demo
# follows: on foot, driving and hovering.
CAMERA_TYPES = {"robot": "TnOverTheShoulderCameraStrategyType", "drive": "TnDrivingCameraStrategyType",
                "hover": "TnHoveringCameraStrategyType", "fly": "TnFlyingCameraStrategyType"}


def _struct_props(pkg, v):
    return read_props(pkg, Reader(v[2]), len(v[2])) if isinstance(v, tuple) and v[0] == "struct" else {}


def _offset_curve(pkg, v):
    """A TnCameraOffsetCurve: its offsets (ahead, side, up; Unreal units) at the
    bottom, middle and top of its pitch range (degrees: both 0 for one offset at any
    pitch)."""
    pr = _struct_props(pkg, v)
    rng = _struct_props(pkg, pr.get("PitchRange"))
    return {"offsets": [_vec(pr.get(k)) for k in ("Offsets", "Offsets[1]", "Offsets[2]")],
            "pitch": (float(rng.get("Min", 0.0)), float(rng.get("Max", 0.0)))}


def _range(pkg, v, default):
    """A FloatRange struct's (Min, Max), or `default`."""
    r = _struct_props(pkg, v)
    return (float(r.get("Min", default[0])), float(r.get("Max", default[1]))) if r else default


# The jets' flying camera's defaults (Default__TnFlyingFollowCameraBehavior): the
# anchor over the jet, the pitch range and how the view's turn is smoothed.
FLYING_FOLLOW_DEFAULTS = {"AnchorOffset": (0.0, 0.0, 220.0), "PitchRange": (-60.0, 60.0),
                          "MaxYawVelocity": 540.0, "MaxYawAcceleration": 720.0, "YawDamping": 4.0,
                          "MaxPitchVelocity": 360.0, "MaxPitchAcceleration": 720.0, "PitchDamping": 4.0,
                          "RollDueToYawVelocity": 0.05}


def camera_strategy(mp, idx):
    """A camera strategy's orbit (its TnLocationOffsetCameraBehavior's distance, and
    by player camera state; or a jet's HmLocationOffsetCameraBehavior's offset
    behind, its LocationOffset), its aim's offsets by the view's pitch (its
    TnScreenSpaceOffsetByPitchCameraBehavior's curve, and by state), its anchor's
    offset over the pawn (HmOffsetAnchorPointRelativeToActorCameraBehavior, and by
    state), Unreal units, its field of view (TnFovCameraBehavior, degrees: None for the demo's
    own), the view's pitch range (its TnOrbitRotationCameraBehavior's, or a
    flying jet's TnFlyingFollowCameraBehavior's, degrees: None when it gives none)
    and that flying camera's settings (`follow`: FLYING_FOLLOW_DEFAULTS' keys). A
    state entry ([states], value) applies while all its states are on."""
    out = {"distance": 0.0, "distances": [], "curve": None, "curves": [], "anchor": (0.0, 0.0, 0.0),
           "anchors": [], "lift": 0.0, "lifts": [], "fov": None, "pitch_range": None, "follow": None}

    def states(s):
        return sorted(mp.name(o) for o in _objs(s.get("PlayerCameraStates")) if o)
    for b in _objs(mp.props(idx).get("Behaviors")):
        if b <= 0:
            continue
        kind = mp.pkg.class_name(mp.pkg.exports[b - 1])
        pr = mp.props(b)
        if kind == "TnLocationOffsetCameraBehavior":
            out["distance"] = float(pr.get("DefaultOrbitDistance", 0.0))
            out["distances"] = [(states(s), float(s.get("OrbitDistance", 0.0)))
                                for s in _structs(mp.pkg, pr.get("OrbitDistancesByPCS"))]
        elif kind == "HmLocationOffsetCameraBehavior":
            # (Its offset's height too: `lift`, and by state.)
            offset = _vec(pr.get("LocationOffset"))
            out["distance"], out["lift"] = -offset[0], offset[2]
            by_state = [(states(s), _vec(s.get("LocationOffset")))
                        for s in _structs(mp.pkg, pr.get("LocationOffsetsByPCS"))]
            out["distances"] = [(st, -o[0]) for st, o in by_state]
            out["lifts"] = [(st, o[2]) for st, o in by_state]
        elif kind == "TnScreenSpaceOffsetByPitchCameraBehavior":
            out["curve"] = _offset_curve(mp.pkg, pr.get("DefaultOffsetCurve"))
            out["curves"] = [(states(s), _offset_curve(mp.pkg, s.get("OffsetCurve")))
                             for s in _structs(mp.pkg, pr.get("OffsetCurvesByPCS"))]
        elif kind == "HmOffsetAnchorPointRelativeToActorCameraBehavior":
            out["anchor"] = _vec(pr.get("Offset"))
            out["anchors"] = [(states(s), _vec(s.get("Offset")))
                              for s in _structs(mp.pkg, pr.get("OffsetsByPCS"))]
        elif kind == "TnFovCameraBehavior":
            # (A field of view the strategy leaves unset is the behavior's own, 75:
            # Default__TnFovCameraBehavior's DefaultFOV, which Escalation's jets'
            # and tanks' keep.)
            fov = _struct_props(mp.pkg, pr.get("DefaultFOV"))
            out["fov"] = float(fov.get("Fov", 75.0))
        elif kind == "TnOrbitRotationCameraBehavior":
            look = _struct_props(mp.pkg, pr.get("DefaultLookSpeed"))
            if "PitchRange" in look:
                out["pitch_range"] = _range(mp.pkg, look["PitchRange"], (-60.0, 60.0))
        elif kind == "TnFlyingFollowCameraBehavior":
            follow = dict(FLYING_FOLLOW_DEFAULTS)
            for k, d in FLYING_FOLLOW_DEFAULTS.items():
                if k == "AnchorOffset" and k in pr:
                    follow[k] = _vec(pr[k])
                elif k == "PitchRange" and k in pr:
                    follow[k] = _range(mp.pkg, pr[k], d)
                elif isinstance(pr.get(k), (int, float)):
                    follow[k] = float(pr[k])
            out["follow"] = follow
            out["pitch_range"] = follow["PitchRange"]
            out["anchor"] = follow["AnchorOffset"]
    return out


def camera_set(mp, name):
    """A camera set's (a robot definition's Cameras) strategies on foot, driving and
    hovering (CAMERA_TYPES): camera_strategy's, or None."""
    out = dict.fromkeys(CAMERA_TYPES)
    idx = mp.by_name.get(name, [0])[0] if name else 0
    if not idx:
        print("  ! camera set not found:", name)
        return out
    for s in _structs(mp.pkg, mp.props(idx).get("Strategies")):
        kind = mp.name(_obj(s.get("Type")))
        strategy = _obj(s.get("Strategy"))
        for k, t in CAMERA_TYPES.items():
            if kind == t and out[k] is None and strategy > 0:
                out[k] = camera_strategy(mp, strategy)
    return out


def _curve_detail(pkg, v):
    """A TnCameraOffsetCurve in full: _offset_curve's and its SmoothTime."""
    out = _offset_curve(pkg, v)
    out["secs"] = float(_struct_props(pkg, v).get("SmoothTime", 0.0))
    return out


def camera_detail(mp, idx):
    """A vehicle's camera strategy in full, as main.rs's vehicle camera follows it
    (build_class.py's vehicle_cam_lines): how long the camera takes to blend into
    it (TransitionTime) and its motion blur (CamPostProcessSettings); its field of
    view (TnFovCameraBehavior) and orbit distance (TnLocationOffsetCameraBehavior)
    by camera state with their smoothing; its anchor over the vehicle
    (HmOffsetAnchorPointRelativeToActorCameraBehavior, Unreal units, in the view's
    axes when OffsetUsesCameraRotation); its offsets by the view's pitch
    (TnScreenSpaceOffsetByPitchCameraBehavior) by state; the view's pitch range and,
    for a TnDrivingOrbitRotationCameraBehavior, how fast its turn follows the
    vehicle's (MatchRotationSpeed: 0 for a camera the player turns); how long its
    orbit's turn is smoothed over (HmOrbitSmootherCameraBehavior); its roll by
    state (TnDutchRollCameraBehavior, degrees); and its wiggle by state
    (TnWiggler3CameraBehavior's profiles: yaw, pitch and roll amplitudes, degrees,
    as HmWiggler3Data.GetRotationAdjust takes them; how many new spots a second
    each heads for; and the seconds a change to the profile eases over). A state entry is ([states], ...), in the game's order: the first
    whose states are all on applies, else the default ([] last)."""
    pr = mp.props(idx)
    out = {"blend": float(pr.get("TransitionTime", 0.0)), "blur": 0.0, "fov": [], "anchor": 0.0,
           "anchor_turns": False, "distance": [], "distance_secs": (0.0, 0.0), "curve": [],
           "pitch_range": (-60.0, 60.0), "match": 0.0, "smooth": 0.0, "roll": [], "wiggle": []}
    post = _struct_props(mp.pkg, pr.get("CamPostProcessSettings"))
    if post.get("bEnableMotionBlur") is True:
        out["blur"] = float(post.get("MotionBlur_Amount", 0.5))

    def states(s):
        return sorted(mp.name(o) for o in _objs(s.get("PlayerCameraStates")) if o)

    def wiggle(p):
        p = _struct_props(mp.pkg, p) if isinstance(p, tuple) else p
        amp = [float(p.get(k, 0.0)) for k in ("RotationAmplitude", "RotationAmplitude[1]", "RotationAmplitude[2]")]
        hz = [float(p.get(k, 0.0)) for k in ("RotationFrequency", "RotationFrequency[1]", "RotationFrequency[2]")]
        return amp, hz, float(p.get("TransitionTime", 0.0))
    for b in _objs(pr.get("Behaviors")):
        if b <= 0:
            continue
        kind = mp.pkg.class_name(mp.pkg.exports[b - 1])
        bp = mp.props(b)
        if kind == "TnFovCameraBehavior":
            for s in _structs(mp.pkg, bp.get("FOVsByPCS")):
                f = _struct_props(mp.pkg, s.get("Settings"))
                out["fov"].append((states(s), float(f.get("Fov", 75.0)), float(f.get("SmoothTime", 0.4))))
            f = _struct_props(mp.pkg, bp.get("DefaultFOV")) or {"Fov": 75.0, "SmoothTime": 0.4}
            out["fov"].append(([], float(f.get("Fov", 75.0)), float(f.get("SmoothTime", 0.4))))
        elif kind == "HmOffsetAnchorPointRelativeToActorCameraBehavior":
            out["anchor"] = _vec(bp.get("Offset"))[2]
            out["anchor_turns"] = bp.get("OffsetUsesCameraRotation") is True
        elif kind in ("TnOrbitRotationCameraBehavior", "TnDrivingOrbitRotationCameraBehavior"):
            look = _struct_props(mp.pkg, bp.get("DefaultLookSpeed"))
            out["pitch_range"] = _range(mp.pkg, look.get("PitchRange"), (-60.0, 60.0))
            if kind == "TnDrivingOrbitRotationCameraBehavior":
                out["match"] = float(bp.get("MatchRotationSpeed", 0.0))
        elif kind == "TnLocationOffsetCameraBehavior":
            for s in _structs(mp.pkg, bp.get("OrbitDistancesByPCS")):
                out["distance"].append((states(s), float(s.get("OrbitDistance", 0.0))))
            out["distance"].append(([], float(bp.get("DefaultOrbitDistance", 0.0))))
            out["distance_secs"] = (float(bp.get("PCSTransitionInTime", 0.0)), float(bp.get("PCSTransitionOutTime", 0.0)))
        elif kind == "TnScreenSpaceOffsetByPitchCameraBehavior":
            for s in _structs(mp.pkg, bp.get("OffsetCurvesByPCS")):
                out["curve"].append((states(s), _curve_detail(mp.pkg, s.get("OffsetCurve"))))
            out["curve"].append(([], _curve_detail(mp.pkg, bp.get("DefaultOffsetCurve"))))
        elif kind == "HmOrbitSmootherCameraBehavior":
            out["smooth"] = float(bp.get("SmoothTime", 0.0))
        elif kind == "TnDutchRollCameraBehavior":
            for s in _structs(mp.pkg, bp.get("DutchRollByPCS")):
                r = _struct_props(mp.pkg, s.get("Settings"))
                out["roll"].append((states(s), float(r.get("RollDeg", 0.0)), float(r.get("SmoothTime", 0.0))))
            out["roll"].append(([], 0.0, float(bp.get("SmoothTime", 0.0))))
        elif kind == "TnWiggler3CameraBehavior":
            for s in _structs(mp.pkg, bp.get("ProfilesByPCS")):
                out["wiggle"].append((states(s),) + wiggle(_struct_props(mp.pkg, s.get("WiggleProfile"))))
            out["wiggle"].append(([],) + wiggle(_struct_props(mp.pkg, bp.get("DefaultProfile"))))
    return out


# Escalation's cameras. Its characters are the campaign's (Optimus Prime, Warpath,
# Bumblebee, Starscream...: the robot definitions in the ESC_ maps' packages), each
# with its own camera set (its Cameras: Truck_Optimus_CAMSET, Tank_Megatron_CAMSET,
# Car_Bumblebee_CAMSET, Plane_Starscream_CAMSET...), not the multiplayer classes'
# (Truck_MP_CAMSET...): the Destroyer's trucks, for one, hover 7 m behind the camera's
# anchor in Escalation, 9 m in multiplayer. A vehicle form one of them changes into
# (its AltFormBlueprint) takes that character's set (vehicle_camera_set); the class's
# others the set most of Escalation's characters of its kind use
# (ESCALATION_CLASS_CAMERAS: the trucks of Optimus Prime, Ratchet and Soundwave, the
# tanks of Warpath and Megatron, the cars of Bumblebee, Cliffjumper and Wheeljack,
# the jets of Starscream and Shockwave). A form with a camera state of its own in
# the multiplayer set (its FormPCS: the DLC's Dinobots, G1 Optimus, Vortex) keeps
# that one, made for it.
ESCALATION_PACKAGES = ("ESC_Gladiator_BASE_m.xxx", "ESC_Temple_BASE_m.xxx", "ESC_Ark_BASE_m.xxx",
                       "ESC_Iacon_BASE_m.xxx")
ESCALATION_CLASS_CAMERAS = {"infiltrator": "Car_Bumblebee_CAMSET", "destroyer": "Truck_Optimus_CAMSET",
                            "titan": "Tank_Megatron_CAMSET", "scientist": "Plane_Starscream_CAMSET"}
_ESCALATION = {}


def escalation_cameras(cooked):
    """Escalation's characters' camera sets: {the vehicle form each changes into: its
    camera set's name}, and {a camera set's name: the package (an Mp) with it}."""
    if cooked not in _ESCALATION:
        by_form, sets = {}, {}
        for pkg in ESCALATION_PACKAGES:
            if not os.path.exists(os.path.join(cooked, pkg)):
                continue
            mp = Mp(cooked, pkg)
            for i, e in enumerate(mp.pkg.exports):
                if mp.pkg.class_name(e) != "TnTransformer":
                    continue
                pr = mp.props(i + 1)
                cams, alt = _obj(pr.get("Cameras")), _obj(pr.get("AltFormBlueprint"))
                if cams and alt:
                    by_form.setdefault(mp.name(alt), mp.name(cams))
            for name in set(by_form.values()) | set(ESCALATION_CLASS_CAMERAS.values()):
                if name not in sets and mp.by_name.get(name):
                    sets[name] = mp
        _ESCALATION[cooked] = (by_form, sets)
    return _ESCALATION[cooked]


def vehicle_camera_set(cooked, cls, form, pcs):
    """A vehicle form's camera in Escalation (see ESCALATION_CLASS_CAMERAS): its camera
    set's name and the package with it, or None for one that keeps the class's
    multiplayer set (a form with a camera state of its own, `pcs`)."""
    if pcs:
        return None
    by_form, sets = escalation_cameras(cooked)
    name = by_form.get(form) or ESCALATION_CLASS_CAMERAS.get(cls)
    return (name, sets[name]) if name in sets else None


def camera_details(mp, name):
    """A camera set's driving and hovering strategies (CAMERA_TYPES' "drive" and
    "hover"), in full: camera_detail's, or None."""
    out = {"drive": None, "hover": None}
    idx = mp.by_name.get(name, [0])[0] if name else 0
    for s in _structs(mp.pkg, mp.props(idx).get("Strategies")) if idx else []:
        kind = mp.name(_obj(s.get("Type")))
        strategy = _obj(s.get("Strategy"))
        for k in out:
            if kind == CAMERA_TYPES[k] and out[k] is None and strategy > 0:
                out[k] = camera_detail(mp, strategy)
    return out


def melee_attack(mp, melee_set):
    """A melee set's (a TnMeleeSet) attack with a weapon (MELEE_WeaponAttack: the one
    every class plays, a sword's swing): its animation set and clip, or None."""
    idx = mp.by_name.get(melee_set, [0])[0] if melee_set else 0
    if not idx:
        return None
    pr = mp.props(idx)
    anim_set = mp.name(_obj(pr.get("MeleeAnimSet")))
    for a in _structs(mp.pkg, pr.get("Attacks")):
        if str(a.get("MeleeType")) == "MELEE_WeaponAttack" and a.get("AnimName"):
            return anim_set, str(a.get("AnimName"))
    return None


def robot_info(mp, robodef):
    """A class's robot as its definition (a TnTransformer) gives it: its animation
    sets (a later one overriding an earlier), its size (RobotMeshMergeScale, the
    parts drawn at that size, and its collision: half height and radius, Unreal
    units), its speeds, camera set, acrobatics, melee set (and its sword swing's
    clip) and sound events."""
    pr = mp.props(mp.find(robodef))
    col = pr.get("Collision")
    cp = read_props(mp.pkg, Reader(col[2]), len(col[2])) if isinstance(col, tuple) and col[0] == "struct" else {}
    num = lambda k, d=0.0: float(pr[k]) if isinstance(pr.get(k), (int, float)) else d  # noqa: E731
    melee = mp.name(_obj(pr.get("MeleeSet")))
    return {"anims": [mp.name(o) for o in _objs(pr.get("AnimSets")) if o],
            "scale": round(num("RobotMeshMergeScale", 1.0), 4),
            "height": float(cp.get("CollisionHeight", 0.0)), "radius": float(cp.get("CollisionRadius", 0.0)),
            "ground_speed": num("BaseGroundSpeed"), "accel": num("AccelRate"), "air_speed": num("AirSpeed"),
            "air_control": num("AirControl"), "cameras": mp.name(_obj(pr.get("Cameras"))),
            "acrobatics": mp.name(_obj(pr.get("AcrobaticsManagerBlueprint"))),
            "melee": melee, "melee_clip": melee_attack(mp, melee), "size": str(pr.get("Size", "")),
            "sound_events": mp.full(_obj(pr.get("SoundEventSet"))) if _obj(pr.get("SoundEventSet")) else None}


def physics_hull(mp, pa):
    """The part of a physics asset that strikes the world: its root bone's body's
    shapes (the convex hulls, and the boxes, capsules and spheres but the triggers
    and those with bNoRBCollision: the jets' and tanks' big boxes are triggers), as
    the box round them in the bone's space ((min, max): Unreal units, X ahead, Y to
    the side, Z up), or None. (The rigid body a vehicle drives and flies as collides
    with the world by these: a jet's rotor or wings are no part of it.)"""
    if pa <= 0:
        return None
    pts = []
    bodies = [b for b in _objs(mp.props(pa).get("BodySetup")) if b > 0]
    for b in bodies[:1]:
        pr = mp.props(b)
        agg = _struct_props(mp.pkg, pr.get("AggGeom"))
        for e in _structs(mp.pkg, agg.get("ConvexElems")):
            v = e.get("VertexData")
            if isinstance(v, tuple) and v[0] == "array" and len(v[1]) >= 4:
                n = struct.unpack_from("<i", v[1])[0]
                if n > 0 and len(v[1]) >= 4 + 12 * n:
                    pts += [struct.unpack_from("<3f", v[1], 4 + 12 * k) for k in range(n)]
        for e in _structs(mp.pkg, agg.get("BoxElems")):
            if e.get("bTrigger") or e.get("bNoRBCollision"):
                continue
            tm = _matrix_of(e.get("TM"))
            hx, hy, hz = (float(e.get(k, 0.0)) * 0.5 for k in ("X", "Y", "Z"))
            for sx in (-1, 1):
                for sy in (-1, 1):
                    for sz in (-1, 1):
                        pts.append(_tm_point(tm, (sx * hx, sy * hy, sz * hz)))
        for key, rad in (("SphylElems", True), ("SphereElems", False)):
            for e in _structs(mp.pkg, agg.get(key)):
                if e.get("bNoRBCollision"):
                    continue
                tm = _matrix_of(e.get("TM"))
                r = float(e.get("Radius", 0.0))
                half = float(e.get("Length", 0.0)) * 0.5 if rad else 0.0
                for end in (-half, half):
                    c = _tm_point(tm, (0.0, 0.0, end))
                    pts += [(c[0] + dx, c[1] + dy, c[2] + dz) for dx in (-r, r) for dy in (-r, r) for dz in (-r, r)]
    if not pts:
        return None
    lo = [min(p[i] for p in pts) for i in range(3)]
    hi = [max(p[i] for p in pts) for i in range(3)]
    return lo, hi


def _matrix_of(v):
    """A Matrix struct property's rows (UE's row vectors: x, y, z axes and origin), or
    the identity."""
    if isinstance(v, tuple) and v[0] == "struct" and len(v[2]) >= 64:
        f = struct.unpack_from("<16f", v[2])
        return [f[0:4], f[4:8], f[8:12], f[12:16]]
    return [(1, 0, 0, 0), (0, 1, 0, 0), (0, 0, 1, 0), (0, 0, 0, 1)]


def _tm_point(m, p):
    return tuple(p[0] * m[0][i] + p[1] * m[1][i] + p[2] * m[2][i] + m[3][i] for i in range(3))


def anim_set_clips(packages, name):
    """An animation set's clips, from the first of `packages` (Mp) that has it:
    {clip name: its RateScale (1 when not given)}, empty when none has it."""
    for pkg in packages:
        if pkg is None:
            continue
        out = {}
        for s in pkg.by_name.get(name, []):
            if pkg.pkg.class_name(pkg.pkg.exports[s - 1]) != "AnimSet":
                continue
            for q in _objs(pkg.props(s).get("Sequences")):
                if q > 0:
                    pr = pkg.props(q)
                    rate = pr.get("RateScale")
                    out[str(pr.get("SequenceName"))] = float(rate) if isinstance(rate, float) else 1.0
        if out:
            return out
    return {}


def vehicle_weapons(cooked, specialty):
    """The weapons a class's vehicles carry (its multiplayer character data's
    VehicleWeapons: the Leader's ["RocketVehicle"]), [] where it has none."""
    if not os.path.exists(os.path.join(cooked, PCD_PACKAGE)):
        return []
    mp = Mp(cooked, PCD_PACKAGE)
    data = mp.props(mp.find(PCD_NAME.format(specialty))).get("Data")
    if not (isinstance(data, tuple) and data[0] == "struct"):
        return []
    return _strs(read_props(mp.pkg, Reader(data[2]), len(data[2])).get("VehicleWeapons"))


def audio_events(mp, comp, defaults):
    """The sound events a vehicle's audio component (an HmPlayerVehicleAudioComponent,
    export `comp` of `mp`) plays: those its properties name, its engine's loops and
    one-shots and its landings in the structs of its arrays too, and the class
    defaults' (`defaults`: {property: event}) for the properties it leaves out (one
    it gives as None plays nothing). Sorted names; None without a component."""
    if not comp:
        return None
    pr = mp.props(comp)
    out = set()

    def events_in(v):
        if isinstance(v, tuple) and v[0] == "obj":
            if v[1] and _class_of(mp.pkg, v[1]).endswith("SoundEvent"):
                out.add(mp.name(v[1]))
        elif isinstance(v, tuple) and v[0] == "array":
            try:
                items = _structs(mp.pkg, v)
            except Exception:
                items = []
            for it in items:
                for x in it.values():
                    events_in(x)
        elif isinstance(v, tuple) and v[0] == "struct":
            for x in _struct_props(mp.pkg, v).values():
                events_in(x)

    for key, v in pr.items():
        events_in(v)
    out.update(e for key, e in defaults.items() if key not in pr)
    return sorted(out)


def _class_of(pkg, idx):
    """An object's class name, an import's or an export's."""
    if idx < 0:
        return str(pkg.imports[-idx - 1]["class"])
    return pkg.class_name(pkg.exports[idx - 1])


_AUDIO_DEFAULTS = {}


def audio_defaults(cooked=None):
    """The vehicle audio components' class defaults' sound events ({property: event
    name}), read once from AUDIO_PACKAGE (by `read`'s CookedPC)."""
    if cooked is not None and not _AUDIO_DEFAULTS:
        path = os.path.join(cooked, AUDIO_PACKAGE)
        if os.path.exists(path):
            hm = Mp(cooked, AUDIO_PACKAGE)
            for name in AUDIO_DEFAULTS:
                for i in hm.by_name.get(name, []):
                    for key, v in hm.props(i).items():
                        if isinstance(v, tuple) and v[0] == "obj" and v[1] \
                                and _class_of(hm.pkg, v[1]).endswith("SoundEvent"):
                            _AUDIO_DEFAULTS.setdefault(key, hm.name(v[1]))
    return _AUDIO_DEFAULTS


def read(cooked, transgame, cls="infiltrator"):
    mp = Mp(cooked)
    audio_defaults(cooked)
    specialty, robodef = CLASSES[cls]
    parts, sets = _ini_parts(transgame, specialty)
    robot = robot_info(mp, robodef)
    robot["camera"] = camera_set(mp, robot["cameras"])
    robot["camera_detail"] = camera_details(mp, robot["cameras"])
    meshes = {}
    vehicles = {}

    def body_bones(pa):
        if pa <= 0:
            return []
        return [str(mp.props(b).get("BoneName", "")) for b in _objs(mp.props(pa).get("BodySetup")) if b > 0]

    def hull(pa):
        return physics_hull(mp, pa)

    def vehicle(path):
        if path in vehicles:
            return vehicles[path]
        idx = mp.find(path)
        if not idx:
            print("  ! vehicle form not found:", path)
            vehicles[path] = None
            return None
        pr = mp.props(idx)
        ms = _obj(pr.get("FormMeshSet"))
        mpr = mp.props(ms) if ms else {}
        # (A few forms name their mesh themselves, without a mesh set: the
        # Scientist's Vortex, Sharpshot and Kickback, SkelMesh.)
        base = _obj(mpr.get("BaseMesh")) if ms else _obj(pr.get("SkelMesh"))
        wheel_parts = [mp.full(o) for o in _objs(mpr.get("MeshParts")) if o]
        fx = {}
        for key in ("JumpFX", "HoverFX", "NitroFX", "BoostFx", "LowLandingFX", "HighLandingFX", "LowLandingHoverFX",
                    "HighLandingHoverFX", "LandFX", "DashFX"):
            fx[key] = [(str(el.get("Socket", "")), mp.name(_obj(el.get("ParticleSystemTemplate"))))
                       for el in _structs(mp.pkg, pr.get(key))]
        v = {
            "form": path,
            "mesh": mp.name(base),
            "scale": round(float(mpr.get("MeshScale", 1.0)), 5),
            # The mesh scales the offsets of the clips it plays by this (Snarl's
            # multiplayer dino, 0.6: its clips are the full-size Dinobot's).
            "translation_scale": round(float(mp.props(base).get("TranslationScale", 1.0)), 5)
            if base > 0 else 1.0,
            "wheel_parts": wheel_parts,
            "sockets": mp.sockets(base) if base > 0 else [],
            "robot_anims": [mp.name(o) for o in _objs(pr.get("RobotAnimSets")) if o],
            "anims": [mp.name(o) for o in _objs(pr.get("AnimSets")) if o],
            "visual_wheels": [{"bone": str(el.get("BoneName", "")), "radius": float(el.get("Radius", 0.0)),
                               "control": str(el.get("SkelControlName", ""))}
                              for el in _structs(mp.pkg, pr.get("VisualWheels"))],
            "chassis_offset": float(pr.get("ChassisOffset", 0.0)),
            # A car's or truck's landings are its low ones under this long in the air
            # (TnCarFormBlueprint's LowLandingAirTimeThreshold, its class default 1 s;
            # Optimus Prime's, Optimus G1's and Ultra Magnus's 0.25).
            "low_landing_air": float(pr.get("LowLandingAirTimeThreshold", 1.0)),
            # The camera shakes its jumps and landings start (JumpingShakeParams,
            # LandingShakeParams: TnCarForm.OnBeginJump, OnLandComplete), over the
            # class default's.
            "shakes": {k: {**SHAKE_DEFAULTS, **{n: float(x) for n, x in _struct_props(mp.pkg, pr.get(key)).items()
                                                if isinstance(x, (int, float))}}
                       for k, key in (("jump", "JumpingShakeParams"), ("land", "LandingShakeParams"))},
            # The physics asset's bodies (VH_Car_PHYSSYS: one, on C_Reference_XR):
            # physics places those bones, whatever the clips do with them.
            "body_bones": body_bones(_obj(pr.get("PhysicsAsset"))),
            # What of its body strikes the world (physics_hull), as the box round it.
            "hull": hull(_obj(pr.get("PhysicsAsset"))),
            "sound_set": mp.name(_obj(pr.get("SoundEventSet"))),
            "death_sound": mp.name(_obj(pr.get("DeathSound"))),
            # Its sounds: {event: cue}, and the cue it dies with.
            "sound_events": mp.sound_events(_obj(pr.get("SoundEventSet"))),
            # The events its audio component plays (audio_events): None where it has
            # no component.
            "audio_events": audio_events(mp, _obj(pr.get("AudioComp")), audio_defaults()),
            "death_cue": mp.pkg.full_name(_obj(pr.get("DeathSound"))) if _obj(pr.get("DeathSound")) else None,
            "car_physics": [mp.name(o) for o in _objs(pr.get("CarBlueprints")) if o],
            "hover_physics": [mp.name(o) for o in _objs(pr.get("HoverBlueprints")) if o],
            # A tank's (TnTankFormBlueprint: it only hovers, its own way) and the
            # bone its cannon aims with.
            "kind": mp.pkg.class_name(mp.pkg.exports[idx - 1]),
            "tank_physics": [mp.name(o) for o in _objs(pr.get("Blueprints")) if o],
            # A jet's (TnPlaneFormBlueprint): hovering and flying, and how far below
            # the view it hovers pitched (PitchOffsetFromCamera, degrees).
            "plane_physics": {"hover": [mp.name(o) for o in _objs(pr.get("HoverVehicleBlueprints")) if o],
                              "fly": [mp.name(o) for o in _objs(pr.get("FlyingVehicleBlueprints")) if o]},
            "pitch_offset": float(pr.get("PitchOffsetFromCamera", 0.0)),
            # The camera state it puts the player in (FormPCS: TnPCS_Slug...), which
            # the camera strategies give their own distances and offsets.
            "pcs": mp.name(_obj(pr.get("FormPCS"))) if _obj(pr.get("FormPCS")) else None,
            # Its animation tree (VEH_Car_ANIMTREE, VEH_Tank_ANIMTREE, and the walking
            # Dinobots' Dinobot_ANIMTREE).
            "anim_tree": mp.name(_obj(pr.get("AnimTree"))),
            # Its handling, by blueprint (each the multiplayer one first, then the
            # campaign's): see handling().
            "handling": {mp.name(o): handling(mp, o) for o in
                         _objs(pr.get("CarBlueprints")) + _objs(pr.get("HoverBlueprints"))
                         + _objs(pr.get("Blueprints")) + _objs(pr.get("HoverVehicleBlueprints"))
                         + _objs(pr.get("FlyingVehicleBlueprints")) if o},
            "fx": fx,
            # The clips its form plays as it jumps and lands (TnCarFormBlueprint's and
            # TnTankFormBlueprint's BoostJumpAnim, HoverJumpAnim, BoostLandAnim,
            # HoverLandAnim and HoverIdleLandAnim: the walking Dinobots'), None where
            # it has none.
            "jump_anims": {k: (str(pr.get(key)) if str(pr.get(key, "None")) != "None" else None)
                           for k, key in (("boost_jump", "BoostJumpAnim"), ("hover_jump", "HoverJumpAnim"),
                                          ("boost_land", "BoostLandAnim"), ("hover_land", "HoverLandAnim"),
                                          ("idle_land", "HoverIdleLandAnim"))},
        }
        vehicles[path] = v
        return v

    for part in parts:
        part["meshes"], part["wheel"] = [], None
        part["anims"], part["vehicle"] = [], None
        for path in part["paths"]:
            idx = mp.find(path)
            if not idx:
                print("  ! part object not found:", path)
                continue
            pr = mp.props(idx)
            sk = _obj(pr.get("SkeletalMeshAttachment"))
            st = _obj(pr.get("StaticMeshAttachment"))
            if sk:
                name = mp.name(sk)
                part["meshes"].append(name)
                if name not in meshes:
                    meshes[name] = {"package": mp.full(sk).split(".")[0], "sockets": mp.sockets(sk) if sk > 0 else []}
            if st:
                # (The robot's mesh, "mesh", where IsRobotPart isn't false; the
                # vehicle's, "vehicle_mesh", where IsVehiclePart is true.)
                wheel = part["wheel"] or {"mesh": None, "vehicle_mesh": None}
                if pr.get("IsRobotPart") is not False:
                    wheel["mesh"] = mp.name(st)
                    wheel["package"] = mp.full(st).split(".")[0]
                if pr.get("IsVehiclePart"):
                    wheel["vehicle_mesh"] = mp.name(st)
                wheel["socket"] = _str(pr.get("AttachmentSocket")) or wheel.get("socket") or "Wheel"
                wheel["vehicle_part"] = wheel["vehicle_mesh"] is not None
                part["wheel"] = wheel
            anim = _obj(pr.get("AnimSetToAppend"))
            if anim:
                part["anims"].append(mp.name(anim))
            alt = _str(pr.get("ImpliesAltPath"))
            if alt:
                part["vehicle"] = alt
                vehicle(alt)

    # The clips' timings. A name is looked up through a mesh's sets from the last to
    # the first (UE3's FindAnimSequence: a later set overrides an earlier one), in
    # the map, then in mp_shared.
    shared = Mp(cooked, SHARED_PACKAGE) if os.path.exists(os.path.join(cooked, SHARED_PACKAGE)) else None
    events_set = robot["sound_events"] or CLASS_SOUND_EVENTS
    class_events = mp.sound_events(mp.find(events_set))
    if not class_events:
        print("  ! sound event set not found:", events_set)

    def clip(sets, name, quiet=False):
        for s in reversed(sets):
            for pkg in (mp, shared):
                if pkg is not None and pkg.sequence(s, name):
                    return pkg.sequence_info(s, name, class_events)
        if not quiet:
            print("  ! clip not found:", name, "in", sets)
        return None

    for v in vehicles.values():
        if v:
            v["clips"] = {"robot_to_vehicle": clip(v["robot_anims"], "Transform_ToVehicle_ROBO"),
                          "robot_to_robot": clip(v["robot_anims"], "Transform_ToRobot_ROBO"),
                          "car_to_vehicle": clip(v["anims"], "Transform_ToVehicle_VEH"),
                          "car_to_robot": clip(v["anims"], "Transform_ToRobot_VEH")}
            # The sounds its own looping clips play, by clip name (Snarl's footsteps
            # as he walks and runs, his tail and growls standing; the cars' play none).
            v["clip_sounds"] = {}
            # (And a walker's jump and landing clips': the Dinobots' stamps as they
            # land, Nav_LandHard_VEH's twice.)
            for name in VEHICLE_LOOP_CLIPS + sorted({c for c in v["jump_anims"].values() if c}):
                info = clip(v["anims"], name, quiet=True)
                if info and info["sounds"]:
                    v["clip_sounds"][name] = info["sounds"]
    arm_sets = {}
    for part in parts:
        if part["type"] != "ArmLo":
            continue
        for s in part["anims"]:
            if s not in arm_sets:
                info = clip([s], "ADD_Weapon_MirrorReverse_ARM")
                arm_sets[s] = {"switch": info["scale_bones"] if info else []}
    # Which clips the class's animation sets (and the Infiltrator's, which robot.glb
    # is made from) have: build_class.py plays the class's own where they differ.
    scout = robot_info(mp, CLASSES["infiltrator"][1])
    scout_anims = scout["anims"]
    anim_clips, anim_rates = {}, {}
    melee_set = (robot.get("melee_clip") or (None, None))[0]
    for s in dict.fromkeys(robot["anims"] + scout_anims + ([melee_set] if melee_set else [])):
        clips = anim_set_clips((mp, shared), s)
        anim_clips[s] = sorted(clips)
        for c, rate in clips.items():
            if rate != 1.0:
                anim_rates[(s, c)] = rate
    # Each vehicle's camera, Escalation's (vehicle_camera_set): its set's strategies,
    # in place of the class's.
    for path, v in vehicles.items():
        found = vehicle_camera_set(cooked, cls, path.split(".")[-1], v.get("pcs")) if v else None
        if found:
            name, emp = found
            v["camera_set"] = name
            v["camera"] = camera_set(emp, name)
            v["camera_detail"] = camera_details(emp, name)
    return {"class": cls, "parts": parts, "sets": sets, "meshes": meshes, "arm_sets": arm_sets,
            "vehicles": {k: v for k, v in vehicles.items() if v}, "robot": robot,
            "class_events": class_events, "scout_anims": scout_anims, "scout_melee": scout["melee_clip"],
            "anim_clips": anim_clips,
            "anim_rates": anim_rates, "vehicle_weapons": vehicle_weapons(cooked, specialty), "mp": mp}


if __name__ == "__main__":
    if len(sys.argv) not in (3, 4):
        sys.exit(__doc__)
    d = read(*sys.argv[1:])
    d.pop("mp")
    d["anim_rates"] = {f"{s}.{c}": r for (s, c), r in d["anim_rates"].items()}
    print(json.dumps(d, indent=1))
