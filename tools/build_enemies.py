"""Build Escalation's enemies for the demo: every AI type the four maps' spawn lists
field (tools/enemy_data.py reads them from the game), each as the game builds it.

  * models/enemies/<type>.glb: its skeleton and meshes (a mesh set's parts, one
    node per part, every alternative of every slot, named s<slot>_<n>; a static part
    hung on its socket's bone as the game hangs it), on the texture sets below,
    and its own clips (its idle; a type with a skeleton of its own its deaths too);
  * models/enemies/npc_clips.glb: the soldiers' clips (their skeleton,
    TR_Skeletons_p.RB_NpcAi_SKEL, is the same for every one of them): the idle, and
    the deaths their death set plays before it lets them fall (NPC_DEATHS: the
    Death_Gunfire_Front chooser group's, the headshot's, the electrocution's, the
    Corrosive Slime Cannon's stagger and the Energon Harvester's reach; the rest of
    TR_Deaths_p.Sets.Basic_DEATHSET's deaths are ragdolls from the start);
  * models/enemies/textures/*.ktx2: the texture sets (build_class.py's Textures:
    colour with the paint mask, normals, metal-roughness, glow);
  * models/enemies/enemies.txt: what src/enemies.rs reads (below).

A model's root is turned as the demo's models are (build_class.py's YAW) and
scaled by its mesh set's MeshScale, and placed by its MeshTranslation (the pawn's
place is its collision cylinder's middle; the model's root is at its feet).

enemies.txt (tab-separated; lengths in metres, glTF axes):
  map    <map> <type ids...>                     each map's roster, in its spawn list's order
  type   <id> <model> <form> <name> <faction> <ai type> <score> <health> <damage multiplier>
         <radius> <half height> <energon r g b a> <scale>
         (form: robot, jet, orb or insect: the one the type starts in)
  colours <id> <r g b> <r g b>                   a colour pair (sRGB 0-255), one line each
  slot   <id> <slot> <node,node,...>             a slot's alternatives ("-" for none)
  body   <id> <bone> box <cx cy cz> <ax ay az> <bx by bz> <cx cy cz> <hx hy hz>
         <id> <bone> capsule <ax ay az> <bx by bz> <radius>
         <id> <bone> sphere <cx cy cz> <radius>
         <id> <bone> convex <n> <x y z>... <m> <nx ny nz d>...
         (in the bone's space: a box's centre, axes and half sizes; a capsule's two
         end centres and radius; a convex hull's corners and its faces' planes,
         outward normal and distance: n . p = d on the face)
  mass   <id> <bone> <mass>                      a body's mass where its physics asset sets it
                                                 (High Moon's RB_BodySetup.Mass, PhysX's units)
  joint  <id> <child bone> <parent bone> <pos1 xyz> <pri1 xyz> <sec1 xyz>
         <pos2 xyz> <pri2 xyz> <sec2 xyz> <swing1> <swing2> <twist>
         (the ragdoll's joints: the joint's place and its twist and secondary axes
         in the child's bone space, then in the parent's; its limits in degrees, "-"
         for a free axis: enemy_data.py's constraints)
  socket <id> <name> <bone> <tx ty tz> <qx qy qz qw> [<sx sy sz>]
                                                 its sockets (TargetingSocket...): its place
                                                 and turn on its bone, and its scale (Unreal's
                                                 axes: its RelativeScale) when it isn't 1
  gun    <id> <hand> <node>                      its gun's mesh for each hand (r, l: gun_r, gun_l)
  gunsocket <id> <hand> <name> <bone> <tx ty tz> <qx qy qz qw>
                                                 that mesh's sockets (MuzzleFlash...)
  vgun   <id> <n> <node> <weapon class>          a vehicle gun's mesh on its n-th attachment
                                                 socket (a jet's dogfight gun's launchers:
                                                 vgun0, vgun1), shown while it has that gun out
  vgunsocket <id> <n> <name> <bone> <tx ty tz> <qx qy qz qw>
                                                 that mesh's sockets (its bones vgun<n>:<bone>)
  clip   <id> <role> <file> <index> <seconds> <loop 0/1>
         (role: idle, or a death's clip by the name its death set plays it by:
         Death_Gunfire_Front (a chooser group: one of them at random),
         Death_Headshot03... (NPC_DEATHS), a type's own (OWN_DEATHS); a role may
         have several)
  other  <id> <other id>                         a type that changes form as it fights
                                                 (TRANSFORMING) and the type its other
                                                 form is: <id>~other, <id>_other.glb
                                                 (not in any roster)
  score  <ai type> <money>                       Escalation's kill money by type
  difficulty <multipliers...>

usage: python build_enemies.py <umodel export dir> <CookedPC> <foc_esc/assets> [enemy_data.json]
       python build_enemies.py --umodel <CookedPC> <umodel export dir> [enemy_data.json]
           prints the umodel commands that export what this reads
"""
import json
import math
import os
import struct
import sys

import numpy as np

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.path.join(HERE, "ue3"))
sys.path.insert(0, HERE)

import build_class as bc  # noqa: E402
import class_data as cd  # noqa: E402
import enemy_data  # noqa: E402
from gltf_io import Doc, Writer  # noqa: E402

OUT_DIR = os.path.join("models", "enemies")
SHARED = "all_shared.xxx"
# The soldiers' skeleton and the anim sets their clips come from (all_shared).
NPC_SKELETON = "RB_NpcAi_SKEL"
NPC_SETS = ["Death_Shared_ANIM", "HitReactions_Shared_ANIM", "Shared_Soldier_AI_ANIM", "SOLDIER_LOCOMOTION_ANIM"]
# (More shared sets, whose clips the enemies' anim trees play: build_ai_anims.py.)
AI_ANIM_SETS = ["AI_HitReacts_Shared_ANIM", "Shared_ROBO_ANIM", "Shared_ROBO_NAV_ANIM"]
# The clips the soldiers' deaths play (their death set's TnDeathActionPlayAnimation
# names: Death_Shared_ANIM's chooser group, whose clips are picked at random, or
# clips), by role: Gunfire_Front_DEATH (a ballistic kill: the death set takes the
# first death that matches, and none of the ones after it from behind or the side
# is ever reached), Headshot01/02_DEATH, Electrocute01/02_DEATH (energy), Slow_DEATH
# and Corrosion_DEATH (the Corrosive Slime Cannon), DrainGun_DEATH (the Energon
# Harvester).
NPC_DEATHS = {
    "Death_Gunfire_Front": "Death_Gunfire_Front",
    "Death_Headshot03": ["Death_Headshot03"],
    "Death_Electrocute": ["Death_Electrocute"],
    "Death_ProjectileMult_Stagger_F": ["Death_ProjectileMult_Stagger_F"],
    "Death_ProjectileSing_Reaching": ["Death_ProjectileSing_Reaching"],
    "Death_BlowBack": ["Death_BlowBack"],
    "Death_EnergyRanged_B_01": ["Death_EnergyRanged_B_01"],
    "Death_EnergyRanged_B_02": ["Death_EnergyRanged_B_02"],
}
# Each type's idle (its anim tree's idle node: the soldiers' patrol stance, as they
# stand before they are aware of anyone) and the clips of a type with its own
# skeleton (by role: its death set's).
IDLES = {
    "Soldier_ANIMTREE": "AI_Nav_Idle_Patrol",
    "RocketTrooper_ANIMTREE": "AI_Nav_Idle_Patrol",
    "Shotgunner_ANIMTREE": "AI_Nav_Idle_Patrol",
    "Scrapper_ANIMTREE": "AI_Nav_Idle_Patrol",
    "Titan_ANIMTREE": "NAV_Idle",
    "SniperRobot_ANIMTREE": "NAV_Idle",
    "SniperVehicle_ANIMTREE": "NAV_Idle",
    "Guard_ANIMTREE": "NAV_Inv_Idle",
    "Leaper_ANIMTREE": "NAV_Idle",
    "Swarmer_ANIMTREE": "Nav_Idle_03",
    "VEH_Plane_ANIMTREE": "Nav_Hover_Pose_VEH",
}
# (Titan_DEATHSET's Generic_DEATH, Guardian_DEATHSET's gunfire, energy and slime
# deaths, Leaper_DEATHSET's own (its blows', its fire's), the Swarmer's
# electrocution, the Sniper's insect's (on a wall, on a ceiling, a headshot, an
# electrocution): each by the name its death plays it by.)
SNIPER_DEATHS = {c: [c] for c in ("Death_01", "Death_02", "Death_Electrocute", "Death_Headshot_01")}
OWN_DEATHS = {
    "Titan": {"Death": ["Death"]},
    "HeavyGunnerAut": {"Death": ["Death"]},
    "HeavyGunnerDec": {"Death": ["Death"]},
    "Guard": {"Death_01": ["Death_01"]},
    "Leaper": {c: [c] for c in ("Leaper_Death_01", "Leaper_Death_02", "FireDeath")},
    "SwarmerLite": {"Death_Electrocute": ["Death_Electrocute"]},
    "Sniper_other": SNIPER_DEATHS,
    "Sniper_AUT_other": SNIPER_DEATHS,
}
# (Every socket of a type's skeleton goes in enemies.txt: src/enemies.rs's effect
# sockets (TargetingSocket, Chest_FX...), the hands' its AI's grenade throws name,
# and what its AI fires from or plays its animations' effects on (their
# HmAnimNotify_PlayEffect and TnAnimNotify_SpawnProjectile notifies), its
# scanner (the Guard's ScannerSocket), its drones' (the Titan's SpawnSockets)...)
# Unreal units to metres, and Unreal's axes to umodel's glTF ones (Y and Z swapped).
UU = 0.01
# Mesh sections whose material isn't cooked (umodel's dummy_material): the texture
# set they are drawn with, by mesh (the Scrapper's chest's inner piece, its chest's).
MATERIAL_FALLBACK = {"RB_Scrapper_CHARSKEL_Chest": "RB_Hound_Chest_TEXSET"}
# The AI's arm guns (their weapons' TnWeaponMesh handedness meshes): each one's upper
# arm piece is RB_ArmUp_Cust_MatINST_INST, cooked into all_shared only, on
# WEP_ArmUp_TEXSET (the players' Neutron Assault Rifle's arm).
for _hand in "RL":
    for _gun in ("RB_BurstRifleAI%s_SKEL", "RB_ShotgunArmAI%s_SKEL", "RB_SharpShotAI%s_SKEL"):
        MATERIAL_FALLBACK[_gun % _hand] = "WEP_ArmUp_TEXSET"
# The types the base game shows in their own material's paint (its Cust_Color_A and
# Cust_COLOR_B) rather than in their character's colours (CharacterBlueprint's
# PrimaryColor and SecondaryColor), by type and material: Ancients' and Ignition's
# Decepticon Snipers are lavender with gunmetal trim in the game (RB_SniperDec_MatInst;
# its insect's VH_SniperDec_MatInst is the same), where their blueprint gives red and
# gold. (Their description has no CharacterColorPairs to paint them over with,
# ApplyCharacterBlueprintOverride; the game's own picture shows it.)
OWN_PAINT = {"Sniper": "RB_SniperDec_MatInst"}
# The names the game shows for the AI types (TransGame.int's [AICharacterNames], by
# each description's CharacterName), from Localization/Cooked/PC/Coalesced.int.
COALESCED = os.path.join("Localization", "Cooked", "PC", "Coalesced.int")
NAMES_FILE = "TransGame/Localization/INT/TransGame.int"


def swap(v):
    return [float(v[0]), float(v[2]), float(v[1])]


def socket_frame(s):
    """A socket's place on its bone (metres) and its turn there (a quaternion), in
    glTF's axes (build_class.py's gltf_socket)."""
    t, q, _ = bc.gltf_socket(s)
    return t, q


def fmt(v):
    return " ".join("%.5g" % x for x in v)


def linear_to_srgb(c):
    """A material's linear colour (r, g, b, ...) as sRGB 0-255 (the colours lines')."""
    out = []
    for x in c[:3]:
        x = min(max(float(x), 0.0), 1.0)
        out.append(round(255 * (12.92 * x if x <= 0.0031308 else 1.055 * x ** (1 / 2.4) - 0.055)))
    return out


def own_paint(materials, tid):
    """The colour pair of a type painted in its own material's paint (OWN_PAINT), or
    None."""
    name = OWN_PAINT.get(tid.split("~")[0])
    v = ((materials.get(name) or {}) if name else {}).get("vectors", {})
    if "Cust_Color_A" in v and "Cust_COLOR_B" in v:
        return [linear_to_srgb(v["Cust_Color_A"]), linear_to_srgb(v["Cust_COLOR_B"])]
    return None


def form_of(t):
    if not t["start_alt"] or not t["alt"]:
        return "robot", t["robot"]
    kind = t["alt"]["kind"]
    return ("jet" if "Plane" in kind and "Orb" not in str(t["alt"]["slots"]) else
            "orb" if "Orb" in str(t["alt"]["slots"]) else "insect"), t["alt"]


# The blueprints whose characters change form as they fight: their other form is
# built too, as a type of its own (<id>~other, <id>_other.glb), and linked to
# theirs (an other line): the Snipers' insects on their perches, the jets' robots.
TRANSFORMING = ("TnSniperCharacterBlueprint", "TnJetCharacterBlueprint")


def other_form_of(t):
    """The form a transforming type doesn't start in: its kind and its form."""
    if t["start_alt"] or not t["alt"]:
        return "robot", t["robot"]
    kind = t["alt"]["kind"]
    return ("jet" if "Plane" in kind else "insect"), t["alt"]


def tree_name(f):
    return (f.get("anim_tree") or "").split(".")[-1]


def exported(ex, part):
    kind = "SkeletalMesh3" if "mesh" in part else "StaticMesh3"
    return ex.find(kind, (part.get("mesh") or part.get("static")) + ".gltf")


def skinned_parts(ex, f):
    """The form's skinned meshes, their exports, by slot."""
    out = []
    for si, slot in enumerate(f["slots"]):
        for ai, p in enumerate(slot):
            if p and "mesh" in p:
                path = exported(ex, p)
                if path:
                    out.append((si, ai, p, path))
                else:
                    print("  ! not exported:", p)
    return out


def bodies(f):
    """The form's physics bodies in their bones' space (glTF axes, metres): boxes
    (centre, axes, half sizes), capsules (two end centres, radius), spheres."""
    out = []
    for b in f["physics"]:
        for e in b["boxes"]:
            tm = np.array(e["tm"]).reshape(4, 4)
            out.append((b["bone"], "box", [swap(tm[3, :3] * UU), swap(tm[0, :3]), swap(tm[1, :3]), swap(tm[2, :3]),
                                           [e["x"] * 0.5 * UU, e["y"] * 0.5 * UU, e["z"] * 0.5 * UU]]))
        for e in b["sphyls"]:
            tm = np.array(e["tm"]).reshape(4, 4)
            c, z = tm[3, :3], tm[2, :3] / max(np.linalg.norm(tm[2, :3]), 1e-6)
            half = e["length"] * 0.5
            out.append((b["bone"], "capsule", [swap((c - z * half) * UU), swap((c + z * half) * UU), [e["radius"] * UU]]))
        for e in b["spheres"]:
            tm = np.array(e["tm"]).reshape(4, 4)
            out.append((b["bone"], "sphere", [swap(tm[3, :3] * UU), [e["radius"] * UU]]))
        for e in b.get("convex", []):
            points = [swap(np.array(p) * UU) for p in e["points"]]
            planes = [swap(p[:3]) + [p[3] * UU] for p in e["planes"]]
            out.append((b["bone"], "convex", [[len(points)]] + points + [[len(planes)]] + planes))
    return out


def masses(f):
    """The form's bodies' masses where its physics asset sets them (RB_BodySetup.Mass,
    PhysX's units: what an impulse on them is against; the others' come from their
    shapes): (bone, mass)."""
    return [(b["bone"], b["mass"]) for b in f["physics"] if b.get("mass")]


def joints(f):
    """The form's ragdoll joints (glTF axes, metres): (child, parent, the frames'
    places and axes, the limits)."""
    out = []
    lim = lambda a: "-" if a is None else "%g" % a  # noqa: E731
    for j in f.get("joints", []):
        frames = [swap(j[k]) for k in ("pos1", "pri1", "sec1", "pos2", "pri2", "sec2")]
        out.append((j["child"], j["parent"], frames, [lim(j["swing1"]), lim(j["swing2"]), lim(j["twist"])]))
    return out


def root(w, scale, translation, half_height=None):
    h = math.radians(bc.YAW) / 2
    t = translation or [0.0, 0.0, 0.0]
    # (MeshTranslation's Z puts the feet on the floor: the model's root is the
    # cylinder's foot, the mesh below it by what the translation goes past the
    # cylinder's half height (a Sniper's legs hang 75 uu below its, as it flies);
    # its X and Y move the mesh in the pawn's frame: Unreal's forward is the
    # model's -Z, after the turn.)
    drop = (t[2] + half_height) * UU if half_height else 0.0
    return w.node({"name": "Root", "rotation": [0, math.sin(h), 0, math.cos(h)], "scale": [scale] * 3,
                   "translation": [-t[1] * UU * 0.0, drop, -t[0] * UU], "children": []}, root=True)


def socket_node(w, names, s, mesh_name):
    """A static part on its socket: a node under the socket's bone (gltf_socket's
    transform), the mesh under it."""
    t, q, sc = bc.gltf_socket(s)
    bone = names.get(s["bone"])
    if bone is None:
        print("  ! socket bone not on the skeleton:", s["name"], s["bone"])
        return None
    n = w.node({"name": "%s@%s" % (mesh_name, s["name"]), "translation": list(t), "rotation": list(q), "scale": list(sc),
                "children": []})
    w.g["nodes"][bone].setdefault("children", []).append(n)
    return n


def _names(mp, value):
    """An array of names (a property's raw value): the names, in order."""
    if not (isinstance(value, tuple) and value and value[0] == "array"):
        return []
    raw = value[1]
    n = struct.unpack_from("<i", raw, 0)[0]
    return [mp.pkg.names[struct.unpack_from("<i", raw, 4 + 8 * i)[0]] for i in range(n)]


# all_shared.xxx (main and umodel_commands open it): where the meshes a map's
# package only imports are cooked.
_shared = []


def mesh_sockets_of(mp, mesh):
    """A skeletal mesh's name and its sockets: its package's, or all_shared's for one
    it only imports (the Titan's X18 Scrapmaker arms, WEP_HeavyMG_p's)."""
    name = mp.name(mesh)
    if mesh > 0:
        return name, mp.sockets(mesh)
    for sh in _shared:
        for j in sh.by_name.get(name, []):
            if sh.pkg.class_name(sh.pkg.exports[j - 1]) == "SkeletalMesh":
                return name, sh.sockets(j)
    return name, []


def open_shared(cooked):
    if not _shared:
        _shared.append(cd.Mp(cooked, SHARED))


# mp_shared.xxx: where the jets' dogfight gun is cooked (TnWeaponRocketVehicle, its
# Rocket_Vehicle_WEPMESH and its launcher, WEP_RocketLauncherVH_SKEL: the trucks'
# rockets), which the maps only name.
MP_SHARED = "mp_shared.xxx"
_mp_shared = []


def open_mp_shared(cooked):
    if not _mp_shared:
        _mp_shared.append(cd.Mp(cooked, MP_SHARED))
    return _mp_shared[0]


def vehicle_weapon_meshes(mp, t, cooked):
    """A jet's vehicle gun (its BehaviorBlueprint's DogfightWeapon, a TnWeaponVehicle:
    WT_AltForm, its WeaponMeshTemplate's AnimatedMesh hung on its plane's sockets, one
    on each of the class's _AttachmentSockets, as TnWeapon.AttachWeaponMeshes hangs
    them), each (index, mesh name, its sockets, the socket it hangs on, the class);
    [] for a type with none."""
    desc = mp.find(t["desc"])
    bp = cd._obj(mp.props(desc).get("BehaviorBlueprint")) if desc else 0
    props = mp.props(bp) if bp else {}
    weapon = cd._obj(props.get("DogfightWeapon"))
    if not weapon:
        return []
    cls = mp.name(weapon)
    msh = open_mp_shared(cooked)
    default = msh.find("TransContent.Default__" + cls) or msh.find("TransGame.Default__" + cls)
    template = cd._obj(msh.props(default).get("WeaponMeshTemplate")) if default else 0
    animated = cd._obj(msh.props(template).get("AnimatedMesh")) if template > 0 else 0
    comp = cd._obj(msh.props(animated).get("Mesh")) if animated > 0 else 0
    mesh = cd._obj(msh.props(comp).get("SkeletalMesh")) if comp > 0 else 0
    if not mesh:
        print("  ! no mesh for the vehicle gun", t["id"], cls)
        return []
    name, socks = mesh_sockets_of(msh, mesh)
    return [(n, name, socks, socket, cls)
            for n, socket in enumerate(_names(msh, msh.props(default).get("_AttachmentSockets")))]


def hang_mesh(w, mats, names, d, mesh, sock, node_name):
    """A mesh with a skeleton of its own (umodel's export `d`) hung on a socket of
    the pawn's skeleton at its reference pose (it has no animations): its bones
    copied under the socket, named <node>:<bone>, the mesh skinned to them (a skin of
    its own) and kept with them (so its bounds go where it is drawn). Its node, or
    None."""
    holder = socket_node(w, names, sock, node_name) if sock else None
    if holder is None:
        return None
    first = len(w.g["nodes"])
    _, _, joints = bc.copy_skeleton(w, d, holder)
    for n in w.g["nodes"][first:]:
        n["name"] = "%s:%s" % (node_name, n["name"])
    ibm = d.acc(d.g["skins"][0]["inverseBindMatrices"]).astype(np.float32)
    w.g["skins"].append({"joints": joints, "inverseBindMatrices": w.accessor(ibm), "skeleton": joints[0]})
    prims = bc.primitives(w, mats, d, d.g["nodes"][0]["mesh"], mesh)
    if not prims:
        return None
    w.g["meshes"].append({"name": mesh, "primitives": prims})
    node = w.node({"name": node_name, "mesh": len(w.g["meshes"]) - 1, "skin": len(w.g["skins"]) - 1})
    w.g["nodes"][holder]["children"].append(node)
    return node


def class_default(mp, cls, prop):
    """Class `cls`'s default value of property `prop` as the game has it: its default
    object's, else its superclass's (a cooked default object holds only what differs
    from its superclass's: the Heavy Gunner's TnWeaponHeavyGunnerMG sets its own
    weapon data but takes TnWeaponTitanHeavyMG's WeaponMeshTemplate, the Titan's
    machine gun arm). None if none sets it."""
    for _ in range(16):
        default = mp.find("TransContent.Default__" + cls) or mp.find("TransGame.Default__" + cls)
        value = mp.props(default).get(prop) if default else None
        if value is not None:
            return value
        klass = mp.find("TransContent." + cls) or mp.find("TransGame." + cls)
        sup = mp.pkg.exports[klass - 1]["super"] if klass else 0
        if not sup:
            return None
        cls = mp.pkg.obj_name(sup)
    return None


def weapon_meshes(mp, t):
    """The type's gun as its blueprint arms it (its BehaviorBlueprint's PrimaryWeapon,
    or the Guard's BattleModeWeapon; that class's WeaponMeshTemplate), each
    (hand "r" or "l", mesh name, its sockets, the socket it hangs on): its
    handedness meshes (Meshes: skinned to the pawn's skeleton, following its pose,
    hung on none), or its AnimatedMesh (a skeletal mesh of its own, the Guard's arm
    cannon: hung on the pawn's socket for each hand, the class's
    _AttachmentSockets, at that mesh's reference pose: it has no animations); []
    for a type with none (its gun is its body's, or it fights hand to hand)."""
    desc = mp.find(t["desc"])
    bp = cd._obj(mp.props(desc).get("BehaviorBlueprint")) if desc else 0
    props = mp.props(bp) if bp else {}
    weapon = cd._obj(props.get("PrimaryWeapon")) or cd._obj(props.get("BattleModeWeapon"))
    if not weapon:
        return []
    cls = mp.name(weapon)
    default = mp.find("TransContent.Default__" + cls) or mp.find("TransGame.Default__" + cls)
    template = cd._obj(class_default(mp, cls, "WeaponMeshTemplate"))
    if not template:
        return []
    out = []
    for el in cd._structs(mp.pkg, mp.props(template).get("Meshes")):
        mesh = cd._obj(el.get("SkelMesh"))
        hand = {"HANDEDNESS_Right": "r", "HANDEDNESS_Left": "l"}.get(str(el.get("Handedness")))
        if mesh and hand:
            out.append((hand, *mesh_sockets_of(mp, mesh), None))
    animated = cd._obj(mp.props(template).get("AnimatedMesh"))
    comp = cd._obj(mp.props(animated).get("Mesh")) if animated > 0 else 0
    mesh = cd._obj(mp.props(comp).get("SkeletalMesh")) if comp > 0 else 0
    if not out and mesh:
        for hand, socket in zip("rl", _names(mp, mp.props(default).get("_AttachmentSockets"))):
            out.append((hand, *mesh_sockets_of(mp, mesh), socket))
    return out


def build_model(t, ex, textures, game_mats, out, sockets_by_mesh, clip_sets, weapon=(), form=None,
                stem=None, vweapons=()):
    """<type>.glb (<stem>.glb: the form given, (kind, form), else the one it starts
    in); returns its slot nodes, its clips (role, index, seconds, loop) and its
    sockets. `vweapons`: its vehicle gun's meshes (vehicle_weapon_meshes)."""
    kind, f = form or form_of(t)
    stem = stem or t["id"]
    parts = skinned_parts(ex, f)
    if not parts:
        print("  ! no meshes for", t["id"])
        return None
    w = Writer(share=True, image_dir=os.path.join(out, "textures"), image_uri="textures")
    mats = bc.Materials(w, textures, game_mats)
    r = root(w, f["scale"], f["translation"], (f["collision"] or [0, 0])[1])
    ref = Doc(parts[0][3])
    _, _, joints = bc.copy_skeleton(w, ref, r)
    ref_names = [ref.g["nodes"][j]["name"] for j in ref.g["skins"][0]["joints"]]
    names = {n: joints[k] for k, n in enumerate(ref_names)}
    ref_ibm = ref.acc(ref.g["skins"][0]["inverseBindMatrices"]).astype(np.float32)
    w.g["skins"].append({"joints": joints, "inverseBindMatrices": w.accessor(ref_ibm), "skeleton": joints[0]})
    slots = {}
    for si, ai, p, path in parts:
        d = Doc(path)
        pn = [d.g["nodes"][j]["name"] for j in d.g["skins"][0]["joints"]]
        missing = [n for n in pn if n not in ref_names]
        if missing:
            print("  ! joints not on the skeleton:", p["mesh"], missing[:4])
        jmap = np.array([ref_names.index(n) if n in ref_names else 0 for n in pn])
        prims = bc.primitives(w, mats, d, d.g["nodes"][0]["mesh"], p["mesh"], jmap)
        if not prims:
            continue
        node_name = "s%d_%d" % (si, ai)
        w.g["meshes"].append({"name": p["mesh"], "primitives": prims})
        w.g["nodes"][r]["children"].append(w.node({"name": node_name, "mesh": len(w.g["meshes"]) - 1, "skin": 0}))
        slots.setdefault(si, {})[ai] = node_name
    # Static parts: on the first skinned part's socket of that name.
    for si, slot in enumerate(f["slots"]):
        for ai, p in enumerate(slot):
            if not p or "static" not in p:
                continue
            path = exported(ex, p)
            sock = next((s for m in [q[2]["mesh"] for q in parts] for s in sockets_by_mesh.get(m, [])
                         if s["name"].startswith(p["socket"])), None)
            if not path or not sock:
                print("  ! static part not placed:", p, bool(path), bool(sock))
                continue
            d = Doc(path)
            prims = bc.primitives(w, mats, d, d.g["nodes"][0]["mesh"], p["static"])
            holder = socket_node(w, names, sock, p["static"])
            if holder is None or not prims:
                continue
            node_name = "s%d_%d" % (si, ai)
            w.g["meshes"].append({"name": p["static"], "primitives": prims})
            n = w.node({"name": node_name, "mesh": len(w.g["meshes"]) - 1})
            w.g["nodes"][holder]["children"].append(n)
            slots.setdefault(si, {})[ai] = node_name
    for si, slot in enumerate(f["slots"]):
        for ai, p in enumerate(slot):
            if p is None:
                slots.setdefault(si, {})[ai] = "-"
    # Its gun, as gun_r and gun_l; the one for its hand is shown while it holds it
    # (src/ai/mod.rs). Each handedness mesh skinned to its skeleton (the weapon's
    # mesh follows the pawn's pose: its AnimatedMesh's UseParentAnimComponent); or
    # a mesh with a skeleton of its own hung on the pawn's socket for that hand (the
    # Guard's arm cannon on WeaponSocket_Primary, as TnWeapon.AttachWeaponMeshes
    # attaches it): its bones copied under that socket, named <node>:<bone>, the
    # mesh skinned to them (a skin of its own) and kept with them (so its bounds go
    # where it is drawn).
    guns = []
    own_sockets = [s for m in [f.get("base")] + [q[2]["mesh"] for q in parts] for s in sockets_by_mesh.get(m, [])]
    for hand, mesh, socks, attach in weapon:
        path = ex.find("SkeletalMesh3", mesh + ".gltf")
        if not path:
            print("  ! gun not exported:", mesh)
            continue
        d = Doc(path)
        node_name = "gun_" + hand
        pn = [d.g["nodes"][j]["name"] for j in d.g["skins"][0]["joints"]]
        if attach:
            sock = next((s for s in own_sockets if s["name"] == attach), None)
            if hang_mesh(w, mats, names, d, mesh, sock, node_name) is None:
                print("  ! gun not hung on its socket:", mesh, attach)
                continue
            guns.append((hand, node_name, [(s["name"], "%s:%s" % (node_name, s["bone"]), *socket_frame(s))
                                           for s in socks if s["bone"] in pn]))
            continue
        missing = [n for n in pn if n not in ref_names]
        if missing:
            print("  ! gun joints not on the skeleton:", mesh, missing[:4])
        jmap = np.array([ref_names.index(n) if n in ref_names else 0 for n in pn])
        prims = bc.primitives(w, mats, d, d.g["nodes"][0]["mesh"], mesh, jmap)
        if not prims:
            continue
        w.g["meshes"].append({"name": mesh, "primitives": prims})
        w.g["nodes"][r]["children"].append(w.node({"name": node_name, "mesh": len(w.g["meshes"]) - 1, "skin": 0}))
        guns.append((hand, node_name, [(s["name"], s["bone"], *socket_frame(s))
                                       for s in socks if s["bone"] in names]))
    # Its vehicle gun (a jet's dogfight gun's launchers), vgun<n> on its n-th
    # attachment socket, hung as the Guard's arm cannon is; shown while it has that
    # gun out (src/ai/mod.rs).
    vguns = []
    for n, mesh, socks, attach, cls in vweapons:
        path = ex.find("SkeletalMesh3", mesh + ".gltf")
        if not path:
            print("  ! vehicle gun not exported:", mesh)
            continue
        d = Doc(path)
        node_name = "vgun%d" % n
        pn = [d.g["nodes"][j]["name"] for j in d.g["skins"][0]["joints"]]
        sock = next((s for s in own_sockets if s["name"] == attach), None)
        if hang_mesh(w, mats, names, d, mesh, sock, node_name) is None:
            print("  ! vehicle gun not hung on its socket:", mesh, attach)
            continue
        vguns.append((n, node_name, cls, [(s["name"], "%s:%s" % (node_name, s["bone"]), *socket_frame(s))
                                          for s in socks if s["bone"] in pn]))
    # Its clips: the idle (every model has its own, so its bones are animation
    # targets), and the deaths of a type with a skeleton of its own.
    clips = []
    idle = IDLES.get(tree_name(f))
    rest = bc.robot_rest(ref)
    sets = f["anim_sets"] + (NPC_SETS if NPC_SKELETON == f.get("base") else [])
    roles = [("idle", idle, True)] if idle else []
    if NPC_SKELETON != f.get("base"):
        for role, cl in OWN_DEATHS.get(stem, {}).items():
            roles += [(role, c, False) for c in cl]
    for role, clip, loop in roles:
        bones, dt = bc.clip_tracks(ex, sets, clip)
        if bones is None:
            print("  ! clip not exported:", t["id"], role, clip)
            continue
        length = bc.base_clip(w, names, clip, bones, dt, loop=loop, rest=rest,
                              skip=("Root_Reference",) if loop else ())
        clips.append((role, stem + ".glb", len(w.g["animations"]) - 1, length, loop))
    w.save(os.path.join(out, stem + ".glb"))
    # Its sockets: the skeleton's (its base skeleton's, else its parts').
    socks = []
    for m in [f.get("base")] + [q[2]["mesh"] for q in parts]:
        for s in sockets_by_mesh.get(m, []):
            if s["name"] not in [x[0] for x in socks] and s["bone"] in names:
                socks.append((s["name"], s["bone"], *socket_frame(s), tuple(s.get("scale", (1.0, 1.0, 1.0)))))
    return {"slots": slots, "clips": clips, "sockets": socks, "names": names, "kind": kind, "form": f,
            "guns": guns, "vguns": vguns}


def build_npc_clips(ex, ref_path, chooser, out):
    """npc_clips.glb: the soldiers' skeleton with their idles and deaths; returns
    (role, index, seconds, loop) per clip."""
    w = Writer(share=True)
    r = w.node({"name": "Root", "children": []}, root=True)
    ref = Doc(ref_path)
    _, _, joints = bc.copy_skeleton(w, ref, r)
    ref_names = [ref.g["nodes"][j]["name"] for j in ref.g["skins"][0]["joints"]]
    names = {n: joints[k] for k, n in enumerate(ref_names)}
    rest = bc.robot_rest(ref)
    clips = []
    wanted = [("idle", "AI_Nav_Idle_Patrol", True)]
    for role, names_ in NPC_DEATHS.items():
        wanted += [(role, c, False) for c in (chooser.get(names_, []) if isinstance(names_, str) else names_)]
    for role, clip, loop in wanted:
        bones, dt = bc.clip_tracks(ex, NPC_SETS, clip)
        if bones is None:
            print("  ! soldier clip not exported:", clip)
            continue
        length = bc.base_clip(w, names, clip, bones, dt, loop=loop, rest=rest,
                              skip=("Root_Reference",) if loop else ())
        clips.append((role, "npc_clips.glb", len(w.g["animations"]) - 1, length, loop))
    size = w.save(os.path.join(out, "npc_clips.glb"))
    print(f"wrote npc_clips.glb ({len(clips)} clips, {size / 1e6:.1f} MB)")
    return clips


def chooser_groups(cooked, anim_set="Death_Shared_ANIM"):
    """An anim set's chooser groups (FoC's AnimSet ChooserGroups): {group: [clips]}."""
    sh = cd.Mp(cooked, SHARED)
    out = {}
    for i in sh.by_name.get(anim_set, []):
        for g in cd._structs(sh.pkg, sh.props(i).get("ChooserGroups")):
            out[str(g.get("GroupName"))] = [str(a.get("AnimName")) for a in cd._structs(sh.pkg, g.get("Anims"))]
    return out


def mesh_sockets(cooked, data):
    """Every skeletal mesh's sockets (the base skeletons' from all_shared: the
    soldiers', the Heavy Gunners' and the Titan's; the rest from their map's
    package): {mesh name: [socket]}."""
    out = {}
    sh = cd.Mp(cooked, SHARED)
    for base in {NPC_SKELETON} | {f.get("base") for t in data["types"].values()
                                  for f in (t["robot"], t["alt"]) if f and f.get("base")}:
        i = sh.find("TR_Skeletons_p." + base)
        out[base] = sh.sockets(i) if i else []
    by_pkg = {}
    for t in data["types"].values():
        for f in (t["robot"], t["alt"]):
            for slot in (f or {}).get("slots", []):
                for p in slot:
                    if p and "mesh" in p:
                        by_pkg.setdefault(t["package"], set()).add(p["mesh"])
    for pkg, meshes in by_pkg.items():
        mp = cd.Mp(cooked, pkg)
        for m in meshes:
            for j in mp.by_name.get(m, []):
                if mp.pkg.class_name(mp.pkg.exports[j - 1]) in ("SkeletalMesh", "SkeletalMeshSkeleton"):
                    out[m] = mp.sockets(j)
    return out


def game_materials(cooked, data):
    out = {}
    for pkg in sorted({t["package"] for t in data["types"].values()}):
        mp = cd.Mp(cooked, pkg)
        out[pkg] = mp
    return out


class PackageMaterials(dict):
    """build_class.py's game-material lookup, from one package (or all_shared, for a
    material it only imports: the Titan's arms' WEP_HeavyMG_MATINST; or mp_shared:
    the jets' dogfight gun's launchers' WEP_RocketLauncher_MATINST)."""

    def __init__(self, mp):
        super().__init__()
        self.mp = mp

    def get(self, name, default=None):
        if name not in self:
            src = self.mp if self.mp.by_name.get(name) else next(
                (sh for sh in _shared + _mp_shared if sh.by_name.get(name)), None)
            self[name] = src.material(name) if src else None
        return dict.get(self, name, default)


def display_names(cooked):
    """{CharacterName: the name the game shows}, or {} without the localization."""
    import build_menu
    path = os.path.join(os.path.dirname(os.path.abspath(cooked)), COALESCED)
    if not os.path.exists(path):
        print("  ! no", path, "- the types' own names are shown")
        return {}
    files = build_menu.coalesced(path)
    text = next((v for k, v in files.items() if k.endswith(NAMES_FILE)), "")
    return {k.split(".", 1)[1]: v for k, v in build_menu.ini(text).items() if k.startswith("AICharacterNames.")}


def main(export_dir, cooked, assets, data_path=None):
    data = json.load(open(data_path)) if data_path else enemy_data.read(cooked)
    open_shared(cooked)
    bc.MATERIAL_FALLBACK.update(MATERIAL_FALLBACK)
    shown = display_names(cooked)
    out = os.path.join(assets, OUT_DIR)
    os.makedirs(out, exist_ok=True)
    ex = bc.Exports(export_dir)
    textures = bc.Textures(ex, os.path.join(out, "textures"), os.path.join(assets, "models", "vehicle.glb"), cooked)
    sockets = mesh_sockets(cooked, data)
    pkgs = game_materials(cooked, data)
    chooser = chooser_groups(cooked)
    lines = ["# Escalation's enemies (tools/build_enemies.py): see its header for the fields"]
    for m, ids in data["maps"].items():
        lines.append("\t".join(["map", m] + ids))
    for ai, money in sorted(data["scoring"]["ai"].items()):
        lines.append("\t".join(["score", ai, "%g" % money]))
    lines.append("\t".join(["difficulty"] + ["%g" % x for x in data["scoring"]["difficulty"]]))
    npc_ref = None

    def type_lines(tid, t, built):
        f = built["form"]
        radius, height = f["collision"] if f["collision"][0] else t["robot"]["collision"]
        lines.append("\t".join(["type", tid, tid.replace("~", "_") + ".glb", built["kind"],
                                shown.get(t["name"], t["name"]), t["faction"].replace("kFaction", "") or "-",
                                t["ai_type"] or "-", "%g" % t["score"], "%g" % t["health"], "%g" % f["damage_multiplier"],
                                "%.4g" % (radius * UU), "%.4g" % (height * UU), fmt(t["energon"] or [1, 1, 1, 1]),
                                "%g" % f["scale"]]))
        own = own_paint(PackageMaterials(pkgs[t["package"]]), tid)
        for pair in [own] if own else t["colours"] or [t["own_colours"]]:
            if pair[0] and pair[1]:
                lines.append("\t".join(["colours", tid, fmt(pair[0]), fmt(pair[1])]))
        for si in sorted(built["slots"]):
            alts = built["slots"][si]
            lines.append("\t".join(["slot", tid, str(si), ",".join(alts[a] for a in sorted(alts))]))
        for bone, shape, vals in bodies(f):
            if bone in built["names"]:
                lines.append("\t".join(["body", tid, bone, shape] + [fmt(v) for v in vals]))
        for bone, mass in masses(f):
            if bone in built["names"]:
                lines.append("\t".join(["mass", tid, bone, "%g" % mass]))
        for child, parent, frames, limits in joints(f):
            if child in built["names"] and parent in built["names"]:
                lines.append("\t".join(["joint", tid, child, parent] + [fmt(v) for v in frames] + limits))
        for name, bone, at, turn, scale in built["sockets"]:
            # (Its scale when it has one: a jet's hover jets on HoverSocket_L/R, half
            # as tall: their Size, TnPlaneForm.Hovering.CalculateBoosterFxAmount.)
            extra = [fmt(scale)] if any(abs(x - 1.0) > 1e-4 for x in scale) else []
            lines.append("\t".join(["socket", tid, name, bone, fmt(at), fmt(turn)] + extra))
        for hand, node, socks in built["guns"]:
            lines.append("\t".join(["gun", tid, hand, node]))
            for name, bone, at, turn in socks:
                lines.append("\t".join(["gunsocket", tid, hand, name, bone, fmt(at), fmt(turn)]))
        for n, node, cls, socks in built.get("vguns", []):
            lines.append("\t".join(["vgun", tid, str(n), node, cls]))
            for name, bone, at, turn in socks:
                lines.append("\t".join(["vgunsocket", tid, str(n), name, bone, fmt(at), fmt(turn)]))
        for role, file, index, length, loop in built["clips"]:
            lines.append("\t".join(["clip", tid, role, file, str(index), "%.4g" % length, str(int(loop))]))
        if f.get("base") == NPC_SKELETON:
            lines.append("\t".join(["npc", tid]))

    for tid, t in data["types"].items():
        print("==", tid)
        weapon = weapon_meshes(pkgs[t["package"]], t)
        vweapons = vehicle_weapon_meshes(pkgs[t["package"]], t, cooked)
        built = build_model(t, ex, textures, PackageMaterials(pkgs[t["package"]]), out, sockets, None, weapon,
                            vweapons=vweapons)
        if not built:
            continue
        f = built["form"]
        if f.get("base") == NPC_SKELETON and npc_ref is None:
            npc_ref = skinned_parts(ex, f)[0][3]
        type_lines(tid, t, built)
        # Its other form (TRANSFORMING), a type of its own linked to it.
        if t["behaviour"] in TRANSFORMING:
            other = build_model(t, ex, textures, PackageMaterials(pkgs[t["package"]]), out, sockets, None, (),
                                form=other_form_of(t), stem=tid + "_other")
            if other:
                type_lines(tid + "~other", t, other)
                lines.append("\t".join(["other", tid, tid + "~other"]))
                if other["form"].get("base") == NPC_SKELETON and npc_ref is None:
                    npc_ref = skinned_parts(ex, other["form"])[0][3]
    if npc_ref:
        for role, file, index, length, loop in build_npc_clips(ex, npc_ref, chooser, out):
            lines.append("\t".join(["npcclip", role, file, str(index), "%.4g" % length, str(int(loop))]))
    open(os.path.join(out, "enemies.txt"), "w").write("\n".join(lines) + "\n")
    print("wrote", os.path.join(OUT_DIR, "enemies.txt"))


def umodel_commands(cooked, export_dir, data_path=None):
    data = json.load(open(data_path)) if data_path else enemy_data.read(cooked)
    open_shared(cooked)

    def q(p):
        return f'"{p}"' if " " in p else p
    meshes, anims = {}, {}
    mps = {}
    for t in data["types"].values():
        forms = [form_of(t)[1]] + ([other_form_of(t)[1]] if t["behaviour"] in TRANSFORMING else [])
        for f in forms:
            for slot in f["slots"]:
                for p in slot:
                    if p:
                        meshes.setdefault(t["package"], set()).add(p.get("mesh") or p.get("static"))
        mp = mps.get(t["package"]) or mps.setdefault(t["package"], cd.Mp(cooked, t["package"]))
        for _, mesh, _, _ in weapon_meshes(mp, t):
            # (A gun the map's package only imports from all_shared.)
            meshes.setdefault(t["package"] if mp.by_name.get(mesh) else SHARED, set()).add(mesh)
        for _, mesh, _, _, _ in vehicle_weapon_meshes(mp, t, cooked):
            meshes.setdefault(MP_SHARED, set()).add(mesh)
        for f in forms:
            for a in f["anim_sets"]:
                anims.setdefault(t["package"], set()).add(a)
    out = []
    for pkg in sorted(meshes):
        out.append(f"umodel -game=trans -export -gltf -path={q(cooked)} -out={q(export_dir)} {pkg} " +
                   " ".join(f"-obj={m}" for m in sorted(meshes[pkg])))
        # (Only the sets the package itself holds: one it imports, as Ignition's does
        # the hit reactions, comes with all_shared's below.)
        mp = mps.get(pkg) or mps.setdefault(pkg, cd.Mp(cooked, pkg))
        own = sorted(a for a in anims.get(pkg, ()) if a not in NPC_SETS
                     and a not in ("Shared_ROBO_ANIM", "Shared_ROBO_NAV_ANIM") and mp.by_name.get(a))
        if own:
            out.append(f"umodel -game=trans -export -path={q(cooked)} -out={q(export_dir)} {pkg} " +
                       " ".join(f"-obj={a}" for a in own))
    out.append(f"umodel -game=trans -export -path={q(cooked)} -out={q(export_dir)} {SHARED} " +
               " ".join(f"-obj={a}" for a in NPC_SETS))
    # (The shared sets whose clips build_ai_anims.py also reads: the hit reactions,
    # and the robots' shared sets, all_shared's and Downfall's cooked copy.)
    out.append(f"umodel -game=trans -export -path={q(cooked)} -out={q(export_dir)} {SHARED} " +
               " ".join(f"-obj={a}" for a in AI_ANIM_SETS))
    out.append(f"umodel -game=trans -export -path={q(cooked)} -out={q(export_dir)} {enemy_data.MAPS['downfall']} "
               "-obj=Shared_ROBO_ANIM")
    out.append(f"umodel -game=trans -export -png -path={q(cooked)} -out={q(export_dir)} {SHARED} "
               "-obj=WEP_ArmUp_TEXSET_Color_NormX -obj=WEP_ArmUp_TEXSET_Masks_NormY "
               "-obj=WEP_HeavyMG_TEXSET_Color_NormX -obj=WEP_HeavyMG_TEXSET_Masks_NormY")
    # The texture sets' pictures (their full-size mips are in the characters' TR_*_ROBO_p
    # and _VEH_p packages, which CookedPC needs): once the meshes are exported (run
    # this again), the sets the meshes' materials name, or a material's own pictures
    # (bc.PLAIN_TEXTURES), each from the first of the maps' packages (in
    # enemy_data.MAPS' order), mp_shared.xxx and all_shared.xxx that has it.
    order = list(enemy_data.MAPS.values()) + [MP_SHARED, SHARED]
    folders = [p for p in order if export_dir and os.path.isdir(os.path.join(export_dir, p[:-4]))]
    if not folders:
        out.append("# then run this again with the export folder: it lists the meshes' textures")
        return out
    def mp_of(p):
        if p not in mps:
            mps[p] = cd.Mp(cooked, p) if os.path.exists(os.path.join(cooked, p)) else None
        return mps[p]
    sets_of = {}
    for pkg in folders:
        for kind in ("SkeletalMesh3", "StaticMesh3"):
            d = os.path.join(export_dir, pkg[:-4], kind)
            for f in sorted(os.listdir(d)) if os.path.isdir(d) else []:
                if not f.endswith(".gltf"):
                    continue
                for m in json.load(open(os.path.join(d, f))).get("materials", []):
                    src = next((p for p in [pkg, MP_SHARED, SHARED] if mp_of(p) and mp_of(p).by_name.get(m["name"])),
                               None)
                    if src is None:
                        continue
                    info = mp_of(src).material(m["name"])
                    if info["texset"]:
                        objs = [info["texset"] + "_Color_NormX", info["texset"] + "_Masks_NormY"]
                    else:
                        objs = [t for k, t in info["textures"].items() if k in bc.PLAIN_TEXTURES.values()]
                    if objs:
                        sets_of.setdefault(pkg, []).append(objs)
    by_package, done = {}, set()
    for pkg in folders:
        for objs in sets_of.get(pkg, []):
            if tuple(objs) in done:
                continue
            src = next((p for p in order if mp_of(p) and all(mp_of(p).by_name.get(o) for o in objs)), None)
            if src:
                done.add(tuple(objs))
                by_package.setdefault(src, set()).update(objs)
    for pkg in order:
        if by_package.get(pkg):
            out.append(f"umodel -game=trans -export -png -path={q(cooked)} -out={q(export_dir)} {pkg} " +
                       " ".join(f"-obj={o}" for o in sorted(by_package[pkg])))
    return out


if __name__ == "__main__":
    if sys.argv[1] == "--umodel":
        print("\n".join(umodel_commands(sys.argv[2], sys.argv[3], sys.argv[4] if len(sys.argv) > 4 else None)))
    else:
        main(sys.argv[1], sys.argv[2], sys.argv[3], sys.argv[4] if len(sys.argv) > 4 else None)
