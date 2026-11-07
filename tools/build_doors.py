"""Build Escalation's locked doors and the unlock consoles that open them, from each
map's BASE package (its consoles, its Kismet and the Matinees its purchases play), for
src/doors.rs.

The maps are built with every area open (tools/build_map.py): the doors where the
purchases leave them, the consoles in their bind pose. This puts the doors back where
the level has them as a match starts, and gives the consoles their animations:

  * models/esc_console.glb (and its textures in models/esc_console/): the wall
    console (TnInteractCurrencyReceptacle's AP_EscalationConsoleWall_SKEL) with its
    bones and its three clips (EscalationConsoleWall_ANIM: Activate_Console, whose
    position is the progress bar, Equip_Console and UnEquip_Console), in its materials
    as the maps draw it (its door screen a flip book), and an outline round its body
    (its materials' EnableOutlineHighlight: ConsoleOutline, lit by doors.rs).
  * models/<map>_doors.glb: every door the map's purchases move, in its place as the
    level starts (each scene a door, in its own frame; doors.rs puts it at the
    actor's place): the static ones (sliding doors, the start barriers' pipes, the
    cover blocks, the force fields) as the map's model draws them, light maps and
    all; the skeletal ones (the Optimus doors, the covers, the dropship doors) with
    their bones and their clips, one scene for each kind. Their materials are the
    map model's, their textures its files (Oblivion's, which its build leaves out,
    copied in from Ancients' build).
  * levels/<map>_doors.bin: their collision, in switchable groups (the format below):
    each door's where it stands closed and where it ends up open, and the player
    blockers (HmPlayerOnlyBlockingVolume) and dynamic blocking volumes the purchases
    switch off, for movement only (the map's collision has none of them).
  * levels/<map>_doors.txt: the consoles, the doors and what each purchase does (the
    format below).
  * the traps (TRAPS: Ignition's Flame Jets and GunShip Support, Oblivion's Combustion
    Chamber, Downfall's Defensive Sentries, Ancients' Electric Fences): their floor
    consoles (TnInteractCurrencyReceptacleMultiUse, bought again and again, a cooldown
    after each purchase: models/esc_console_floor.glb, built as the wall console is,
    its textures with the wall console's in models/esc_console/), and in
    levels/<map>_doors.txt what their purchases switch (their flames' and lightning's
    emitters, pain volumes and lights: emitter, pain and light lines), the sentries they wake
    (sentry lines), the steam vents and the gunship (skeletal doors their Matinees
    play: the gunship's turn track, turn lines), their Matinees' toggle and
    brightness tracks, and the sounds of the map's AUDIO package's Kismet they set
    off (Oblivion's combustion chamber's blast).
  * models/<map>.glb and levels/<map>_col.bin, in place: the doors and the wall
    consoles out of the map's scene (their nodes stay in the file, unused), and the
    doors' collision where they stood open out of the map's (it is in their groups).
    Run again, it finds them by their nodes all the same.

usage: python build_doors.py <TransGame\\CookedPC> <foc_esc\\assets> <map> <export folder> [<map> <export folder> ...]

The export folders are the maps' umodel exports, as build_map.py takes them (their
BASE package's meshes and animation sets); the maps must have been built (Ancients
before Oblivion: Oblivion's doors take Ancients' materials).

levels/<map>_doors.txt, tab-separated, positions in Bevy metres, rotations (x, y, z, w):

  prompt <key> <text>
      A console prompt (TransGame.int's [ContextualCommands]: `i its deposit, `c its
      price).
  console <id> <x y z> <rotation> <stand x y z> <cost> <deposit> <prompt> <total> <kind> <cooldown>
      A console: its place, the spot its user stands at (UseConditions' interact
      spot), its price and deposit, its prompt's key (TransGame.int's
      [ContextualCommands]), the name of the total it pays into (shared by the
      consoles that name it; "-": its own), its kind (wall: a door's, bought once;
      floor: a trap's) and a trap's cooldown (seconds: its total cleared as it is
      bought, it comes back on that long after, ReenableAfterCooldown; 0 for a wall
      console).
  door <id> <x y z> <scene> <closed group> <open group>
      A static door as the level starts: where the actor is, its scene in
      models/<map>_doors.glb (the mesh turned and sized as the level has it, in the
      actor's frame: doors.rs places the scene at the actor's place), and its
      collision groups in levels/<map>_doors.bin where it stands closed and where it
      ends up open (-1: none).
  skeletal <id> <x y z> <rotation> <scale x y z> <scene> <closed group> <open group> <clip> <time>
      A skeletal door (its scene shared by its kind), placed by its place, turn and
      scale, and its pose as the level starts (a clip of the glb and its time).
  clip <scene> <name> <animation> <seconds>
      A skeletal kind's clips: their names, their animations in the glb, their lengths.
  rider <base> <actor>
      An actor hard-attached to a door: it moves with it.
  collapse <id> <x y z>
      A door that is a prop a purchase brings down (a door line of its own: its
      scene and its collision where it stands, none once it is down): Downfall's
      scaffolds (TnStaticDestructibleActor, DES_ASH_ESCBeam1_DSYS), which the
      Courtyard's and the Doors' consoles' Kismet brings down
      (SeqAct_TnActivateDestructible: the HmKismetDestructionTrigger out of their
      intact state) as the navigation's toggles open the ways through them. The
      place is where it comes down: the middle of its foot.
  crumble <id> <state> <x y z> <rotation>
      What it does as it comes down: its blueprint's state the Kismet's trigger
      brings it to (named "<blueprint>:<n>"), set off at the actor's place and turn
      (src/gibs.rs's set_off, as Oblivion's barrel's: tools/build_barrels.py) - the
      scaffolds' broken state throws twelve pieces of the beam
      (HmDebrisDestructionEffectFactory: Beam1_pt1_STAT ... Beam1_pt4_STAT, the mesh
      systems tools/build_weapon_fx.py makes of them) with its falling bits
      (Catwalk_Debris_FX).
  dsys <state> ...
      That state's effects, as tools/build_death_sets.py writes a destructible's
      (particle, sound, gibs: its HmDebrisDestructionEffectFactory chunks, their
      places in the actor's frame, Unreal's units), once for each state.
  spot <id> <x y z> [<half x y z>]
      An actor that only has a place (where a Matinee's sounds play); a line of
      sound (HmAmbientSoundLineEmitter) runs from its place less to its place plus
      the half given, and is heard from its point nearest the listener.
  volume <id> <group>
      A blocking volume a purchase switches: its group.
  matinee <id> <seconds> <rate> <looping 0/1>
  move <matinee> <actor> <key> ...
      A move track's keys, each "t dx dy dz mode ax ay az lx ly lz": the actor's
      offset from where it starts (world space), the key's interpolation (L linear,
      C constant, else the curve, Hermite with the arrive and leave tangents). The
      actor is a door, or a blocking volume (moved out of the way).
  scale <matinee> <actor> <key> ...
      A DrawScale track's keys, each "t scale mode arrive leave": the actor's size
      (the AirStrike station's force field collapsing).
  sound <matinee> <t> <cue> <actor> [<actor> ...]
  anim <matinee> <actor> <start> <clip> <start offset> <end offset> <rate> <looping 0/1> <reverse 0/1>
      An animation track's key: from its start time the actor plays the clip.
  do <timeline> <t> <op> [<arg> ...]
      What a console's purchase does (timeline: the console) and what gameplay
      start does (timeline: start), at its time: play|reverse|pause|resume|stop
      <matinee>; on|off|toggle <target> ... (consoles, volumes, spawn clusters, the
      traps' emitters, pain volumes and lights); outline_on|outline_off <console> ...;
      hide|unhide <actor> ...; transform|untransform <actor> ...; sound <cue>
      <actor> ... (a looping cue plays on until) sound_stop <cue> <actor> ...;
      wake|sleep <sentry> ... (SeqAct_TnAiSentrySetDormant); collapse <prop> ...
      (SeqAct_TnActivateDestructible: a collapse line's prop brought down).
  cluster <id> <spawn point> ...
      A spawn cluster's spawn points: their numbers among the enemy spawn points of
      levels/<map>_points.txt.
  emitter <id> <system> <x y z> <x axis> <y axis> <scale x y z> <on 0/1> <lit 0/1>
      A trap's particle effect (an Emitter its purchase or Matinee switches): its
      system (fx/weapons_fx.txt's, TRAP_SYSTEMS), its place, its X and Y axes, its
      size (DrawScale3D times DrawScale), whether it is on as the level starts and
      whether the game lights it (its particle component accepts lights and has a
      light environment: bAcceptsLights and LightEnvironment; the effects the game
      spawns take no light).
  pain <id> <damage a second> <players 0/1> <living 0/1> <kind> <pain 0/1> <plane> ...
      A trap's pain volume (PhysicsVolume, DynamicPhysicsVolume: off till its
      purchase turns it on): DamagePerSec, done to what is in it, players too or
      only the AI (bAffectPlayer), only the living (bAffectsLivingPawns); its damage
      type (flame: TnDamageTypeFlameZone, setting them on fire; explosion:
      TnDamageTypeEnvironmentExplosion), bPainCausing; its brush as planes, each
      "nx ny nz d" (a point p is in it where n.p <= d for every plane).
  light <id> <x y z> <r g b> <brightness> <radius> <falloff exponent> <on 0/1>
      A trap's light (PointLightToggleable): its colour, Brightness, Radius
      (metres) and FalloffExponent, and whether it is on as the level starts.
  sentry <id> <kind> <x y z> <rotation> <scale> <spawn time> [<actor> <bone> <offset x y z> <rotation>]
      A sentry a trap wakes (its factory: SeqAct_TnPawnFactory, SeqAct_ActorFactory),
      spawned as gameplay starts at its time (its spawn point's place and turn) and
      put to sleep by gameplay start: its kind (rocket: RocketInteract_Sentry_TURRETDEF,
      Downfall's turrets; mg: MGInteract_TURRETDEF, the gunship's guns) and its mesh's
      size; one attached to an actor's bone (SeqAct_AttachToActor) rides on it, at its
      offset and turn from the bone (the bone's frame, as the glb's skeleton has it).
  turn <matinee> <actor> <rotation> <key> ...
      A move track's rotation keys, each "t roll pitch yaw mode ax ay az lx ly lz"
      (degrees; their curve as a move track's), relative to the actor's turn as it
      starts (its Rotation, Bevy axes, given): the actor turned by base · key ·
      base⁻¹ (the gunship circling).
  toggle <matinee> <t> <on|off|toggle> <actor> ...
      A Matinee's toggle track key: it switches the traps' emitters as it is passed.
  brightness <matinee> <actor> <key> ...
      A light's Brightness track's keys, each "t brightness mode arrive leave".

levels/<map>_doors.bin, little-endian, as levels/<map>_col.bin packs triangles:
"FOCD", u32 version (1), origin (3 f32) and step (f32), u32 group count; then for each
group: u16 name length and its UTF-8 name, u8 flags (1: solid, 2: the players'
movement only, else movement only),
u32 triangle count and 9 i16s a triangle.
"""
import json
import os
import shutil
import struct
import sys

import numpy as np

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.path.join(HERE, "ue3"))
sys.path.insert(0, HERE)

import anim  # noqa: E402
import bsp  # noqa: E402
import build_class as bc  # noqa: E402
import build_level as bl  # noqa: E402
import build_map as bm  # noqa: E402
import doors_kismet as dk  # noqa: E402
import kismet as K  # noqa: E402
import layout as lay  # noqa: E402
import levelfx  # noqa: E402
import remove_console as rc  # noqa: E402
from gltf_io import Doc, Writer, node_globals  # noqa: E402
from psa_io import Psa  # noqa: E402
from ue3pkg import Package  # noqa: E402

P = dk.P
DOOR_CONSOLE = "TnInteractCurrencyReceptacle"
CONSOLE_MESH = "AP_EscalationConsoleWall_SKEL"
CONSOLE_BIND = CONSOLE_MESH + "@bind"
CONSOLE_ANIMSET = "EscalationConsoleWall_ANIM"
CONSOLE_CLIPS = ("Activate_Console", "Equip_Console", "UnEquip_Console")
# The traps' consoles (TnInteractCurrencyReceptacleMultiUse: the floor console,
# AP_EscalationConsoleFloor_SKEL, its clips EscalationConsoleFloor_ANIM's of the same
# names as the wall console's), by their prompts' keys: Ignition's Flame Jets
# (Flamer) and GunShip Support, Oblivion's Combustion Chamber (Airlock), Downfall's
# Defensive Sentries (RoxSentry) and Ancients' Electric Fences (Elec). Oblivion's
# Security Drones (Drones) aren't built: their console stays in the map's model as it
# is.
TRAP_CONSOLE = "TnInteractCurrencyReceptacleMultiUse"
TRAPS = ("Flamer", "GunShip", "Airlock", "RoxSentry", "Elec")
FLOOR_MESH = "AP_EscalationConsoleFloor_SKEL"
FLOOR_BIND = FLOOR_MESH + "@bind"
FLOOR_ANIMSET = "EscalationConsoleFloor_ANIM"
# A multi-use console's Cooldown when it sets none (the class default's).
DEFAULT_COOLDOWN = 1.0
# The traps' particle systems, by the game's full names, as fx/weapons_fx.txt has
# them (tools/build_weapon_fx.py: two of them are "Flamethrower_FX").
TRAP_SYSTEMS = {
    "FX_CHR_Grimlock_p.FX.Flamethrower_FX": "Flamethrower_FX",
    "FX_L01_Exodus_p.FX.Flamethrower_FX": "Flamethrower_Exodus_FX",
    "ENV_MP_ProtoTypes_p.ESC_IncineratorFire_FX": "ESC_IncineratorFire_FX",
}
# The sentries the trap consoles wake, by their TnSentry archetypes (their
# factories' TnActorFactorySentryAI.Sentry): Downfall's rocket turrets and the
# gunship's machine guns, and the size their meshes are drawn at (their
# destructibles' SkeletalMeshComponent Scale).
SENTRY_KINDS = {"WEP_Interact_Sentry_p.RocketInteract_Sentry_TURRETDEF": ("rocket", 1.5),
                "WEP_Interact_Sentry_p.MGInteract_TURRETDEF": ("mg", 1.0)}
# The console's materials as Downfall's build draws them, and those its outline goes
# round (EnableOutlineHighlight on: the body and its new parts; OutlineColor
# (0.2155, 0.8865, 1.0), OutlineIntensity 2).
CONSOLE_MAP = "downfall"
CONSOLE_OUTLINED = ("AP_EscalationConsoles_MATINST", "AP_EscalationConsoles_NewParts_MATINST")
CONSOLE_OUTLINE = "ConsoleOutline"
CONSOLE_OUTLINE_COLOR = [0.2155, 0.8865, 1.0]
OUTLINE_WIDTH = 0.02
# Where Oblivion's doors and force fields take their materials (its build has none).
MATERIAL_DONOR = "ancients"
# Actor classes: the doors (with a mesh), the volumes a purchase switches, and those
# that only have a place.
MESH_CLASSES = ("StaticInterpActor", "InterpActor", "TnTransformingWorldActor", "SkeletalMeshActor",
                "HmSkeletalMeshActor", "StaticMeshActor")
VOLUME_CLASSES = ("HmPlayerOnlyBlockingVolume", "DynamicBlockingVolume", "BlockingVolume")
SPOT_CLASSES = ("AmbientSound", "AmbientSoundSimple", "AmbientSoundNonLoop", "HmAmbientSoundLineEmitter",
                "AmbientSoundMovable")
# Left alone: the sentries' preview meshes and drones (the traps' sentries are
# their own: the sentry lines), and what isn't a door (Oblivion's observation deck's
# jets, flying past once it is open). The traps' steam vents and gunship are
# skeletal doors here, their Matinees playing them.
SKIP_MESHES = ("WEP_SentryRocket_SKEL", "AP_AttackSentry_SKEL", "AP_JetsLoResMixed_SKELMESH",
               "EnergonCube_HoldingTower_STAT")
MODES = {"CIM_Linear": "L", "CIM_Constant": "C"}
# HmAmbientSoundLineEmitter's LineLength (its class default, Unreal units).
LINE_LENGTH = 500.0


def f(x):
    return f"{x:.6g}"


def fv(v):
    return " ".join(f(x) for x in v)


# ---------------------------------------------------------------------------
# Materials and meshes copied from a built map's model
# ---------------------------------------------------------------------------

class Source:
    """A built map's model: its glTF (gltf_io.Doc) and where its textures are."""

    def __init__(self, assets, key):
        self.key = key
        self.path = os.path.join(assets, "models", key + ".glb")
        self.doc = Doc(self.path)
        self.g = self.doc.g
        self.by_name = {}
        for i, m in enumerate(self.g.get("materials", [])):
            self.by_name.setdefault(m.get("name"), i)


class Copier:
    """Copies materials and static meshes from Sources into a Writer, the textures
    pointing at the same files (copied into this map's folder from another's)."""

    def __init__(self, w, assets, key, folder=None):
        self.w, self.assets, self.key = w, assets, key
        self.folder = folder or key
        self.mats, self.meshes, self.images = {}, {}, {}

    def image(self, src, img):
        uri = img["uri"]
        folder, name = uri.split("/", 1)
        if folder != self.folder:
            here = os.path.join(self.assets, "models", self.folder)
            os.makedirs(here, exist_ok=True)
            data = open(os.path.join(self.assets, "models", folder, name), "rb").read()
            target = os.path.join(here, name)
            if os.path.exists(target) and open(target, "rb").read() != data:
                stem, ext = os.path.splitext(name)
                name = f"{stem}_{src.key}{ext}"
                target = os.path.join(here, name)
            if not os.path.exists(target):
                open(target, "wb").write(data)
            uri = f"{self.folder}/{name}"
        if uri not in self.images:
            self.w.g["images"].append({"uri": uri, "name": img.get("name", name)})
            self.images[uri] = len(self.w.g["images"]) - 1
        return self.images[uri]

    def texture(self, src, index):
        t = src.g["textures"][index]
        image = self.image(src, src.g["images"][t["source"]])
        sampler = src.g["samplers"][t["sampler"]] if "sampler" in t else None
        nt = {"source": image}
        if sampler is not None:
            if sampler not in self.w.g["samplers"]:
                self.w.g["samplers"].append(dict(sampler))
            nt["sampler"] = self.w.g["samplers"].index(sampler)
        self.w.g["textures"].append(nt)
        return len(self.w.g["textures"]) - 1

    def material(self, src, index):
        k = (src.path, index)
        if k not in self.mats:
            m = json.loads(json.dumps(src.g["materials"][index]))

            def walk(d):
                for key, val in d.items():
                    if isinstance(val, dict):
                        if key.endswith("Texture") and "index" in val:
                            val["index"] = self.texture(src, val["index"])
                        walk(val)
            walk(m)
            self.mats[k] = self.w.material(m)
        return self.mats[k]

    def named(self, src, name):
        """The material named so in the Source (None if it hasn't one)."""
        i = src.by_name.get(name)
        return None if i is None else self.material(src, i)

    def mesh(self, src, index):
        """A static mesh of the Source, its primitives as they are (light-map UVs,
        vertex colours and all)."""
        k = (src.path, index)
        if k in self.meshes:
            return self.meshes[k]
        prims = []
        for p in src.g["meshes"][index]["primitives"]:
            attrs = {}
            for name, ai in p["attributes"].items():
                acc = src.g["accessors"][ai]
                arr = src.doc.acc(ai)
                if acc.get("normalized"):
                    top = {5121: (255, np.uint8), 5123: (65535, np.uint16)}[acc["componentType"]]
                    arr = np.round(np.clip(arr, 0, 1) * top[0]).astype(top[1])
                    attrs[name] = self.w.accessor(arr, 34962, normalized=True)
                else:
                    attrs[name] = self.w.accessor(arr, 34962, minmax=name == "POSITION")
            q = {"attributes": attrs,
                 "indices": self.w.accessor(src.doc.acc(p["indices"]).reshape(-1).astype(np.uint32), 34963)}
            if "material" in p:
                q["material"] = self.material(src, p["material"])
            prims.append(q)
        self.w.g["meshes"].append({"name": src.g["meshes"][index].get("name", ""), "primitives": prims})
        self.meshes[k] = len(self.w.g["meshes"]) - 1
        return self.meshes[k]


def node_matrix(n):
    if "matrix" in n:
        return np.array(n["matrix"], np.float64).reshape(4, 4).T
    from gltf_io import node_local
    return node_local(n)


def matrix_list(T):
    return [float(f"{x:.7g}") for x in T.T.ravel()]


# ---------------------------------------------------------------------------
# Skinned models (the consoles' and the skeletal doors')
# ---------------------------------------------------------------------------

def smooth_normals(pos, nrm):
    key = np.round(pos / 1e-4).astype(np.int64)
    _, group = np.unique(key, axis=0, return_inverse=True)
    group = group.reshape(-1)
    s = np.zeros((group.max() + 1, 3))
    np.add.at(s, group, nrm)
    s /= np.maximum(np.linalg.norm(s, axis=1, keepdims=True), 1e-9)
    return s[group]


def skinned(w, doc, material_of, root, outline=None):
    """umodel's skinned mesh under `root`: its skeleton, its primitives in the
    materials `material_of` gives their names, and (with `outline`: the outlined
    material names and the outline's material) a shell round those, skinned the
    same. Returns the bones' node indices by name and their rest pose."""
    _, bone_root, joints = bc.copy_skeleton(w, doc, root)
    names = {w.g["nodes"][j]["name"]: j for j in joints}
    prims, shells = [], []
    for p in doc.g["meshes"][0]["primitives"]:
        mname = doc.g["materials"][p["material"]]["name"] if "material" in p else None
        at = p["attributes"]
        pos = doc.acc(at["POSITION"]).astype(np.float32)
        nrm = doc.acc(at["NORMAL"]).astype(np.float32)
        jt = doc.acc(at["JOINTS_0"]).astype(np.uint16)
        wt = doc.acc(at["WEIGHTS_0"]).astype(np.float32)
        wt = wt / np.maximum(wt.sum(1, keepdims=True), 1e-8)
        attrs = {"POSITION": w.accessor(pos, 34962, minmax=True), "NORMAL": w.accessor(nrm, 34962),
                 "TEXCOORD_0": w.accessor(doc.acc(at["TEXCOORD_0"]).astype(np.float32), 34962),
                 "JOINTS_0": w.accessor(jt, 34962), "WEIGHTS_0": w.accessor(wt, 34962)}
        if "TANGENT" in at:
            attrs["TANGENT"] = w.accessor(doc.acc(at["TANGENT"]).astype(np.float32), 34962)
        idx = doc.acc(p["indices"]).reshape(-1).astype(np.uint32)
        q = {"attributes": attrs, "indices": w.accessor(idx, 34963)}
        m = material_of(mname)
        if m is not None:
            q["material"] = m
        prims.append(q)
        if outline and mname in outline[0]:
            n = smooth_normals(pos.astype(np.float64), nrm.astype(np.float64))
            shell = (pos + n * OUTLINE_WIDTH).astype(np.float32)
            tri = idx.reshape(-1, 3)[:, [0, 2, 1]].reshape(-1)
            shells.append({"attributes": {"POSITION": w.accessor(shell, 34962, minmax=True),
                                          "NORMAL": w.accessor((-n).astype(np.float32), 34962),
                                          "JOINTS_0": w.accessor(jt, 34962), "WEIGHTS_0": w.accessor(wt, 34962)},
                           "indices": w.accessor(tri.astype(np.uint32), 34963), "material": outline[1]})
    skin = doc.g["skins"][0]
    ibm = doc.acc(skin["inverseBindMatrices"]).astype(np.float32).reshape(-1, 16)
    w.g["skins"].append({"joints": joints, "inverseBindMatrices": w.accessor(ibm), "skeleton": bone_root})
    s = len(w.g["skins"]) - 1
    w.g["meshes"].append({"name": doc.g["nodes"][0].get("name", "mesh"), "primitives": prims})
    w.g["nodes"][root]["children"].append(w.node({"name": doc.g["nodes"][0].get("name", "mesh"),
                                                  "mesh": len(w.g["meshes"]) - 1, "skin": s}))
    if shells:
        w.g["meshes"].append({"name": CONSOLE_OUTLINE, "primitives": shells})
        w.g["nodes"][root]["children"].append(w.node({"name": CONSOLE_OUTLINE, "mesh": len(w.g["meshes"]) - 1,
                                                      "skin": s}))
    rest = {w.g["nodes"][j]["name"]: (w.g["nodes"][j].get("translation", [0, 0, 0]),
                                     w.g["nodes"][j].get("rotation", [0, 0, 0, 1])) for j in joints}
    return names, rest


def find_export(export_dirs, mesh, kind="SkeletalMesh3"):
    for d in export_dirs:
        path = os.path.join(d, kind, mesh + ".gltf")
        if os.path.exists(path):
            return path
    return None


def package_dirs(export_root):
    return [os.path.join(export_root, d) for d in sorted(os.listdir(export_root))
            if os.path.isdir(os.path.join(export_root, d))]


def read_clip(pkg, anim_dirs, sets, seq):
    """A sequence's keys from the first of its animation sets that has it: umodel's
    .psa export, else the package (FoC's own layout: anim.py)."""
    for s in sets:
        s = s.split(".")[-1]
        for d in anim_dirs:
            path = os.path.join(d, s + ".psa")
            if os.path.exists(path):
                tracks, dt = Psa(path).clip(seq)
                if tracks:
                    return tracks, dt
        try:
            tracks, dt = anim.clip(pkg, s, seq)
            if tracks:
                return tracks, dt
        except (KeyError, ValueError):
            pass
    return None, None


def build_console(cooked, assets, export_root):
    """models/esc_console.glb: the wall console, its clips and its outline."""
    gltf = find_export(package_dirs(export_root), CONSOLE_MESH)
    if gltf is None:
        sys.exit(f"no {CONSOLE_MESH}.gltf in {export_root}")
    doc = Doc(gltf)
    src = Source(assets, CONSOLE_MAP)
    folder = os.path.join(assets, "models", "esc_console")
    if os.path.isdir(folder):
        shutil.rmtree(folder)
    w = Writer()
    w.g["extensionsUsed"] = ["KHR_materials_emissive_strength", "KHR_materials_unlit"]
    cp = Copier(w, assets, "esc_console")
    outline = w.material({"name": CONSOLE_OUTLINE + "_MAT",
                          "pbrMetallicRoughness": {"baseColorFactor": [0, 0, 0, 1], "metallicFactor": 0.0,
                                                   "roughnessFactor": 1.0},
                          "emissiveFactor": CONSOLE_OUTLINE_COLOR,
                          "extensions": {"KHR_materials_emissive_strength": {"emissiveStrength": 2.0}}})
    root = w.node({"name": "AP_EscalationConsoleWall", "children": []}, root=True)
    names, rest = skinned(w, doc, lambda n: cp.named(src, n), root, (CONSOLE_OUTLINED, outline))
    pkg = Package(os.path.join(cooked, bm.MAPS[CONSOLE_MAP]["base"] + ".xxx"))
    lengths = []
    for clip in CONSOLE_CLIPS:
        tracks, dt = anim.clip(pkg, CONSOLE_ANIMSET, clip)
        lengths.append(bc.base_clip(w, names, clip, tracks, dt, rest=rest, skip=()))
    del pkg
    path = os.path.join(assets, "models", "esc_console.glb")
    size = w.save(path)
    print(f"wrote {path} ({size / 1e6:.2f} MB): clips " +
          ", ".join(f"{c} {n:.2f} s" for c, n in zip(CONSOLE_CLIPS, lengths)))


def build_floor_console(cooked, assets, export_root):
    """models/esc_console_floor.glb: the traps' floor console (AP_EscalationConsoleFloor
    _SKEL), as the wall console is built (its clips, of the same names, from
    EscalationConsoleFloor_ANIM; its materials as Downfall's build draws it, its
    screen the trap flip book, AP_EscalationConsoleTrap3Flipbook_MAT; its outline),
    its textures in the wall console's folder (the same files but its screen's)."""
    exports = package_dirs(export_root)
    gltf = find_export(exports, FLOOR_MESH)
    if gltf is None:
        sys.exit(f"no {FLOOR_MESH}.gltf in {export_root}")
    doc = Doc(gltf)
    src = Source(assets, CONSOLE_MAP)
    # (Its textures go with the wall console's, which it shares but for its screen:
    # build_console has just made that folder afresh.)
    folder = os.path.join(assets, "models", "esc_console_floor")
    if os.path.isdir(folder):
        shutil.rmtree(folder)
    w = Writer()
    w.g["extensionsUsed"] = ["KHR_materials_emissive_strength", "KHR_materials_unlit"]
    cp = Copier(w, assets, "esc_console_floor", folder="esc_console")
    outline = w.material({"name": CONSOLE_OUTLINE + "_MAT",
                          "pbrMetallicRoughness": {"baseColorFactor": [0, 0, 0, 1], "metallicFactor": 0.0,
                                                   "roughnessFactor": 1.0},
                          "emissiveFactor": CONSOLE_OUTLINE_COLOR,
                          "extensions": {"KHR_materials_emissive_strength": {"emissiveStrength": 2.0}}})
    root = w.node({"name": "AP_EscalationConsoleFloor", "children": []}, root=True)
    names, rest = skinned(w, doc, lambda n: cp.named(src, n), root, (CONSOLE_OUTLINED, outline))
    pkg = Package(os.path.join(cooked, bm.MAPS[CONSOLE_MAP]["base"] + ".xxx"))
    anim_dirs = bm.export_dirs(export_root, "AnimSet")
    lengths = []
    for clip in CONSOLE_CLIPS:
        tracks, dt = read_clip(pkg, anim_dirs, [FLOOR_ANIMSET], clip)
        if not tracks:
            sys.exit(f"no {FLOOR_ANIMSET} {clip} (export the anim set: umodel -export ... {FLOOR_ANIMSET})")
        lengths.append(bc.base_clip(w, names, clip, tracks, dt, rest=rest, skip=()))
    del pkg
    path = os.path.join(assets, "models", "esc_console_floor.glb")
    size = w.save(path)
    print(f"wrote {path} ({size / 1e6:.2f} MB): clips " +
          ", ".join(f"{c} {n:.2f} s" for c, n in zip(CONSOLE_CLIPS, lengths)))


# ---------------------------------------------------------------------------
# Collision
# ---------------------------------------------------------------------------

def mesh_triangles(name, d, T):
    """A mesh's collision (build_level.collision_mesh, as build_map.py makes the
    map's) placed by the 4x4 column-vector matrix T: (n, 9) float32."""
    A, t = T[:3, :3], T[:3, 3]
    cm = bl.collision_mesh(name, d, np.linalg.norm(A, axis=0).max())
    if cm is None:
        return np.zeros((0, 9), np.float32)
    v = cm[0] @ A.T + t
    return v[cm[1].reshape(-1)].astype(np.float32).reshape(-1, 9)


def volume_triangles(pkg, path, owner):
    """A volume actor's brush (its BSP polygons, placed), Bevy metres."""
    M = bm.actor_matrix(bm.actor_props(pkg, pkg.exports[owner - 1]))
    tris = []
    for i, e in enumerate(pkg.exports, 1):
        if pkg.class_name(e) != "Model" or e["outer"] != owner:
            continue
        _, _vectors, points, nodes, _surfs, verts = bsp.extract(path, pkg=pkg, index=i)
        for vpool, _isurf, nv, _plane in nodes:
            if nv < 3:
                continue
            idx = verts[vpool:vpool + nv]
            if idx.max() >= len(points):
                continue
            Q = np.concatenate([points[idx].astype(np.float64), np.ones((nv, 1))], 1) @ M
            for k in range(1, nv - 1):
                tris.append(np.concatenate([Q[0, :3], Q[k, :3], Q[k + 1, :3]]))
    if not tris:
        return np.zeros((0, 9), np.float32)
    t = np.array(tris).reshape(-1, 3, 3)
    return (t[:, :, [0, 2, 1]] * 0.01).reshape(-1, 9).astype(np.float32)


class Groups:
    """levels/<map>_doors.bin's groups, quantized as the map's collision is."""

    def __init__(self, origin, step):
        self.origin, self.step = origin, step
        self.groups = []

    def add(self, name, tris, solid=True, players_only=False):
        """A group: solid (rays stop at it too), or for movement only (all of it,
        or only the players': players_only, an HmPlayerOnlyBlockingVolume's, which
        the AI walks through). Its number (-1 for none)."""
        if tris is None or len(tris) == 0:
            return -1
        q = np.round((np.asarray(tris, np.float64).reshape(-1, 3) - self.origin) / self.step)
        q = np.clip(q, -32768, 32767).astype(np.int16).reshape(-1, 9)
        self.groups.append((name, 1 if solid else (2 if players_only else 0), q))
        return len(self.groups) - 1

    def write(self, path):
        with open(path, "wb") as fh:
            fh.write(b"FOCD")
            fh.write(struct.pack("<I3ff", 1, *[float(x) for x in self.origin], float(self.step)))
            fh.write(struct.pack("<I", len(self.groups)))
            for name, flags, q in self.groups:
                b = name.encode()
                fh.write(struct.pack("<H", len(b)) + b + struct.pack("<BI", flags, len(q)))
                fh.write(q.astype("<i2").tobytes())


def drop_found(col_tris, origin, step, mine):
    """The map collision's triangles that are these (in its quantization, a step
    off allowed): their indices."""
    if len(mine) == 0:
        return set()
    q = np.round((mine.reshape(-1, 3) - origin) / step).astype(np.int64).reshape(-1, 9)
    want = set(rc.corner_sets(q))
    keys = rc.corner_sets(col_tris)
    hits = {j for j, k in enumerate(keys) if k in want}
    if len(hits) < len(want):
        wanted = [np.array(w_).reshape(3, 3) for w_ in want]
        lo = np.min([w_.min(0) for w_ in wanted], 0) - 2
        hi = np.max([w_.max(0) for w_ in wanted], 0) + 2
        c = col_tris.reshape(-1, 3, 3)
        near = np.nonzero(((c.min(1) >= lo) & (c.max(1) <= hi)).all(1))[0]
        for j in near:
            if j in hits:
                continue
            kk = np.array(keys[j]).reshape(3, 3)
            if any(np.abs(kk - w_).max() <= 1 for w_ in wanted):
                hits.add(int(j))
    return hits


# ---------------------------------------------------------------------------
# A map's doors
# ---------------------------------------------------------------------------

def fx_indices(assets, key):
    """The map's effect materials (levels/<map>_fx.txt, levelfx.py): {game material
    name: index}."""
    out = {}
    path = os.path.join(assets, "levels", f"{key}_fx.txt")
    if os.path.exists(path):
        with open(path) as fh:
            for line in fh:
                p = line.split()
                if len(p) >= 4 and p[0] == "mat":
                    out.setdefault(p[3], int(p[1]))
    return out


def prompts(cooked):
    """TransGame.int's [ContextualCommands]: {key: text} (from the game's
    Localization/Cooked/PC/Coalesced.int, next to its CookedPC)."""
    import build_menu
    path = os.path.join(os.path.dirname(os.path.normpath(cooked)), "Localization", "Cooked", "PC", "Coalesced.int")
    if not os.path.exists(path):
        print(f"  ! {path} not found: the consoles' prompts are left out")
        return {}
    text = build_menu.coalesced(path).get("TransGame/Localization/INT/TransGame.int", "")
    out, section = {}, None
    for line in text.splitlines():
        line = line.strip()
        if line.startswith("["):
            section = line
        elif section == "[ContextualCommands]" and "=" in line:
            k, v = line.split("=", 1)
            out.setdefault(k.strip(), v.strip())
    return out


def door_consoles(data):
    """The consoles built: the doors' and the traps' (TRAPS)."""
    return [c for c in data["consoles"] if c["class"] == DOOR_CONSOLE or is_trap(c)]


def is_trap(c):
    return c["class"] == TRAP_CONSOLE and str(c["prompt"]) in TRAPS


def trap_consoles(data):
    return [c for c in data["consoles"] if is_trap(c)]


def trap_matinees(data):
    """The Matinees the traps' consoles play."""
    return {e["op"] for c in trap_consoles(data) for e in c["timeline"] if e["cls"] == "SeqAct_Interp"}


def timelines(data):
    """(name, log) of every door console's purchase, and gameplay start's."""
    return [(c["id"], c["timeline"]) for c in door_consoles(data)] + [("start", data["start"])]


def door_matinees(data):
    """The Matinees the door consoles' purchases play (and nothing else's)."""
    out = set()
    for c in door_consoles(data):
        for e in c["timeline"]:
            if e["cls"] == "SeqAct_Interp":
                out.add(e["op"])
    return sorted(m for m in out if m in data["matinees"])


def ops_of(data, mats):
    """Each timeline's entries as doors.txt's "do" ops."""
    known = set(mats)
    out = []
    for name, log in timelines(data):
        for e in log:
            d = e["do"]
            t = e["t"]
            if e["cls"] == "SeqAct_Interp":
                if e["op"] not in known:
                    continue
                verb = {"matinee play": "play", "matinee reverse": "reverse", "matinee pause": "pause",
                        "matinee resume": "resume", "matinee stop": "stop"}.get(d)
                if verb:
                    out.append((name, t, verb, [e["op"]]))
            elif d.startswith("toggle "):
                verb = {"Turn On": "on", "Turn Off": "off", "Toggle": "toggle"}.get(d[7:])
                if verb and e.get("targets"):
                    out.append((name, t, verb, e["targets"]))
            elif d.startswith("outline "):
                verb = {"Turn On": "outline_on", "Turn Off": "outline_off"}.get(d[8:])
                if verb and e.get("targets"):
                    out.append((name, t, verb, e["targets"]))
            elif d.startswith("hidden "):
                verb = {"Hide": "hide", "UnHide": "unhide", "Show": "unhide"}.get(d[7:])
                if verb and e.get("targets"):
                    out.append((name, t, verb, e["targets"]))
            elif d.startswith("transform ") and d != "transform finished":
                verb = {"Transform": "transform", "Untransform": "untransform"}.get(d[10:])
                if verb and e.get("targets"):
                    out.append((name, t, verb, e["targets"]))
            elif d == "sound Play" and e.get("sound"):
                out.append((name, t, "sound", [str(e["sound"])] + list(e.get("at") or [])))
            elif d == "sound Stop" and e.get("sound"):
                out.append((name, t, "sound_stop", [str(e["sound"])] + list(e.get("at") or [])))
            elif d in ("dormant", "awake") and e.get("sentries"):
                out.append((name, t, "sleep" if d == "dormant" else "wake", list(e["sentries"])))
            elif e["cls"] == "SeqAct_TnActivateDestructible" and e.get("targets"):
                out.append((name, t, "collapse", list(e["targets"])))
    return out


def actor_matrix(level_actor, pkg):
    """An actor's mesh placement as build_map.py makes it (component and actor
    matrices), as a Bevy 4x4 column-vector matrix."""
    pr = bm.actor_props(pkg, pkg.exports[level_actor["index"] - 1])
    m = bm.actor_matrix(pr)
    for key in bm.SKELETAL_COMPONENTS + ("StaticMeshComponent",):
        c = bm.obj_index(pr.get(key))
        if c:
            m = lay.component_matrix(bm.chain_props(pkg, c)) @ m
            break
    T = np.eye(4)
    T[:3, :3] = P @ m[:3, :3].T @ P
    T[:3, 3] = P @ m[3, :3] * 0.01
    return T


def decompose(T):
    """A 4x4 column-vector matrix as (translation, rotation (x, y, z, w), scale)."""
    A = T[:3, :3]
    scale = np.linalg.norm(A, axis=0)
    R = A / np.maximum(scale, 1e-12)
    if np.linalg.det(R) < 0:
        scale[0], R[:, 0] = -scale[0], -R[:, 0]
    return T[:3, 3].tolist(), dk.quat_from_matrix(R), scale.tolist()


def bevy_rotation(rot):
    """An Unreal rotator as a rotation in Bevy's axes (column vectors)."""
    m = lay.rotator_matrix(*(rot or [0, 0, 0]))[:3, :3]
    return P @ m.T @ P


def convex_planes(tris):
    """A convex brush's faces as planes (n, d), n pointing out (n.x <= d inside),
    from its triangles (Bevy metres): one plane for each face."""
    t = np.asarray(tris, np.float64).reshape(-1, 3, 3)
    centre = t.reshape(-1, 3).mean(0)
    planes = []
    for a, b, c in t:
        n = np.cross(b - a, c - a)
        ln = np.linalg.norm(n)
        if ln < 1e-9:
            continue
        n /= ln
        d = float(n @ a)
        if n @ centre > d:
            n, d = -n, -d
        if not any(np.allclose(n, n2, atol=1e-4) and abs(d - d2) < 1e-3 for n2, d2 in planes):
            planes.append((n, d))
    return planes


def trap_lines(data, level, pkg, base_path, ops, lines):
    """The traps' own lines (see the header: emitter, pain, light, sentry), for the
    actors the trap consoles' purchases and Matinees switch and the sentries they
    wake; the names of those actors and sentries."""
    trap_ids = {c["id"] for c in trap_consoles(data)}
    switched = set()
    for name, _t, verb, args in ops:
        if name in trap_ids and verb in ("on", "off", "toggle"):
            switched.update(args)
    for m in trap_matinees(data):
        for grp in (data["matinees"].get(m) or {}).get("groups", []):
            if any(tr["type"] in ("toggle", "brightness") for tr in grp["tracks"]):
                switched.update(grp["actors"])
    out = set()
    for n in sorted(switched):
        la = level["actors"].get(n)
        if not la:
            continue
        cls = la["class"]
        loc = la.get("Location") or [0.0, 0.0, 0.0]
        if cls == "Emitter":
            psc = la.get("ParticleSystemComponent_") or {}
            tmpl = psc.get("Template")
            if not tmpl:
                continue
            system = TRAP_SYSTEMS.get(tmpl, tmpl.split(".")[-1])
            m = lay.rotator_matrix(*(la.get("Rotation") or [0, 0, 0]))[:3, :3]
            x, y = m[0], m[1]
            ds = float(la.get("DrawScale") or 1.0)
            ds3 = la.get("DrawScale3D") or [1.0, 1.0, 1.0]
            on = psc.get("bAutoActivate", True) is not False
            lit = bool(psc.get("bAcceptsLights")) and psc.get("LightEnvironment") is not None
            lines.append("\t".join(["emitter", n, system, fv(dk.bevy_pos(loc)), fv([x[0], x[2], x[1]]),
                                    fv([y[0], y[2], y[1]]), fv([ds * ds3[0], ds * ds3[1], ds * ds3[2]]),
                                    "1" if on else "0", "1" if lit else "0"]))
            out.add(n)
        elif cls in ("PhysicsVolume", "DynamicPhysicsVolume"):
            planes = convex_planes(volume_triangles(pkg, base_path, la["index"]))
            if len(planes) < 4:
                print(f"  ! {n}: no brush")
                continue
            kind = str(la.get("DamageType") or "").split(".")[-1]
            kind = {"TnDamageTypeFlameZone": "flame", "TnDamageTypeEnvironmentExplosion": "explosion"}.get(kind, kind)
            lines.append("\t".join(["pain", n, f(float(la.get("DamagePerSec") or 0.0)),
                                    "0" if la.get("bAffectPlayer") is False else "1",
                                    "0" if la.get("bAffectsLivingPawns") is False else "1", kind or "-",
                                    "1" if la.get("bPainCausing") else "0"] +
                                   [fv(list(pn) + [pd]) for pn, pd in planes]))
            out.add(n)
        elif cls == "PointLightToggleable":
            lc = la.get("LightComponent_") or {}
            r, g_, b, _a = lc.get("LightColor") or [255, 255, 255, 255]
            lines.append("\t".join(["light", n, fv(dk.bevy_pos(loc)), fv([r / 255.0, g_ / 255.0, b / 255.0]),
                                    f(float(lc.get("Brightness", 1.0))), f(float(lc.get("Radius", 1024.0)) * 0.01),
                                    f(float(lc.get("FalloffExponent", 2.0))),
                                    "0" if lc.get("bEnabled") is False else "1"]))
            out.add(n)
    # The sentries: spawned as gameplay starts (where their factories' spawn points
    # are, or attached to an actor's bone: the gunship's), each its factory's.
    spawned, attached = {}, {}
    for e in data["start"]:
        if e["do"] == "spawn" and e["op"] not in spawned:
            spawned[e["op"]] = (e["t"], (e.get("at") or [None])[0])
        elif e["do"] == "attach":
            pr = level["ops"][e["op"]]["props"]
            for s_ in e.get("sentries") or []:
                attached[s_] = ((e.get("targets") or [None])[0], pr)
    for s_, (t, at) in sorted(spawned.items()):
        fac = level["ops"][s_].get("factory") or {}
        kind = SENTRY_KINDS.get(fac.get("Sentry"))
        if kind is None:
            continue
        kind, scale = kind
        a = level["actors"].get(at) or {}
        R = bevy_rotation(a.get("Rotation"))
        cells = ["sentry", s_, kind, fv(dk.bevy_pos(a.get("Location") or [0, 0, 0])), fv(dk.quat_from_matrix(R)),
                 f(scale), f(t)]
        if s_ in attached:
            base, pr = attached[s_]
            sock = {"location": pr.get("RelativeOffset") if pr.get("bUseRelativeOffset") else [0, 0, 0],
                    "rotation": pr.get("RelativeRotation") if pr.get("bUseRelativeRotation") else [0, 0, 0],
                    "scale": [1.0, 1.0, 1.0]}
            tt, q, _sc = bc.gltf_socket(sock)
            cells += [base or "-", str(pr.get("BoneName") or "-"), fv(tt), fv(q)]
        lines.append("\t".join(cells))
        out.add(s_)
    return out


def crumble_lines(cooked, package, names):
    """What the props a purchase brings down do as they come down (crumble and dsys
    lines, see the module docs): for each, its destructible's state the Kismet's
    HmKismetDestructionTrigger takes its first state to, and that state's effects
    (each state once). Its place and turn as tools/build_barrels.py writes a
    barrel's."""
    import build_death_sets as bds
    import class_data as cd
    mp = cd.Mp(cooked, package)
    p = mp.pkg
    lines, done = [], set()
    # (What the demo draws or plays of a state's effects, as the barrel's.)
    skip = ("HmStaticMeshDestructionEffectFactory", "TnTargetableDestructionEffectFactory",
            "HmNoiseDestructionEffectFactory", "HmStateNotificationDestructionEffectFactory")
    holder = type("Holder", (), {})()
    holder.lines = []
    for n in names:
        idx = next((i + 1 for i, e in enumerate(p.exports)
                    if e["name"] == n and p.class_name(e) == "TnStaticDestructibleActor"), 0)
        if not idx:
            print(f"  ! {n}: not in {package}")
            continue
        pr = bm.actor_props(p, p.exports[idx - 1])
        comp = bm.obj_index(pr.get("DestructibleComponent"))
        bp = bm.obj_index(bm.chain_props(p, comp).get("Blueprint")) if comp else None
        if not bp:
            continue
        blueprint = p.exports[bp - 1]["name"]
        pieces = [i for i in cd._objs(mp.props(bp).get("PieceBlueprints")) if i > 0]
        for k, piece in enumerate(pieces):
            states = list(cd._objs(mp.props(piece).get("StateBlueprints")))
            if not states:
                continue
            # (The link out of its first state that the Kismet's trigger takes.)
            to = None
            for link in cd._objs(mp.props(states[0]).get("LinkBlueprints")):
                lpr = mp.props(link)
                trigger = mp.full(cd._obj(lpr.get("TransitionTrigger"))) or ""
                if trigger.endswith("HmKismetDestructionTrigger"):
                    to = int(bds.num(lpr.get("NextStateHandle"), 0))
            if to is None or to >= len(states):
                print(f"  ! {n}: {blueprint} has no state its Kismet brings it to")
                continue
            state = f"{blueprint}:{to}" if len(pieces) == 1 else f"{blueprint}:{k}:{to}"
            m = bm.actor_matrix(pr)
            T = np.eye(4)
            T[:3, :3] = P @ m[:3, :3].T @ P
            T[:3, 3] = P @ m[3, :3] * 0.01
            t, q, _sc = decompose(T)
            lines.append("\t".join(["crumble", n, state, fv(t), fv(q)]))
            if state in done:
                continue
            done.add(state)
            ptm = bds.transform(mp, mp.props(piece).get("Transform"))
            for f in cd._objs(mp.props(states[to]).get("EffectFactories")):
                if f > 0 and p.class_name(p.exports[f - 1]) not in skip:
                    bds.Deaths.factory(holder, state, mp, f, ptm)
    print(f"  {len(lines)} props' coming down ({len(done)} states)")
    return lines + holder.lines


def build_map_doors(key, export_root, cooked, assets):
    spec = bm.MAPS[key]
    base_path = os.path.join(cooked, spec["base"] + ".xxx")
    pkg = Package(base_path)
    pkg.path = base_path
    level = dk.read_level(pkg)
    # (The map's AUDIO package's Kismet, which the BASE package's remote events set
    # off: Oblivion's combustion chamber's blast and alarm.)
    audio_path = os.path.join(cooked, spec["base"].replace("_BASE_", "_AUDIO_") + ".xxx")
    if os.path.exists(audio_path):
        apkg = Package(audio_path)
        apkg.path = audio_path
        dk.merge_audio(level, dk.read_level(apkg))
        del apkg
    glb_path = os.path.join(assets, "models", key + ".glb")
    col_path = os.path.join(assets, "levels", f"{key}_col.bin")
    g, rest = rc.read_glb(glb_path)
    data = dk.interpret(level, g)
    src = Source(assets, key)
    donor = Source(assets, MATERIAL_DONOR) if key != MATERIAL_DONOR else src
    exports = package_dirs(export_root)
    anim_dirs = bm.export_dirs(export_root, "AnimSet")
    opened = K.opened_state(pkg)
    origin, step, col_tris, col_vol = rc.read_col(col_path)
    groups = Groups(origin, step)
    mats = door_matinees(data)
    ops = ops_of(data, mats)
    consoles = door_consoles(data)
    actors = data["actors"]
    lines = [f"# {spec['title']}'s locked doors and their unlock consoles (tools/build_doors.py): see its header"]
    take_out, drop = set(), set()

    # ---- the consoles ----
    texts = prompts(cooked)
    for k in sorted({str(c["prompt"]) for c in consoles}):
        if k in texts:
            lines.append("\t".join(["prompt", k, texts[k]]))
    bind_poses = {}
    for c in consoles:
        tr = c["transform"]
        floor = is_trap(c)
        mesh, bind = (FLOOR_MESH, FLOOR_BIND) if floor else (CONSOLE_MESH, CONSOLE_BIND)
        cooldown = (c.get("cooldown") or DEFAULT_COOLDOWN) if floor else 0
        lines.append("\t".join(["console", c["id"], fv(tr["translation"]), fv(tr["rotation"]), fv(c["spot"]),
                                str(c["cost"]), str(c["increment"]), str(c["prompt"]), str(c["total"] or "-"),
                                "floor" if floor else "wall", f(float(cooldown))]))
        near = [i for i, n in enumerate(g["nodes"]) if n.get("name") == bind and
                np.linalg.norm(node_matrix(n)[:3, 3] - np.array(tr["translation"])) < 0.5]
        take_out.update(near)
        if not near:
            # (The map's build has no consoles, Oblivion's: its collision neither.)
            if mesh not in bind_poses:
                gltf = find_export(exports, mesh)
                bind_poses[mesh] = bm.Posed(gltf) if gltf else False
            if bind_poses[mesh]:
                T = actor_matrix(level["actors"][c["id"]], pkg)
                k = groups.add("console:" + c["id"], mesh_triangles(bind, bind_poses[mesh], T))
                lines.append("\t".join(["volume", "console:" + c["id"], str(k)]))

    # ---- what the purchases move ----
    movers = set()
    for m in mats:
        for grp in data["matinees"][m]["groups"]:
            for tr in grp["tracks"]:
                if tr["type"] in ("move", "anim", "scale"):
                    movers.update(grp["actors"])
    for _name, _t, verb, args in ops:
        if verb in ("transform", "untransform", "hide", "unhide"):
            movers.update(args)
    for n in list(movers):
        for r_ in (actors.get(n) or {}).get("riders", []):
            if r_["hard"]:
                movers.add(r_["actor"])
    # Each skeletal kind's clips (the sequences its actors start in and their
    # Matinees play) and animation sets.
    kinds = {}
    for n in movers:
        a = actors.get(n)
        if not a or "mesh" not in a or "SkeletalMeshComponent_" not in level["actors"].get(n, {}):
            continue
        mesh = a["mesh"].split(".")[-1]
        clips, sets = kinds.setdefault(mesh, ([], []))
        seq = (a.get("AnimNode") or {}).get("AnimSeqName")
        if seq:
            clips.append(seq)
        sets.extend(s_ for s_ in a.get("AnimSets") or [] if s_ not in sets)
        for m in mats:
            for grp in data["matinees"][m]["groups"]:
                if n in grp["actors"]:
                    for tr in grp["tracks"]:
                        if tr["type"] == "anim":
                            clips.extend(str(k.get("AnimSeqName")) for k in tr["keys"] or [])
                            sets.extend(s_ for s_ in tr.get("animsets") or [] if s_ not in sets)

    w = Writer()
    w.g["extensionsUsed"] = ["KHR_materials_emissive_strength", "KHR_materials_unlit"]
    cp = Copier(w, assets, key)
    fx_index, fx_mats = fx_indices(assets, key), {}
    scenes = []
    skeletal_scenes = {}
    posed_cache = {}
    out_actors = set()

    def posed(mesh, pose, sets):
        k = (mesh, str(pose))
        if k not in posed_cache:
            gltf = find_export(exports, mesh)
            clip = (lambda seq: read_clip(pkg, anim_dirs, sets, seq)) if sets else None
            posed_cache[k] = bm.Posed(gltf, pose, clip) if gltf else None
        return posed_cache[k]

    def material_from(over, i, name):
        want = over[i] if i < len(over) and over[i] else name
        if want in bm.FX_MATERIALS and want in fx_index:
            # (Drawn by its game material, as the map's model draws it: levelfx.rs.)
            if want not in fx_mats:
                fx_mats[want] = w.material(levelfx.fx_gltf_material(key, fx_index[want], want))
            return fx_mats[want]
        if want in src.by_name:
            return cp.named(src, want)
        if want in donor.by_name:
            return cp.named(donor, want)
        print(f"  ! {key}: no material {want}")
        return None

    for n in sorted(movers):
        a = actors.get(n)
        if not a:
            continue
        if a["class"] in VOLUME_CLASSES:
            # A volume riding on a door: on while the door stands where it starts.
            tris = volume_triangles(pkg, base_path, a["index"])
            only = a["class"] == "HmPlayerOnlyBlockingVolume"
            lines.append("\t".join(["volume", n, str(groups.add("volume:" + n, tris, False, only))]))
            continue
        if "mesh" not in a:
            continue
        mesh = a["mesh"].split(".")[-1]
        if mesh in SKIP_MESHES or a["class"] not in MESH_CLASSES:
            continue
        tr = a["transform"]
        closed = np.array(tr["translation"], np.float64)
        final = np.array(a.get("final") or [0.0, 0.0, 0.0], np.float64)
        match = a.get("glb")
        T = actor_matrix(level["actors"][n], pkg)
        # (An effect mesh, a force field's glow, has no collision: blocking volumes
        # do its blocking.)
        collide = not any(s_ in mesh for s_ in bm.SKIP_COLLIDE + bm.FX_MESHES)
        over = a.get("Materials") or []
        out_actors.add(n)
        if mesh not in kinds:
            # A static door: the map model's mesh (or the export's), in its own frame
            # (doors.rs places it at the actor's place).
            if match:
                gn = g["nodes"][match["node"]]
                M = node_matrix(gn)
                here = closed if match["state"] == "closed" else closed + final
                local = M.copy()
                local[:3, 3] -= here
                node = {"name": n, "mesh": cp.mesh(src, gn["mesh"]), "matrix": matrix_list(local)}
                if "extras" in gn:
                    node["extras"] = gn["extras"]
                take_out.add(match["node"])
                d, cname = rc.MeshDoc(src.doc, gn["mesh"]), gn["name"]
            else:
                gltf = find_export(exports, mesh, "StaticMesh3")
                if gltf is None:
                    print(f"  ! {n}: no {mesh} in the export")
                    continue
                d, cname = Doc(gltf), mesh
                local = T.copy()
                local[:3, 3] -= closed
                prims = []
                for i, p in enumerate(d.g["meshes"][0]["primitives"]):
                    at_ = p["attributes"]
                    attrs = {"POSITION": w.accessor(d.acc(at_["POSITION"]).astype(np.float32), 34962, minmax=True),
                             "NORMAL": w.accessor(d.acc(at_["NORMAL"]).astype(np.float32), 34962),
                             "TEXCOORD_0": w.accessor(d.acc(at_["TEXCOORD_0"]).astype(np.float32), 34962)}
                    q = {"attributes": attrs,
                         "indices": w.accessor(d.acc(p["indices"]).reshape(-1).astype(np.uint32), 34963)}
                    mname = d.g["materials"][p["material"]]["name"] if "material" in p else None
                    mi = material_from(over, i, mname)
                    if mi is not None:
                        q["material"] = mi
                    prims.append(q)
                w.g["meshes"].append({"name": mesh, "primitives": prims})
                node = {"name": n, "mesh": len(w.g["meshes"]) - 1, "matrix": matrix_list(local)}
            scenes.append([w.node(node)])
            s = len(scenes) - 1
            cg = og = -1
            if collide:
                Tc = local.copy()
                Tc[:3, 3] += closed
                cg = groups.add("closed:" + n, mesh_triangles(cname, d, Tc))
                if final.any():
                    To = Tc.copy()
                    To[:3, 3] += final
                    tris = mesh_triangles(cname, d, To)
                    og = groups.add("open:" + n, tris)
                    if match and match["state"] == "open":
                        drop |= drop_found(col_tris, origin, step, tris)
            lines.append("\t".join(["door", n, fv(tr["translation"]), str(s), str(cg), str(og)]))
        else:
            # A skeletal door: its kind's scene, placed and posed.
            clips_wanted, sets = kinds[mesh]
            if mesh not in skeletal_scenes:
                gltf = find_export(exports, mesh)
                if gltf is None:
                    print(f"  ! {n}: no {mesh} in the export")
                    continue
                doc = Doc(gltf)
                root = w.node({"name": mesh, "children": []})
                names, rest_pose = skinned(w, doc, lambda name, over=over: material_from(over, 0, name), root)
                clips = {}
                for clip in dict.fromkeys(clips_wanted):
                    tracks, dt = read_clip(pkg, anim_dirs, sets, clip)
                    if not tracks:
                        print(f"  ! {mesh}: no clip {clip}")
                        continue
                    secs = bc.base_clip(w, names, clip, tracks, dt, rest=rest_pose, skip=())
                    clips[clip] = (len(w.g["animations"]) - 1, secs)
                scenes.append([root])
                skeletal_scenes[mesh] = (len(scenes) - 1, clips)
            s, clips = skeletal_scenes[mesh]
            seq = (a.get("AnimNode") or {}).get("AnimSeqName")
            end = a.get("InitialTransformState") == "ETWAS_Transformed"
            pose_clip = seq if seq in clips else (next(iter(clips)) if clips else "-")
            pose_t = clips[pose_clip][1] if (end and pose_clip in clips) else 0.0
            cg = og = -1
            if collide and seq:
                pz = posed(mesh, ("end" if end else "start", seq), sets)
                closed_name = f"{mesh}@{seq}@{'end' if end else 'start'}"
                if pz is not None:
                    cg = groups.add("closed:" + n, mesh_triangles(closed_name, pz, T))
                tris = None
                if match:
                    # Where the map's model has it, in the pose its build left it in.
                    gn = g["nodes"][match["node"]]
                    pose = None
                    if ("anim", a["index"]) in opened:
                        pose = ("anim", opened[("anim", a["index"])])
                    elif gn["name"].endswith("@end"):
                        pose = ("end", gn["name"].split("@")[1])
                    elif gn["name"].endswith("@start"):
                        pose = ("start", gn["name"].split("@")[1])
                    pz2 = posed(mesh, pose, sets) if pose else None
                    if pz2 is not None:
                        tris = mesh_triangles(gn["name"], pz2, node_matrix(gn))
                        drop |= drop_found(col_tris, origin, step, tris)
                    take_out.add(match["node"])
                elif final.any() and pz is not None:
                    # (Moved, and not in the map's model: where it ends up.)
                    To = T.copy()
                    To[:3, 3] += final
                    tris = mesh_triangles(closed_name, pz, To)
                og = groups.add("open:" + n, tris)
            elif match:
                take_out.add(match["node"])
            at, turn, scale = decompose(T)
            lines.append("\t".join(["skeletal", n, fv(at), fv(turn), fv(scale), str(s), str(cg), str(og),
                                    pose_clip, f(pose_t)]))
        for r_ in a.get("riders", []):
            if r_["hard"]:
                lines.append("\t".join(["rider", n, r_["actor"]]))
    for _mesh, (s, clips) in skeletal_scenes.items():
        for clip, (ai, secs) in clips.items():
            lines.append("\t".join(["clip", str(s), clip, str(ai), f(secs)]))

    # ---- the props the purchases bring down ----
    # (The map's build has them standing, their intact state's meshes: build_map.py's
    # destructibles. Each is a door here, its meshes the map model's, its collision
    # theirs, out of the map's.)
    collapsing = sorted({x for _n, _t, verb, args in ops if verb == "collapse" for x in args})
    pieces = {}
    if collapsing:
        for it in bm.destructibles(pkg, spec["base"] + ".xxx", set()):
            pieces.setdefault(it["source"], []).append(it)
        glb_nodes = dk.glb_nodes(g)
    for n in collapsing:
        a = actors.get(n)
        if not a or n not in pieces:
            print(f"  ! {n}: brought down, but no meshes of its own")
            continue
        here = np.array(a["transform"]["translation"], np.float64)
        nodes_, tris = [], []
        for it in pieces[n]:
            m = np.array(it["matrix"], np.float64).reshape(4, 4)
            at = [m[3, 0] * 0.01, m[3, 2] * 0.01, m[3, 1] * 0.01]
            match = dk.glb_match(glb_nodes, it["mesh"], [("closed", at)])
            if match is None:
                print(f"  ! {n}: no {it['mesh']} in the map's model at {at}")
                continue
            gn = g["nodes"][match["node"]]
            M = node_matrix(gn)
            local = M.copy()
            local[:3, 3] -= here
            node = {"name": n, "mesh": cp.mesh(src, gn["mesh"]), "matrix": matrix_list(local)}
            if "extras" in gn:
                node["extras"] = gn["extras"]
            nodes_.append(w.node(node))
            take_out.add(match["node"])
            tris.append(mesh_triangles(gn["name"].split("@")[0], rc.MeshDoc(src.doc, gn["mesh"]), M))
        if not nodes_:
            continue
        scenes.append(nodes_)
        tris = np.concatenate(tris)
        cg = groups.add("closed:" + n, tris)
        found = drop_found(col_tris, origin, step, tris)
        drop |= found
        out_actors.add(n)
        lines.append("\t".join(["door", n, fv(here), str(len(scenes) - 1), str(cg), "-1"]))
        corners = tris.reshape(-1, 3)
        lo, hi = corners.min(0), corners.max(0)
        foot = [(lo[0] + hi[0]) * 0.5, lo[1], (lo[2] + hi[2]) * 0.5]
        lines.append("\t".join(["collapse", n, fv(foot)]))
        print(f"  {n}: brought down by a purchase ({len(nodes_)} meshes, {len(tris)} collision triangles, "
              f"{len(found)} of them out of the map's)")
    if collapsing:
        lines += crumble_lines(cooked, spec["base"] + ".xxx", collapsing)

    # ---- the volumes the purchases switch, and where sounds play ----
    switched = set()
    for _name, _t, verb, args in ops:
        if verb in ("on", "off", "toggle"):
            switched.update(args)
    volumes = set()
    for n in sorted(switched):
        a = actors.get(n)
        if not a or a["class"] not in VOLUME_CLASSES or n in movers:
            continue
        tris = volume_triangles(pkg, base_path, a["index"])
        only = a["class"] == "HmPlayerOnlyBlockingVolume"
        lines.append("\t".join(["volume", n, str(groups.add("volume:" + n, tris, False, only))]))
        volumes.add(n)
    trap_actors = trap_lines(data, level, pkg, base_path, ops, lines)
    spots = set()
    for _name, _t, verb, args in ops:
        if verb in ("sound", "sound_stop"):
            spots.update(args[1:])
    for m in mats:
        for grp in data["matinees"][m]["groups"]:
            if any(tr["type"] == "sound" for tr in grp["tracks"]):
                spots.update(grp["actors"])
    for n in sorted(spots - out_actors):
        a = actors.get(n)
        if a:
            cells = ["spot", n, fv(a["transform"]["translation"])]
            la = level["actors"].get(n) or {}
            if la.get("class") == "HmAmbientSoundLineEmitter":
                # (A line of sound, through its place along its X: LineLength times
                # its DrawScale3D's X and its DrawScale. Its sounds are heard from the
                # point of it nearest the listener: the flame jets' roar.)
                m = lay.rotator_matrix(*(la.get("Rotation") or [0, 0, 0]))[:3, :3]
                ds = float(la.get("DrawScale") or 1.0) * float((la.get("DrawScale3D") or [1.0])[0])
                half = m[0] * float(la.get("LineLength") or LINE_LENGTH) * ds * 0.5 * 0.01
                cells.append(fv([half[0], half[2], half[1]]))
            lines.append("\t".join(cells))
            out_actors.add(n)

    # ---- the Matinees ----
    volume_movers = {n for n in movers if (actors.get(n) or {}).get("class") in VOLUME_CLASSES}
    for m in mats:
        mt = data["matinees"][m]
        pr = mt.get("props", {})
        lines.append("\t".join(["matinee", m, f(float(mt.get("length") or 0.0)),
                                f(float(pr.get("PlayRate", 1.0) or 1.0)), "1" if pr.get("bLooping") else "0"]))
        for grp in mt["groups"]:
            for tr in grp["tracks"]:
                if tr["type"] == "move" and tr["relative"]:
                    for an, keys in tr["keys"].items():
                        if not keys or (an not in out_actors and an not in volume_movers) or \
                                all(not any(k[1]) for k in keys):
                            continue
                        cells = [" ".join([f(k[0]), fv(k[1]), MODES.get(k[2], "A"), fv(k[3]), fv(k[4])])
                                 for k in keys]
                        lines.append("\t".join(["move", m, an] + cells))
                elif tr["type"] == "turn":
                    # (Roll, pitch and yaw in degrees, relative to the actor's own turn
                    # as it starts, which follows: the gunship circling.)
                    cells = [" ".join([f(k[0]), fv(k[1]), MODES.get(k[2], "A"), fv(k[3]), fv(k[4])])
                             for k in tr["keys"]]
                    for an in grp["actors"]:
                        if an in out_actors and cells:
                            R = bevy_rotation(level["actors"][an].get("Rotation"))
                            lines.append("\t".join(["turn", m, an, fv(dk.quat_from_matrix(R))] + cells))
                elif tr["type"] == "toggle":
                    for k in tr["keys"]:
                        act = {"ETTA_On": "on", "ETTA_Off": "off", "ETTA_Toggle": "toggle"}.get(str(k["ToggleAction"]))
                        who = [x for x in grp["actors"] if x in trap_actors]
                        if act and who:
                            lines.append("\t".join(["toggle", m, f(float(k["Time"])), act] + who))
                elif tr["type"] == "brightness":
                    cells = [" ".join([f(k[0]), f(k[1]), MODES.get(k[2], "A"), f(k[3]), f(k[4])])
                             for k in tr["keys"]]
                    for an in grp["actors"]:
                        if an in trap_actors and cells:
                            lines.append("\t".join(["brightness", m, an] + cells))
                elif tr["type"] == "scale":
                    cells = [" ".join([f(k[0]), f(k[1]), MODES.get(k[2], "A"), f(k[3]), f(k[4])])
                             for k in tr["keys"]]
                    for an in grp["actors"]:
                        if an in out_actors and cells:
                            lines.append("\t".join(["scale", m, an] + cells))
                elif tr["type"] == "sound":
                    for k in tr["keys"]:
                        if k.get("sound"):
                            lines.append("\t".join(["sound", m, f(float(k.get("time") or 0.0)), str(k["sound"])] +
                                                   [x for x in grp["actors"] if x in out_actors]))
                elif tr["type"] == "anim":
                    for k in tr["keys"] or []:
                        for an in grp["actors"]:
                            if an not in out_actors:
                                continue
                            lines.append("\t".join([
                                "anim", m, an, f(float(k.get("StartTime", 0.0) or 0.0)), str(k.get("AnimSeqName")),
                                f(float(k.get("AnimStartOffset", 0.0) or 0.0)),
                                f(float(k.get("AnimEndOffset", 0.0) or 0.0)),
                                f(float(k.get("AnimPlayRate", 1.0) or 1.0)), "1" if k.get("bLooping") else "0",
                                "1" if k.get("bReverse") else "0"]))
    # ---- what each purchase does ----
    console_ids = {c["id"] for c in consoles}
    known = console_ids | volumes | set(data["clusters"]) | out_actors | trap_actors
    known |= {n for n in movers if (actors.get(n) or {}).get("class") in VOLUME_CLASSES}
    for name, t, verb, args in ops:
        if verb not in ("play", "reverse", "pause", "resume", "stop", "sound", "sound_stop"):
            args = [x for x in args if x in known]
            if not args:
                continue
        elif verb in ("sound", "sound_stop"):
            args = [args[0]] + [x for x in args[1:] if x in known]
        lines.append("\t".join(["do", name, f(t), verb] + [str(x) for x in args]))
    index = {n: i for i, n in enumerate(data["spawnpoints"])}
    for c, pts in sorted(data["clusters"].items()):
        lines.append("\t".join(["cluster", c] + [str(index[p_]) for p_ in pts if p_ in index]))
    del pkg

    # ---- writing ----
    if scenes:
        w.g["scenes"] = [{"nodes": s} for s in scenes]
        path = os.path.join(assets, "models", f"{key}_doors.glb")
        size = w.save(path)
        print(f"wrote {path} ({size / 1e6:.2f} MB, {len(scenes)} scenes)")
    groups.write(os.path.join(assets, "levels", f"{key}_doors.bin"))
    with open(os.path.join(assets, "levels", f"{key}_doors.txt"), "w") as fh:
        fh.write("\n".join(lines) + "\n")
    # The map's model and collision without them.
    for parent in g["nodes"]:
        if "children" in parent:
            parent["children"] = [c for c in parent["children"] if c not in take_out]
    for sc in g["scenes"]:
        sc["nodes"] = [c for c in sc["nodes"] if c not in take_out]
    rc.write_glb(glb_path, g, rest)
    if drop:
        keep = np.array([j not in drop for j in range(len(col_tris))], bool)
        rc.write_col(col_path, origin, step, col_tris[keep], col_vol)
    print(f"{key}: {len(consoles)} consoles, {len(mats)} Matinees, {len(out_actors)} doors and places, "
          f"{len(groups.groups)} collision groups; {len(take_out)} nodes out of the map's scene, "
          f"{len(drop)} collision triangles out of its own")


def main():
    if len(sys.argv) < 5 or len(sys.argv) % 2 == 0:
        sys.exit(__doc__)
    cooked, assets = sys.argv[1], sys.argv[2]
    pairs = list(zip(sys.argv[3::2], sys.argv[4::2]))
    # (Ancients first: Oblivion takes its materials.)
    pairs.sort(key=lambda kv: kv[0] != MATERIAL_DONOR)
    console_done = False
    for key, export_root in pairs:
        if key not in bm.MAPS:
            sys.exit(f"unknown map {key}")
        if not console_done and key == CONSOLE_MAP:
            build_console(cooked, assets, export_root)
            build_floor_console(cooked, assets, export_root)
            console_done = True
        build_map_doors(key, export_root, cooked, assets)
    if not console_done:
        print(f"(models/esc_console.glb and esc_console_floor.glb not built: they take {CONSOLE_MAP}'s export)")


if __name__ == "__main__":
    main()
