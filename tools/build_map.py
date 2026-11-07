"""Build one of the game's Escalation maps for the demo, straight from its packages.

For each map in MAPS (Downfall, ESC_Iacon; Oblivion, ESC_Ark; Ancients, ESC_Temple;
Ignition, ESC_Gladiator) it writes, with the parts it shares with build_level.py
(texture-set materials, BSP, baked lighting, lights, fog and grade):

  * every placed static mesh of the map's two packages (<map>_ART_m: the art,
    <map>_BASE_m: gameplay), at the level's own transforms, materials and light maps;
    a mesh section whose material umodel can't load (one the art package takes from
    the gameplay package) gets the material the mesh names (layout.py);
  * the level's BSP floors and walls;
  * the pickup bases the BASE package's purchase points stand on (what they sell,
    the health cubes, ammo crates, weapons and equipment over them, main.rs adds);
  * the transforming doors and covers (TnTransformingWorldActor), and on the maps
    marked "open" the level's other skeletal props (consoles, loot crates, doors)
    posed as the level leaves them, and its destructibles in their intact state;
  * on "open" maps, every area open: the doors, barriers and moving cover where the
    Matinees that the consoles' purchases play leave them, the transforming doors
    those purchases transform transformed (open), and nothing the level hides as
    gameplay starts (ue3/kismet.py); on the others, the paid gates left out;
  * its terrain (ue3/terrain.py), as the game draws it: smooth, its layers painted
    over each other by its weight maps, with its own light maps; and its collision;
  * the texture-set materials' glow as each material instance sets it (its
    EmissiveIntensity and Emissive_Color, ue3/matparams.py);
  * the sky (seen through the level's height fog, baked into the dome and the far
    pieces of sky) and the glowing, see-through, masked and animated surfaces, from
    their own materials (SPECIAL below), with glTF material extras ("foc") telling
    the demo which are additive, unlit, fog-free or flip books;
  * the effects (levelfx.py): the effect meshes (light shafts, glow spheres, smoke
    cards, pin lights) with their game materials compiled to a shader, the particle
    emitters the level starts with running, and the lens flares;
  * collision: the meshes (a few by their collision hulls, MAPS' "hulls"), the BSP
    and, for movement only, the blocking volumes
    the players-only ones (HmPlayerOnlyBlockingVolume: the enemies' spawn walls,
    which their AI walks through, a lid over the map where MAPS gives one:
    Ignition's, which the game lacks, and the boxes MAPS' "blockers" put where the
    game lets a player go that he shouldn't: Downfall's over the start room's roof)
    apart from the others;
  * the player start, enemy spawn points, AI path nodes, the purchase points (health,
    ammo, heavy ammo, weapons and equipment, with their prices and respawn times),
    and the level camera (the view of the map before the match begins);
  * the baked light maps (both packages'), the lights, the fog and the colour grade.

Inputs
  * a umodel export of <art> and <base> (whole packages, glTF + PNG), of the
    animation sets the level poses its props with that the packages don't hold in
    FoC's own layout (ue3/anim.py reads those), and of the few textures they take
    from other packages (see the README). umodel finds the
    full-size mips in the packages the textures come from, so those must be in the
    CookedPC folder it reads (the game's own is fine).
  * the game's CookedPC folder (the map's packages, their _LM light-map files, and
    the packages of the flip-book textures umodel doesn't export, ue3/texture.py).

Set FOC_KTX_CACHE to a folder to keep the compressed textures between builds.

Outputs (under <foc_esc/assets>; <map> is the key in MAPS)
  * models/<map>.glb and models/<map>/*.ktx2 - the map; its textures are files of
    their own next to it (glTF image URIs), at the game's full size, block-compressed
    with their mip maps as the game's own are (ktx2_io.py; Bevy reads KTX2 as it is)
  * levels/<map>_col.bin, levels/<map>_points.txt, levels/<map>_lightmaps/,
    levels/<map>_lights.txt, levels/<map>_grade.bin
  * levels/<map>_fx.txt, textures/levelfx/, shaders/levelfx/<map>.wgsl and
    shaders/levelfx.wgsl - the effects (levelfx.py)

usage: python build_map.py <map> <umodel export folder> <TransGame\\CookedPC> <foc_esc\\assets>
       python build_map.py --points <map> <TransGame\\CookedPC> <foc_esc\\assets>
         (levels/<map>_points.txt alone, from the BASE package)
       python build_map.py --volumes <map> <TransGame\\CookedPC> <foc_esc\\assets>
         (levels/<map>_col.bin's blocking volumes alone rewritten, its solid
         triangles kept: the players-only ones apart)
       python build_map.py --hulls <map> <TransGame\\CookedPC> <foc_esc\\assets>
         (levels/<map>_col.bin's triangles of MAPS' "hulls" meshes alone rewritten:
         their own shape's out, their collision hulls' in)
"""
import hashlib, math, os, struct, sys
import numpy as np
from PIL import Image

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.path.join(HERE, "ue3"))
sys.path.insert(0, HERE)
import build_level as bl  # noqa: E402
import ktx2_io  # noqa: E402
from gltf_io import Doc, Writer, node_globals  # noqa: E402
from psa_io import Psa  # noqa: E402
import layout as lay  # noqa: E402
import lighting  # noqa: E402
import bsp  # noqa: E402
import kismet  # noqa: E402
import texture  # noqa: E402
import terrain  # noqa: E402
import anim  # noqa: E402
import matparams  # noqa: E402
import levelfx  # noqa: E402
from ue3pkg import Package, Reader  # noqa: E402
from props import read_props  # noqa: E402

MAPS = {
    "downfall": {
        "title": "Downfall",
        "art": "ESC_Iacon_ART_m",
        "base": "ESC_Iacon_BASE_m",
        # Every area open, as the level stands once each console has been bought.
        "open": True,
        # The players' start room (the hall round the start, its ceiling 29.4 m up)
        # is open to the sky at its west end, through a light well, and a jet could
        # fly up it onto the hall's roof (30.7 m), where the enemies spawn (four
        # TnSurvivalSpawnPoints, 32.5 m up) and walk off its east end, and stay
        # there: nothing in the game stops it below the map's players-only lid
        # (HmPlayerOnlyBlockingVolume_2848, 38.4 m up). A players-only box from 30 m
        # (under the roof, over the hall's ceiling) up into the lid, over the light
        # well, the roof and its grates, keeps the players off it; the AI's pawns pass
        # it, as they do the game's spawn walls. Bevy metres, its least and greatest
        # corners (x, y, z).
        "blockers": [((54.5, 30.0, 4.0), (128.0, 38.6, 60.0))],
        # Meshes that collide by their collision hulls, as all of the game's do: a
        # StaticMesh's UseSimpleBoxCollision and UseSimpleLineCollision (true unless
        # it says otherwise: Default__StaticMesh) have the game's movement and traces
        # meet its BodySetup's hulls (AggGeom), where the demo's meet the mesh's own
        # shape (build_level.collision_mesh). Mostly the two are near enough; the
        # steel trusses off the start hall's roof are not: beam1_STAT is an open
        # lattice, 27 x 7.6 x 7.6 m, and its hull a box round it. The game's
        # navigation runs along their tops from the roof's spawn points to jumps
        # down off their sides, and the enemies walked the lattice, dropped into it
        # and landed on its side bracing, and stayed there. (The ones the purchases
        # bring down, TnStaticDestructibleActors, keep their shape: build_doors.py
        # makes their collision groups of it.)
        "hulls": ("beam1_STAT",),
    },
    "oblivion": {
        "title": "Oblivion",
        "art": "ESC_Ark_ART_m",
        "base": "ESC_Ark_BASE_m",
        # ForcefieldPlane_STAT in the BASE package are Escalation's gates
        # (StaticInterpActors that drop when the team pays at a console), left out:
        # its areas are all open.
        "skip_base": ("ForcefieldPlane_STAT",),
        # Those gates' material, which build_doors.py draws them in (as Ancients' own
        # force fields are drawn: FX_MATERIALS), from Ancients' package (Oblivion's
        # build has none of its own): compiled into the map's effects.
        "fx_donor": ("ESC_Temple_ART_m.xxx", ("ForceField_ESC_Matinst",)),
    },
    "ancients": {
        "title": "Ancients",
        "art": "ESC_Temple_ART_m",
        "base": "ESC_Temple_BASE_m",
        # Every area open, as the level stands once each console has been bought.
        "open": True,
    },
    "ignition": {
        "title": "Ignition",
        "art": "ESC_Gladiator_ART_m",
        "base": "ESC_Gladiator_BASE_m",
        # Every area open, as the level stands once each console has been bought.
        "open": True,
        # The arena is a pit in one great block of BSP, open to the sky, and the
        # game puts nothing over it (Downfall and Ancients have a blocking volume
        # over theirs): a jet could climb out of it and fly over the block. A lid at
        # the block's top (4096 units up) keeps the jets in the pit.
        "lid": 40.96,
        # The block's top (and its outer sides and underside) is BSP_Base, the
        # editor's grid (ENV_MP_ProtoTypes_p's, TEST_Grid_Materials_p.Grid_Grey):
        # those faces get the plating the pit's walls have up to the top, Panel_02.
        "aliases": {"BSP_Base": "Panel_02_TEXSET"},
    },
}

# Effect meshes: soft additive or translucent shapes the game draws with effect
# materials (light beams and shafts, fog and smoke cards, glow spheres, pin lights).
# Drawn with their game materials by src/levelfx.rs (levelfx.py).
FX_MESHES = ("LightBeam_Cone", "FogPlane", "GlowSphere", "PinLight", "SquareLightPlane")
# Materials drawn the same way wherever they are: Oblivion's windows and what is seen
# through them, each its game material's graph (tools/ue3/fxmat.py), moving as the
# game's do. The windows: ENV_Refractive_Glass_Trans_Mat's instances (blue glass,
# its pattern DecWalls_Mask's green, glinting, a bright rim where it is seen edge-on
# and an orange line sweeping across it) and the energon holding tower's
# TowerWindow_MATINST (ENV_Refractive_Glass_Opaque_Mat: the room behind seen
# through it, bent by its normal map, tinted blue). Through them (FX_BACKDROP, drawn
# outside the fog): the space dome (ENV_SpaceDome_MAT: three star fields, turning
# and drifting, let through by a cloud mask drifting across them, and the nebula),
# the space bridge and the vortex (its clouds and its black eye turning, the eye
# zooming in); Cybertron, lit by its light map, is a map material (_planet). And their energy fields (ESC_Shield_tiled2: waves running
# through their red mesh) and the two animated energon signs (fbook_sinewave2_CLR_Mat
# and fbook_sinewave6_CLR_Mat, flip books of a sine wave, the second under
# scan lines). Downfall's electric gates (ElectroPlane_MAT on its cards: lightning
# panning across them, flaring at random, each card its own). The Escalation force
# fields over the spawn rooms' doors and windows (ENV_ForceField_ALL_MAT's
# instances: Ignition's green ForceField_ESC_GREEN_Matinst, Ancients' and Oblivion's
# orange ForceField_ESC_Matinst): see-through, their circuit pattern drifting down
# under bright bands sweeping across it, glowing where they meet the walls round
# them (their depth-biased alpha) and more as they are seen edge-on.
FX_MATERIALS = ("ENV__ARK__Glass3_MATINST", "ENV__ARK__Glass2_MATINST", "TowerWindow_MATINST",
                "ENV_SpaceDome_MAT", "spacehole_CLR_Mat", "Vortex_MAT_INST",
                "ENV_RST_VortexClouds2_MAT", "ENV_RST_VortexBlackSwril2_MAT", "ESC_Shield_tiled2",
                "fbook_sinewave2_CLR_Mat", "fbook_sinewave6_CLR_Mat", "ElectroPlane_MAT",
                "ForceField_ESC_GREEN_Matinst", "ForceField_ESC_Matinst")

# How bright a purchase point base's lights are, per unit of its EmissiveIntensity
# (10 for every base): the game's colours at the brightness its captures show them
# (MapMats.get's pickup branch).
PICKUP_GLOW_SCALE = 0.5

# Effect meshes placed further than this from the play area's middle (m) are part of
# the sky (Ignition's moons' glow spheres, GlowSphere_STAT round each moon): seen
# through the level's height fog as the sky is, which is worked out for each of
# their vertices from the play area (ray_fog, into their vertex colours: levelfx.rs's
# FX_SKYFOG), as the game works out its fog for a translucent material at each
# vertex, rather than through the camera's fog over the play area.
SKY_FX_DISTANCE = 300.0

FX_BACKDROP = ("ENV_SpaceDome_MAT", "spacehole_CLR_Mat", "Vortex_MAT_INST",
               "ENV_RST_VortexClouds2_MAT", "ENV_RST_VortexBlackSwril2_MAT")
# (The windows' glass still stands in the way, as the rest of the map does.)
FX_GLASS = ("ENV__ARK__Glass3_MATINST", "ENV__ARK__Glass2_MATINST", "TowerWindow_MATINST")
# No collision: thin or decorative meshes, effect planes, the far backdrop (Downfall's
# buildings, bldg_0n; Ancients' bldg_foundation pillars and beams stand in the play area).
SKIP_COLLIDE = ("Card_STAT", "ElectroPlane_STAT", "ForcefieldPlane_STAT", "SpaceDome_STAT", "fin_ctronplanet",
                "fin_spacebridge", "Vortex", "esign_", "energonSign", "EnergonSign", "sign_arrow",
                "CookieDisc", "HangingLights", "TrackLight", "Cable", "cable", "RST_Skydome", "bldg_0",
                "vt_wires")
# The skeletal props the level places (as an actor class or the start of one).
SKELETAL_CLASSES = ("TnTransformingWorldActor", "SkeletalMeshActor", "HmSkeletalMeshActor", "TnInteract")
# Actors left out: Escalation's weapon upgrade console (AP_EscalationConsoleWeapon_SKEL),
# as the demo's weapons take their upgrades from the Customize build, the way
# multiplayer's do (src/upgrades.rs). (tools/remove_console.py takes it out of maps
# built before.)
SKIPPED_ACTORS = ("TnInteractSurvivalWeaponUpgradeConsole",)
# The properties that hold an actor's skeletal mesh component.
SKELETAL_COMPONENTS = ("SkeletalMeshComponent", "BaseMesh", "Mesh")
# Actor classes that may carry meshes and so may ride on (be hard-attached to) a
# moving actor.
MESH_ACTORS = ("StaticMeshActor", "StaticInterpActor", "InterpActor", "HmStaticMeshActor", "KActor",
               "TnStaticDestructibleActor") + SKELETAL_CLASSES


# ---------------------------------------------------------------------------
# Reading actors
# ---------------------------------------------------------------------------

def _clean(k):
    return all(c.isalnum() or c in "_[]" for c in k)


def actor_props(pkg, e, maxoff=120):
    """An object's tagged properties, wherever they start (actors have a state frame of
    varying size before them): the longest clean parse."""
    best = None
    for st in range(0, maxoff):
        try:
            r = Reader(pkg.data, e["offset"] + st)
            pr = read_props(pkg, r, e["offset"] + e["size"])
        except Exception:
            continue
        if pr and all(_clean(k) for k in pr):
            if best is None or len(pr) > len(best):
                best = pr
    return best or {}


def chain_props(pkg, idx, depth=6):
    """An object's properties merged with its archetypes' (its own win): the class
    defaults a map cooks in say what a console's or crate's mesh is."""
    out = {}
    while idx and idx > 0 and depth:
        e = pkg.exports[idx - 1]
        for k, v in actor_props(pkg, e, 60).items():
            out.setdefault(k, v)
        idx = e["archetype"]
        depth -= 1
    return out


def vec3(pr, key, default=(0.0, 0.0, 0.0)):
    return struct.unpack("<3f", pr[key][2][:12]) if key in pr else default


def rot3(pr, key="Rotation"):
    return struct.unpack("<3i", pr[key][2][:12]) if key in pr else (0, 0, 0)


def actor_matrix(pr):
    """An actor's PrePivot, DrawScale(3D), Rotation and Location as a row-vector
    matrix (UE units), as the engine's AActor::LocalToWorld makes it: its PrePivot
    taken off first, then scaled, turned and moved. (Only brushes and volumes set a
    PrePivot on these maps: a volume whose pivot was moved in the editor, such as
    Ancients' Loft blocker, HmPlayerOnlyBlockingVolume_9719, which stood 56 m from
    its place without it.)"""
    ds = pr.get("DrawScale", 1.0)
    ds3 = vec3(pr, "DrawScale3D", (1.0, 1.0, 1.0))
    m = np.diag([ds * ds3[0], ds * ds3[1], ds * ds3[2], 1.0]) @ lay.rotator_matrix(*rot3(pr))
    m[3, :3] = np.asarray(vec3(pr, "Location")) - np.asarray(vec3(pr, "PrePivot")) @ m[:3, :3]
    return m


def obj_name(pkg, v):
    return pkg.full_name(v[1]) if isinstance(v, tuple) and v[0] == "obj" and v[1] else None


def obj_index(v):
    """The export an object property points at (None for an import or nothing)."""
    return v[1] if isinstance(v, tuple) and v[0] == "obj" and v[1] and v[1] > 0 else None


def objs(v):
    """The objects of an array-of-objects property."""
    if not (isinstance(v, tuple) and v[0] == "array") or len(v[1]) < 4:
        return []
    n = struct.unpack_from("<i", v[1])[0]
    return list(struct.unpack_from(f"<{n}i", v[1], 4))


def level_actors(pkg):
    """Export indices of the actors the level places (its PersistentLevel's)."""
    lvl = {i for i, e in enumerate(pkg.exports, 1) if pkg.class_name(e) == "Level"}
    return [i for i, e in enumerate(pkg.exports, 1) if e["outer"] in lvl]


def actor_deltas(pkg, state):
    """{actor: 4x4 D} for the actors the level's Matinees move (kismet.opened_state),
    and those hard-attached to them: an actor's new world matrix is its old one @ D."""
    deltas = {}
    for idx, move in state.items():
        if isinstance(idx, tuple):
            continue
        pr = actor_props(pkg, pkg.exports[idx - 1])
        old = actor_matrix(pr)
        R = lay.rotator_matrix(*rot3(pr))[:3, :3]
        loc = np.array(vec3(pr, "Location"))
        ds = pr.get("DrawScale", 1.0)
        ds3 = vec3(pr, "DrawScale3D", (1.0, 1.0, 1.0))
        S = np.diag([ds * ds3[0], ds * ds3[1], ds * ds3[2]])
        Rk = kismet.euler_matrix(move.euler)
        new = np.eye(4)
        if move.relative:  # IMF_RelativeToInitial: in the actor's starting frame
            new[:3, :3] = S @ Rk @ R
            new[3, :3] = loc + move.offset @ R
        else:
            new[:3, :3] = S @ Rk
            new[3, :3] = move.offset
        deltas[idx] = np.linalg.inv(old) @ new
    # Actors riding on a moved one (Base, bHardAttach) go with it.
    for idx in level_actors(pkg):
        if idx in deltas or not pkg.class_name(pkg.exports[idx - 1]).startswith(MESH_ACTORS):
            continue
        pr = actor_props(pkg, pkg.exports[idx - 1])
        parent = obj_index(pr.get("Base"))
        if parent in deltas and pr.get("bHardAttach"):
            deltas[idx] = deltas[parent]
    return deltas


def furniture(pkg, pkg_file, hidden=()):
    """The BASE package's pickup bases: each purchase point (TnPickupFactoryPurchasable)
    but the health stations' stands on its OptionalBaseMesh, placed by the component's
    own transform within the factory's."""
    out = []
    for i, e in enumerate(pkg.exports, 1):
        if pkg.class_name(e) != "TnPickupFactoryPurchasable" or i in hidden:
            continue
        pr = actor_props(pkg, e)
        if pr.get("PresetType") == "PPT_Health" or not pr.get("UseOptionalBase"):
            continue
        comp = pr.get("OptionalBaseMesh")
        if not comp or comp[1] <= 0:
            continue
        cpr = actor_props(pkg, pkg.exports[comp[1] - 1])
        mesh = obj_name(pkg, cpr.get("StaticMesh"))
        if not mesh:
            continue
        m = lay.component_matrix(cpr) @ actor_matrix(pr)
        out.append({"mesh": mesh, "matrix": m.ravel().tolist(), "source": e["name"], "materials": [],
                    "component": comp[1], "package": pkg_file})
    return out


def doors(pkg, pkg_file):
    """The level's transforming doors (TnTransformingWorldActor) of the art package:
    their skeletal mesh, animation set and sequence, and the end of the sequence the
    level starts them at (SetInitialTransformState: untransformed at its start,
    transformed at its end). (Maps not marked "open"; the others use skeletal_props.)"""
    out = []
    for i, e in enumerate(pkg.exports, 1):
        if pkg.class_name(e) != "TnTransformingWorldActor":
            continue
        pr = actor_props(pkg, e)
        comp = pr.get("SkeletalMeshComponent")
        if not comp or comp[1] <= 0:
            continue
        cpr = actor_props(pkg, pkg.exports[comp[1] - 1])
        mesh = obj_name(pkg, cpr.get("SkeletalMesh"))
        anim = cpr.get("Animations")
        seq = actor_props(pkg, pkg.exports[anim[1] - 1]).get("AnimSeqName") if anim and anim[1] > 0 else None
        sets = [pkg.full_name(x) for x in objs(cpr.get("AnimSets"))]
        if not mesh or not seq or not sets:
            continue
        end = "end" if pr.get("InitialTransformState") == "ETWAS_Transformed" else "start"
        out.append({"skel": bl.last(mesh), "animset": bl.last(sets[0]), "pose": (end, seq),
                    "matrix": actor_matrix(pr).ravel().tolist(), "source": e["name"], "package": pkg_file,
                    "component": comp[1]})
    return out


def skeletal_props(pkg, pkg_file, state, hidden, deltas, transformed=None):
    """The skeletal meshes the level places (SKELETAL_CLASSES: transforming doors and
    covers, consoles, loot crates, animated doors), with what poses them: the
    animation a Matinee leaves them in (kismet), else the sequence their animation
    node holds (at its start, or its end for a transforming actor that is
    transformed: as the level's Kismet leaves it, `transformed`, else as it starts),
    else their bind pose. Their mesh, animation sets and sequence come from the
    component or the class defaults it was made from (its archetypes)."""
    transformed = transformed or {}
    out = []
    for idx in level_actors(pkg):
        e = pkg.exports[idx - 1]
        cls = pkg.class_name(e)
        if idx in hidden or not cls.startswith(SKELETAL_CLASSES) or cls in SKIPPED_ACTORS:
            continue
        pr = actor_props(pkg, e)
        comp = None
        for key in SKELETAL_COMPONENTS:
            c = obj_index(pr.get(key))
            if c and "Skeletal" in pkg.class_name(pkg.exports[c - 1]):
                comp = c
                break
        if comp is None:
            continue
        cpr = chain_props(pkg, comp)
        mesh = obj_name(pkg, cpr.get("SkeletalMesh"))
        if not mesh:
            continue
        sets = [pkg.full_name(x) for x in objs(cpr.get("AnimSets")) if x]
        seq = None
        node = obj_index(cpr.get("Animations"))
        if node:
            seq = chain_props(pkg, node).get("AnimSeqName")
        anim = state.get(("anim", idx))
        if anim is not None and not sets:
            # (Its Matinee group brings the animation sets: Ignition's steam vents.)
            sets = state.get(("animsets", idx), [])
        if anim is not None:
            pose = ("anim", anim)
        elif seq and sets:
            end = cls == "TnTransformingWorldActor" and transformed.get(
                idx, pr.get("InitialTransformState") == "ETWAS_Transformed")
            pose = ("end" if end else "start", str(seq))
        else:
            pose = None  # the bind pose
        m = lay.component_matrix(cpr) @ actor_matrix(pr)
        if idx in deltas:
            m = m @ deltas[idx]
        out.append({"skel": bl.last(mesh), "animset": bl.last(sets[0]) if sets else None,
                    "animsets": [bl.last(s) for s in sets], "pose": pose,
                    "matrix": m.ravel().tolist(), "source": e["name"], "package": pkg_file, "component": comp,
                    "class": cls})
    return out


def _affine(pkg, v):
    """An AffineTransform struct (Position, Orientation, Scale) as a row-vector matrix."""
    m = np.eye(4)
    if not (isinstance(v, tuple) and v[0] == "struct"):
        return m
    sub = read_props(pkg, Reader(v[2]), len(v[2]))
    if "Scale" in sub:
        s = sub["Scale"]
        m = np.diag([*(struct.unpack("<3f", s[2][:12]) if isinstance(s, tuple) else (s, s, s)), 1.0]) @ m
    if "Orientation" in sub:
        m = m @ lay.rotator_matrix(*struct.unpack("<3i", sub["Orientation"][2][:12]))
    if "Position" in sub:
        m[3, :3] = struct.unpack("<3f", sub["Position"][2][:12])
    return m


def destructibles(pkg, pkg_file, hidden):
    """The level's destructible props (TnStaticDestructibleActor) in their intact
    state: each piece's first state's meshes (HmStaticMeshDestructionEffectFactory),
    placed by the factory's transform and component template within the actor."""
    out = []
    for idx in level_actors(pkg):
        e = pkg.exports[idx - 1]
        if pkg.class_name(e) != "TnStaticDestructibleActor" or idx in hidden:
            continue
        pr = actor_props(pkg, e)
        comp = obj_index(pr.get("DestructibleComponent"))
        blueprint = obj_index(chain_props(pkg, comp).get("Blueprint")) if comp else None
        if not blueprint:
            continue
        A = actor_matrix(pr)
        for piece in objs(actor_props(pkg, pkg.exports[blueprint - 1], 60).get("PieceBlueprints")):
            states = objs(actor_props(pkg, pkg.exports[piece - 1], 60).get("StateBlueprints"))
            if not states:
                continue
            for fac in objs(actor_props(pkg, pkg.exports[states[0] - 1], 60).get("EffectFactories")):
                if fac <= 0 or pkg.class_name(pkg.exports[fac - 1]) != "HmStaticMeshDestructionEffectFactory":
                    continue
                fpr = actor_props(pkg, pkg.exports[fac - 1], 60)
                mesh = obj_name(pkg, fpr.get("Mesh"))
                if not mesh:
                    continue
                tpl = obj_index(fpr.get("ComponentTemplate"))
                tpr = chain_props(pkg, tpl) if tpl else {}
                mats = [pkg.full_name(x) if x else None for x in objs(tpr.get("Materials"))]
                m = lay.component_matrix(tpr) @ _affine(pkg, fpr.get("Transform")) @ A
                out.append({"mesh": mesh, "matrix": m.ravel().tolist(), "source": e["name"], "materials": mats,
                            "package": pkg_file, "destructible": True})
    return out


def mesh_hull(pkgs, name):
    """A StaticMesh's collision hulls (its BodySetup's AggGeom's ConvexElems: each
    one's corners, Unreal units in the mesh's own space, and its triangles, corner
    indices), from the first of `pkgs` that has the mesh; None if it has none."""
    for pkg in pkgs:
        for e in pkg.exports:
            if e["name"] != name or pkg.class_name(e) != "StaticMesh":
                continue
            body = obj_index(actor_props(pkg, e, 60).get("BodySetup"))
            if not body:
                return None
            agg = actor_props(pkg, pkg.exports[body - 1], 60).get("AggGeom")
            if not (isinstance(agg, tuple) and agg[0] == "struct"):
                return None
            elems = read_props(pkg, Reader(agg[2]), len(agg[2]))
            others = [k for k in elems if k != "ConvexElems"]
            if others:
                print(f"  ! {name}'s hulls: {others} not read")
            v = elems.get("ConvexElems")
            if not (isinstance(v, tuple) and v[0] == "array" and len(v[1]) >= 4):
                return None
            r = Reader(v[1])
            out = []
            for _ in range(r.i32()):
                el = read_props(pkg, r, len(v[1]))
                vd, fd = el.get("VertexData"), el.get("FaceTriData")
                if not (isinstance(vd, tuple) and isinstance(fd, tuple)):
                    continue
                n = struct.unpack_from("<i", vd[1])[0]
                corners = np.array(struct.unpack_from(f"<{3 * n}f", vd[1], 4), np.float64).reshape(n, 3)
                m = struct.unpack_from("<i", fd[1])[0]
                faces = np.array(struct.unpack_from(f"<{m}i", fd[1], 4), np.int64).reshape(-1, 3)
                out.append((corners, faces))
            return out or None
    return None


def place_hull(hull, A, t):
    """A mesh's hulls (mesh_hull) placed by a placement's matrix (A, t: as the
    mesh's export is placed, umodel's metres with Y up): (n, 9) float32, each
    triangle wound to face out of its hull (collision.rs)."""
    tris = []
    for corners, faces in hull:
        v = (corners @ bl.P.T * 0.01) @ A.T + t
        mid = v.mean(0)
        tri = v[faces]
        n = np.cross(tri[:, 1] - tri[:, 0], tri[:, 2] - tri[:, 0])
        inward = (n * (tri.mean(1) - mid)).sum(1) < 0
        tri[inward] = tri[inward][:, [0, 2, 1]]
        tris.append(tri.reshape(-1, 9))
    return np.concatenate(tris).astype(np.float32)


def near_play(tris, pts):
    """The collision triangles within 25 m of the AI's paths and spawn points (what
    lies beyond is left out)."""
    conv = lambda v: [round(v[0] * 0.01, 2), round(v[2] * 0.01, 2), round(v[1] * 0.01, 2)]  # noqa: E731
    nav = np.array([conv(v) for v in pts["TnTacticalPathNode"] + pts["TnSurvivalSpawnPoint"]])
    lo, hi = nav.min(0) - 25, nav.max(0) + 25
    lo[1], hi[1] = -60, nav[:, 1].max() + 40
    tmin, tmax = tris.reshape(-1, 3, 3).min(1), tris.reshape(-1, 3, 3).max(1)
    return tris[((tmax >= lo) & (tmin <= hi)).all(1)]


def clip_reader(it, anim_dirs, packages):
    """What reads the clips that pose a skeletal placement: sequence -> ({bone:
    (positions, rotations)}, frame time), or (None, None). From umodel's export of its
    animation set (.psa), else straight from its package (ue3/anim.py: the FoC clips
    umodel skips, like the Optimus doors'); the first of its animation sets that has
    the sequence."""
    sets = it.get("animsets") or ([it["animset"]] if it.get("animset") else [])
    pkg = packages.get(it.get("package"))

    def clip(seq):
        for animset in sets:
            psa = next((os.path.join(d, animset + ".psa") for d in anim_dirs
                        if os.path.exists(os.path.join(d, animset + ".psa"))), None)
            tracks = dt = None
            if psa:
                tracks, dt = Psa(psa).clip(seq)
            if tracks is None and pkg is not None:
                try:
                    tracks, dt = anim.clip(pkg, animset, seq)
                except (KeyError, ValueError):
                    pass
            if tracks is not None:
                return tracks, dt
        return None, None
    return clip


class Posed:
    """A skeletal mesh posed at one moment of a clip (or in its bind pose), as a static
    mesh with the same interface as a gltf_io.Doc of umodel's static mesh export (one
    mesh, its primitives' attributes, indices and materials). `pose` is None (bind
    pose), ("start" | "end", sequence) or ("anim", kismet.Anim); `clip` reads a
    sequence's keys (clip_reader). `posed` says whether the clip was found."""

    def __init__(self, gltf, pose=None, clip=None):
        d = Doc(gltf)
        self.posed = False
        if clip and pose:
            seq = pose[1].sequence if pose[0] == "anim" else pose[1]
            tracks, dt = clip(seq)
            self.posed = bool(tracks)
            names = {n.get("name"): i for i, n in enumerate(d.g["nodes"])}
            for bone, (pos, rot) in (tracks or {}).items():
                i = names.get(bone)
                if i is None:
                    continue
                n_frames = len(pos)
                if pose[0] == "end":
                    f = n_frames - 1
                elif pose[0] == "anim":
                    t = kismet.anim_time(pose[1], (n_frames - 1) * dt)
                    f = int(round(t / dt)) if dt > 0 else 0
                else:
                    f = 0
                f = min(max(f, 0), n_frames - 1)
                n = d.g["nodes"][i]
                n.pop("matrix", None)
                n["translation"] = [float(x) for x in pos[f]]
                n["rotation"] = [float(x) for x in rot[f]]
        G = node_globals(d.g)
        self.g = {"meshes": [{"primitives": []}], "materials": d.g["materials"], "accessors": []}
        self._arrays = []
        for ni, n in enumerate(d.g["nodes"]):
            if "mesh" not in n or "skin" not in n:
                continue
            skin = d.g["skins"][n["skin"]]
            ibm = d.acc(skin["inverseBindMatrices"]).reshape(-1, 4, 4).transpose(0, 2, 1)
            J = np.stack([G[j] @ ibm[k] for k, j in enumerate(skin["joints"])])
            for p in d.g["meshes"][n["mesh"]]["primitives"]:
                at = p["attributes"]
                pos = d.acc(at["POSITION"]).astype(np.float64)
                nrm = d.acc(at["NORMAL"]).astype(np.float64)
                jt = d.acc(at["JOINTS_0"]).astype(int)
                wt = d.acc(at["WEIGHTS_0"]).astype(np.float64)
                M = np.einsum("vk,vkij->vij", wt, J[jt])
                ph = np.concatenate([pos, np.ones((len(pos), 1))], 1)
                pos = np.einsum("vij,vj->vi", M, ph)[:, :3]
                nrm = np.einsum("vij,vj->vi", M[:, :3, :3], nrm)
                nrm /= np.maximum(np.linalg.norm(nrm, axis=1, keepdims=True), 1e-9)
                attrs = {"POSITION": self._add(pos.astype(np.float32)), "NORMAL": self._add(nrm.astype(np.float32)),
                         "TEXCOORD_0": self._add(d.acc(at["TEXCOORD_0"]).astype(np.float32))}
                if "TANGENT" in at:
                    tan = d.acc(at["TANGENT"]).astype(np.float64)
                    t3 = np.einsum("vij,vj->vi", M[:, :3, :3], tan[:, :3])
                    t3 /= np.maximum(np.linalg.norm(t3, axis=1, keepdims=True), 1e-9)
                    attrs["TANGENT"] = self._add(np.concatenate([t3, tan[:, 3:4]], 1).astype(np.float32))
                q = {"attributes": attrs, "indices": self._add(d.acc(p["indices"]).reshape(-1).astype(np.uint32))}
                if "material" in p:
                    q["material"] = p["material"]
                self.g["meshes"][0]["primitives"].append(q)

    def _add(self, arr):
        self._arrays.append(arr)
        self.g["accessors"].append({"count": len(arr)})
        return len(self._arrays) - 1

    def acc(self, i):
        a = self._arrays[i]
        return a.reshape(len(a), -1) if a.ndim == 1 else a


def pose_key(it):
    """The name a posed skeletal placement's mesh is cached and drawn under: the mesh,
    the sequence and where in it ("start", "end", or a Matinee's run: seconds played,
    "r" reversed, "l" looping, +/- start and end offsets; their decimal points as
    "_", since names are cut at dots)."""
    pose = it.get("pose")
    if pose is None:
        return f"{it['skel']}@bind"
    if pose[0] == "anim":
        a = pose[1]
        start, end, run, reverse, looping = a.time
        at = f"{run:g}s" + ("r" if reverse else "") + ("l" if looping else "")
        at += (f"+{start:g}" if start else "") + (f"-{end:g}" if end else "")
        return f"{it['skel']}@{a.sequence}@{at}".replace(".", "_")
    return f"{it['skel']}@{pose[1]}@{pose[0]}"


def file_texsets(pkg):
    """{material: texture-set file base}: the texture set each material instance uses
    (following Parent links), named as its exported Color_NormX texture is (texture
    sets and their textures don't always share a name)."""
    sets = lay.material_texsets(pkg)
    files = {}
    for i, e in enumerate(pkg.exports, 1):
        if pkg.class_name(e) == "Texture2D" and e["name"].endswith("_Color_NormX") and e["outer"] > 0:
            outer = pkg.exports[e["outer"] - 1]
            if pkg.class_name(outer) == "TextureSet":
                files[outer["name"]] = e["name"][: -len("_Color_NormX")]
    return {m: files.get(t, t) for m, t in sets.items()}


def section_materials(pkg):
    """{static mesh: [material of each section]}, as the package names them."""
    out = {}
    for i, e in enumerate(pkg.exports, 1):
        if pkg.class_name(e) != "StaticMesh":
            continue
        try:
            out[e["name"]] = [bl.last(pkg.full_name(m)) if m else None for m in lay.mesh_section_materials(pkg, i)]
        except Exception:
            pass
    return out


def kismet_objects(pkg):
    """Export indices of the objects the level's Kismet holds in its object variables
    (SeqVar_Object, SeqVar_ObjectList): the actors its actions work on."""
    out = set()
    for e in pkg.exports:
        if pkg.class_name(e) not in ("SeqVar_Object", "SeqVar_ObjectList"):
            continue
        pr = actor_props(pkg, e, 16)
        v = pr.get("ObjValue")
        if isinstance(v, tuple) and v[0] == "obj":
            out.add(v[1])
        lst = pr.get("ObjList")
        if isinstance(lst, tuple) and lst[0] == "array" and len(lst[1]) >= 4:
            n = struct.unpack_from("<i", lst[1], 0)[0]
            out.update(struct.unpack_from("<i", lst[1], 4 + 4 * k)[0] for k in range(n) if 8 + 4 * k <= len(lst[1]))
    return out


def volume_triangles(pkg, path, classes=("BlockingVolume", "HmPlayerOnlyBlockingVolume")):
    """The surfaces of the level's blocking volumes (Unreal units, world space), as
    triangles: their brushes' BSP polygons, placed by the volume actors. Volumes the
    level's Kismet switches are left out: they are the paid gates' player blockers
    (SeqAct_Toggle turns them off as a team opens the gate), and the demo's gates are
    all open."""
    switched = kismet_objects(pkg)
    tris = []
    for i, e in enumerate(pkg.exports, 1):
        if pkg.class_name(e) != "Model" or e["outer"] <= 0:
            continue
        owner = pkg.exports[e["outer"] - 1]
        if pkg.class_name(owner) not in classes or e["outer"] in switched:
            continue
        pr = actor_props(pkg, owner)
        M = actor_matrix(pr)
        _, vectors, points, nodes, surfs, verts = bsp.extract(path, pkg=pkg, index=i)
        for vpool, isurf, nv, plane in nodes:
            if nv < 3:
                continue
            idx = verts[vpool:vpool + nv]
            if idx.max() >= len(points):
                continue
            P = points[idx].astype(np.float64)
            P = np.concatenate([P, np.ones((nv, 1))], 1) @ M
            for k in range(1, nv - 1):
                tris.append(np.concatenate([P[0, :3], P[k, :3], P[k + 1, :3]]))
    return np.array(tris).reshape(-1, 9)


# ---------------------------------------------------------------------------
# Materials
# ---------------------------------------------------------------------------

def load_png(path, mode="RGBA"):
    return np.asarray(Image.open(path).convert(mode))


def normal_xy(norm_rgb):
    """A game normal map's x and y (R and G, 0-255) in glTF's convention: the game's
    y points the other way (as the texture sets' NormY, which Mats flips too)."""
    return np.stack([norm_rgb[..., 0], 255 - norm_rgb[..., 1]], -1)


# A level material whose reflection coefficient (CoefficientColor_C) passes this is
# coloured metal (Ignition's gold statue of Starscream, TR_Starscream_*_MAT: (2, 1,
# 0.11)), which the game's ENV_BRDF_A_MAT shading makes shine in its colour and
# reflect its surroundings: drawn with the characters' shading (gilded_flag).
GILDED = 1.2


def gilded_flag(values):
    """The "brdf:" flag (foc_extras) of a gilded material - matparams.brdf's values,
    when its CoefficientColor_C passes GILDED - or None: Roughness Power, Roughness
    Offset, SmoothStep_Min, SmoothStep_Max, DiffuseIntensityScalar,
    DiffuseIntensityAdd, CoefficientColor_C (r, g, b), MetallicOffset and the
    reflection's strength (NewReflectionIntensity + 2, as ENV_Reflection_BRDF_MF
    adds), for src/character.rs's paint shader (gild_level_parts)."""
    if not values or max(values["coefficient"]) <= GILDED:
        return None
    v = [values["Roughness Power"], values["Roughness Offset"], values["SmoothStep_Min"], values["SmoothStep_Max"],
         values["DiffuseIntensityScalar"], values["DiffuseIntensityAdd"], *values["coefficient"],
         values["MetallicOffset"], values["NewReflectionIntensity"] + 2.0]
    return "brdf:" + ":".join("%g" % round(x, 5) for x in v)


def foc_extras(*flags):
    """glTF material extras the demo reads (lighting.rs, map_material_flags): "add"
    (drawn additively), "nofog" (outside the fog: the far backdrop), "nolight" (no
    light maps: lit by nothing but itself), "noshadow" (casts no shadow: the skies),
    "flipbook:<columns>:<rows>:<frames a second>[:random]" (its texture is a flip
    book's frames, left to right and top to bottom, shown one at a time, in order or
    at random), "depthbias:<n>" (its depth pushed nearer, n > 0, or further, n < 0, by
    n of the depth buffer's smallest steps; and see-through ones drawn as if n metres
    nearer: the terrain's layers, terrain_meshes), "brdf:<values>" (shaded as the
    characters are: gilded_flag), "prelit" (its light map's light baked into its
    colour, its light map plain white light: _planet)."""
    return {"foc": " ".join(flags)}


def linear(rgb8):
    """sRGB bytes -> linear light, 0-1 (the game samples colour maps this way)."""
    return ktx2_io.srgb_to_linear(rgb8)


def srgb_bytes(lin):
    """Linear light (0-1) -> sRGB bytes."""
    return np.round(ktx2_io.linear_to_srgb(lin)).astype(np.uint8)


class MapMats(bl.Mats):
    """build_level's texture-set materials at the game's full texture size, plus the
    map's own materials (SPECIAL). Every texture is a KTX2 file, block-compressed with
    its mip maps as the game's are (ktx2_io.py)."""

    def __init__(self, w, texsets, aliases, packages=(), cooked=None):
        super().__init__(w, texsets)
        self.aliases = aliases
        self.packages, self.cooked = packages, cooked
        # Texture sets from every package umodel wrote, not just the art package's.
        for d in bl.EXTRA_TX:
            self.bases |= {f[: -len("_Color_NormX.png")] for f in os.listdir(d) if f.endswith("_Color_NormX.png")}
        # The material instances' own parameters (their glow), the art package's first,
        # over the shared packages' (their parents may be there: matparams.map_params).
        self.params = matparams.map_params(packages, cooked)

    def texset_for(self, mat):
        if mat in self.aliases:
            return self.aliases[mat]
        return super().texset_for(mat)

    def package_texture(self, name):
        """A texture umodel doesn't export (a flip book), read from the map's packages
        (ue3/texture.py): RGBA bytes."""
        return self.package_flipbook(name)[0]

    def package_flipbook(self, name):
        """A flip book (TextureFlipBook) from the map's packages: its RGBA bytes and the
        flag that animates it (foc_extras)."""
        for pkg in self.packages:
            for i, e in enumerate(pkg.exports, 1):
                if e["name"] == name and "Texture" in pkg.class_name(e):
                    pr, _ = lay.props_any(pkg, e, (4, 0, 8))
                    flag = "flipbook:%d:%d:%g" % (pr.get("HorizontalImages", 1), pr.get("VerticalImages", 1),
                                                  pr.get("FrameRate", 4.0))
                    if pr.get("FBMethod") == "TFBM_RANDOM":
                        flag += ":random"
                    return texture.read_texture(pkg, i, self.cooked)[0], flag
        raise FileNotFoundError(name)

    def get(self, mat):
        if mat in self.cache:
            return self.cache[mat]
        if mat in SPECIAL:
            self.cache[mat] = self.w.material(SPECIAL[mat](self))
            return self.cache[mat]
        base = self.texset_for(mat) if mat else None
        if not base or not bl.find_tex(base + "_Color_NormX") or not bl.find_tex(base + "_Masks_NormY"):
            return self.untextured(mat)
        w = self.w
        c = load_png(bl.find_tex(base + "_Color_NormX"))
        h, wd = c.shape[:2]
        m = np.asarray(Image.open(bl.find_tex(base + "_Masks_NormY")).convert("RGBA").resize((wd, h), Image.BILINEAR))
        color = c[..., :3].copy()
        # The normal's x and y (Color A and Masks A; y flipped, as build_level.py's),
        # for a two-channel (BC5) normal map.
        normal = np.stack([c[..., 3], 255 - m[..., 3]], -1)
        # The shine (Masks G) as build_level.py turns it into roughness, at the masks'
        # own size.
        ms = np.asarray(Image.open(bl.find_tex(base + "_Masks_NormY")).convert("RGBA"))
        spec = ms[..., 1].astype(np.float32) / 255
        mr = np.stack([np.zeros_like(spec), np.clip(0.9 - 0.65 * spec, 0.25, 0.95), np.clip(0.2 + 0.5 * spec, 0, 1)], -1)
        mr = (mr * 255).astype(np.uint8)
        glow = np.clip((m[..., 0].astype(np.float32) - 140) / 115, 0, 1)
        gm = {"name": mat,
              "pbrMetallicRoughness": {"baseColorTexture": self.tex(base + "_c", color),
                                       "metallicRoughnessTexture": self.tex(base + "_mr", mr, "data"),
                                       "metallicFactor": 0.0, "roughnessFactor": 1.0},
              "normalTexture": self.tex(base + "_n", normal, "normal")}
        frac = (glow > 0.25).mean()
        own = matparams.glow(self.params, mat)
        pickup = matparams.pickup_glow(self.params, mat) if own is None else None
        if pickup is not None and 0.0005 < frac < 0.35:
            # A purchase point's base (ESC_PickupBase_*: an instance of the pickups'
            # Pickup_Unlit_MAT): its lights (the glow mask) in its Emissive_Color at
            # its EmissiveIntensity, by the kind of point it is: cyan for the weapons'
            # and ammo's, yellow the heavy weapons' and heavy ammo's, red the assault
            # equipment's and blue the utility equipment's.
            intensity, tint = pickup
            gm["emissiveTexture"] = self.tex(base + "_glow", np.repeat((glow * 255).astype(np.uint8)[..., None], 3, -1))
            gm["emissiveFactor"] = [round(c, 4) for c in tint]
            gm["extensions"] = {"KHR_materials_emissive_strength": {"emissiveStrength": intensity * PICKUP_GLOW_SCALE}}
            self.use_ext("KHR_materials_emissive_strength")
        elif own is not None:
            # An instance of ENV_BRDF_A_MAT: its glow as its material makes it (the
            # Masks red channel x EmissiveIntensity x Emissive_Color or the colour map),
            # none when its EmissiveIntensity is 0 or its Emissive_Color black (Downfall's
            # Street_Light_MoreSpec_MATINST: 1, black).
            intensity, tint = own
            mask = ms[..., 0].astype(np.float32) / 255
            if intensity > 0 and mask.max() > 0.02 and (tint is None or max(tint) > 0):
                h2, w2 = mask.shape
                src = linear(color) if tint is None else np.asarray(tint, np.float32)
                if tint is None and (h2, w2) != color.shape[:2]:
                    src = linear(np.asarray(Image.fromarray(color).resize((w2, h2), Image.BILINEAR)))
                tex, peak = self.hdr(mat + "_e", mask[..., None] * src * intensity)
                gm["emissiveTexture"] = tex
                gm["emissiveFactor"] = [1, 1, 1]
                gm["extensions"] = {"KHR_materials_emissive_strength": {"emissiveStrength": peak}}
                self.use_ext("KHR_materials_emissive_strength")
        elif 0.0005 < frac < 0.35:
            em = (color.astype(np.float32) * glow[..., None] * 1.2).clip(0, 255).astype(np.uint8)
            gm["emissiveTexture"] = self.tex(base + "_e", em)
            gm["emissiveFactor"] = [1, 1, 1]
            gm["extensions"] = {"KHR_materials_emissive_strength": {"emissiveStrength": 2.5}}
            self.use_ext("KHR_materials_emissive_strength")
        flag = gilded_flag(matparams.brdf(self.params, mat))
        if flag:
            gm["extras"] = foc_extras(flag)
        self.cache[mat] = w.material(gm)
        return self.cache[mat]

    def untextured(self, mat):
        """build_level's stand-ins: the BSP's builder grid, or flat grey."""
        if mat and "BSP_Base" in mat and bl.find_tex("Grid_Grey"):
            gm = {"name": mat, "pbrMetallicRoughness": {"baseColorTexture": self.tex("grid", T("Grid_Grey")[..., :3]),
                                                        "metallicFactor": 0.0, "roughnessFactor": 0.7}}
        else:
            self.missing.add(mat)
            gm = {"name": mat or "none", "pbrMetallicRoughness": {"baseColorFactor": [0.32, 0.31, 0.33, 1],
                                                                  "metallicFactor": 0.0, "roughnessFactor": 0.6}}
        self.cache[mat] = self.w.material(gm)
        return self.cache[mat]

    def tex(self, name, arr, kind="color", wrap=10497):
        """A texture of `arr` as KTX2 (ktx2_io.encode: kind "color", "data" or
        "normal"). With FOC_KTX_CACHE set to a folder, encoded textures are kept there
        and reused by later builds (compressing them is most of a build's time)."""
        cache = os.environ.get("FOC_KTX_CACHE")
        if cache:
            arr = np.ascontiguousarray(arr)
            key = hashlib.sha1(arr.tobytes() + repr((arr.shape, str(arr.dtype), kind)).encode()).hexdigest()
            path = os.path.join(cache, key + ".ktx2")
            if os.path.exists(path):
                data = open(path, "rb").read()
            else:
                data = ktx2_io.encode(arr, kind)
                os.makedirs(cache, exist_ok=True)
                open(path, "wb").write(data)
        else:
            data = ktx2_io.encode(arr, kind)
        return {"index": self.w.texture(name, data, wrap)}

    def use_ext(self, ext):
        used = self.w.g.setdefault("extensionsUsed", [])
        if ext not in used:
            used.append(ext)

    def hdr(self, name, lin, alpha=None):
        """A colour texture of linear light that may pass 1 (the game's units): the
        texture holds it over its peak, as sRGB, and the peak is returned for a
        factor or an emissive strength."""
        peak = max(float(np.max(lin)), 1e-6)
        rgb = srgb_bytes(lin / peak)
        if alpha is not None:
            rgb = np.concatenate([rgb, np.asarray(alpha, np.uint8)[..., None]], -1)
        return self.tex(name, rgb), peak


def unlit(m, name, rgb_tex, factor=(1, 1, 1), alpha="OPAQUE", flags=(), double=False, alpha_tex=None):
    """An unlit material showing `rgb_tex` (times `factor`, which may exceed 1: the
    texture then holds rgb / that)."""
    m.use_ext("KHR_materials_unlit")
    f = np.asarray(factor, np.float32)
    peak = max(float(f.max()), 1e-6)
    scale = max(peak, 1.0)
    rgb = np.clip(rgb_tex.astype(np.float32) * (f / scale), 0, 255).astype(np.uint8)
    if alpha_tex is not None:
        rgba = np.concatenate([rgb, alpha_tex[..., None]], -1)
        tex = m.tex(name, rgba)
    else:
        tex = m.tex(name, rgb)
    g = {"name": name, "pbrMetallicRoughness": {"baseColorTexture": tex, "baseColorFactor": [scale, scale, scale, 1.0],
                                                "metallicFactor": 0.0, "roughnessFactor": 1.0},
         "extensions": {"KHR_materials_unlit": {}}, "alphaMode": alpha, "extras": foc_extras(*flags)}
    if double:
        g["doubleSided"] = True
    return g


def unlit_hdr(m, name, lin, alpha="OPAQUE", flags=(), double=False, alpha_tex=None, factor=(1.0, 1.0, 1.0)):
    """An unlit material showing linear light `lin` x `factor` (the game's units; may
    pass 1). A strong colour belongs in `factor`: the texture keeps `lin`'s own range,
    and so its detail."""
    m.use_ext("KHR_materials_unlit")
    tex, peak = m.hdr(name, lin, alpha_tex)
    r, g_, b = (peak * float(f) for f in factor)
    g = {"name": name, "pbrMetallicRoughness": {"baseColorTexture": tex, "baseColorFactor": [r, g_, b, 1.0],
                                                "metallicFactor": 0.0, "roughnessFactor": 1.0},
         "extensions": {"KHR_materials_unlit": {}}, "alphaMode": alpha, "extras": foc_extras(*flags)}
    if double:
        g["doubleSided"] = True
    return g


def T(name):
    """An exported texture, by name."""
    p = bl.find_tex(name)
    if not p:
        raise FileNotFoundError(name)
    return load_png(p)


def _black(m):
    # Black_Mat: unlit black cards that hide the far side of openings (the game fades
    # them in from 7 m away by depth; here they are simply black).
    return {"name": "Black_Mat", "pbrMetallicRoughness": {"baseColorFactor": [0, 0, 0, 1], "metallicFactor": 0.0,
                                                          "roughnessFactor": 1.0},
            "extensions": {"KHR_materials_unlit": {}}, "extras": foc_extras("nolight")}


def _forcefield(m):
    # ESC_Shield_tiled2 (ENV_ForceField_MAT): additive red energy - its wave colour
    # (0.85, 0.03, 0.08) on the ChainLinkFence_02_MASK pattern, tiled 4 x 4.
    mask = T("ChainLinkFence_02_MASK")[..., 0]
    h, wd = mask.shape
    small = np.asarray(Image.fromarray(mask).resize((wd // 4, h // 4), Image.LANCZOS)).astype(np.float32)
    tiled = np.tile(small, (4, 4))
    rgb = np.stack([tiled * 0.85, tiled * 0.03, tiled * 0.08], -1) * 0.9 + np.array([60, 6, 4]) * 0.35
    return unlit(m, "ESC_Shield_tiled2", rgb.clip(0, 255), (1.0, 1.0, 1.0), "BLEND", ("add", "nolight"), True)


def _spacehole(m):
    return unlit(m, "spacehole_CLR_Mat", T("spacehole_CLR")[..., :3], (1, 1, 1), "BLEND", ("add", "nofog", "nolight"), True)


def _energon_signs(tex_name, strength):
    def f(m):
        return unlit(m, tex_name + "_sign", T(tex_name)[..., :3], (strength,) * 3, "BLEND", ("add", "nolight"), True)
    return f


def _vortex_clouds(m):
    c = T("ENV_RST_VortexSwirlClouds_CLR")
    return unlit(m, "ENV_RST_VortexClouds2_MAT", c[..., :3], (1, 1, 1), "BLEND", ("nofog", "nolight"), True, c[..., 3])


def _vortex_eye(m):
    c = T("ENV_RST_VortexSwirl_CLR")
    return unlit(m, "ENV_RST_VortexBlackSwril2_MAT", c[..., :3], (1, 1, 1), "BLEND", ("nofog", "nolight"), True, c[..., 3])


def _vortex(m):
    # Vortex_MAT_INST: additive, its colour (0.21, 0.58, 0.48) x 4 times the mesh's
    # vertex colours (kept by the mesh; here a flat glow).
    rgb = np.full((4, 4, 3), 255, np.uint8)
    return unlit(m, "Vortex_MAT_INST", rgb, (0.212 * 2.4, 0.582 * 2.4, 0.477 * 2.4), "BLEND", ("add", "nofog", "nolight"), True)


def _sized(a, h, wd):
    """A texture's bytes at h x wd (bilinear), if they aren't already."""
    if a.shape[:2] == (h, wd):
        return a
    return np.asarray(Image.fromarray(a).resize((wd, h), Image.BILINEAR))


# The three directions a light map's light is kept for, in a surface's tangent space
# (x along its tangent, z its normal): the order of DirectionalMaxComponent's R, G
# and B, as the game's light-mapped shaders have them.
LM_BASIS = np.array([[0.0, 0.816497, 0.57735], [-0.707107, -0.408248, 0.57735], [0.707107, -0.408248, 0.57735]],
                    np.float32)


def lightmap_texels(L, lm, h, wd):
    """A 2D light map in the game's terms at every texel of an h x wd texture laid
    on the UVs it is laid on (bilinear, as the GPU samples it): its light's colour
    (NormalizedAverageColor x its scale, RGB) and how much of it comes from each of
    the three directions of LM_BASIS (DirectionalMaxComponent, with its exponent and
    scale: the game's shaders' decoding), each h x wd x 3."""
    (sx, sy), (bx, by) = lm["coord_scale"], lm["coord_bias"]

    def sample(page):
        img = (page.astype(np.float32) / 255.0) ** lighting.GAMMA
        ph, pw = img.shape[:2]
        px = (bx + (np.arange(wd) + 0.5) / wd * sx) * pw - 0.5
        py = (by + (np.arange(h) + 0.5) / h * sy) * ph - 0.5
        x0, y0 = np.floor(px).astype(int), np.floor(py).astype(int)
        fx, fy = (px - x0)[None, :, None], (py - y0)[:, None, None]
        xa, xb = np.clip(x0, 0, pw - 1), np.clip(x0 + 1, 0, pw - 1)
        top, bottom = img[np.clip(y0, 0, ph - 1)], img[np.clip(y0 + 1, 0, ph - 1)]
        return ((top[:, xa] * (1 - fx) + top[:, xb] * fx) * (1 - fy)
                + (bottom[:, xa] * (1 - fx) + bottom[:, xb] * fx) * fy)

    color = sample(L.pages[lm["nac"]][1]) * np.asarray(lm["nac_scale"], np.float32)
    weights = (np.maximum(sample(L.pages[lm["dirmax"]][1]), 1e-4) ** np.asarray(lm["dirmax_exp"], np.float32)
               * np.asarray(lm["dirmax_scale"], np.float32))
    return color, weights


def _planet_stub(m):
    # (ctron_CLR_Mat's place until its placement is lit: _planet.)
    return {"name": "ctron_CLR_Mat", "pbrMetallicRoughness": {"baseColorFactor": [0.05, 0.05, 0.05, 1.0],
                                                               "metallicFactor": 0.0, "roughnessFactor": 1.0}}


def uv_frames(d, xform, h, wd):
    """Where each texel of an h x wd texture laid on a mesh's first UVs lies on it:
    its position, normal and tangent (world space, through the 4 x 4 `xform`) and its
    bitangent's sign (glTF's: bitangent = sign x normal x tangent), each h x wd x 3
    (the sign h x wd), from the mesh of Doc `d` rasterized in UV space. Texels it
    doesn't cover are left zero."""
    out_p = np.zeros((h, wd, 3), np.float32)
    out_n = np.zeros((h, wd, 3), np.float32)
    out_t = np.zeros((h, wd, 3), np.float32)
    out_s = np.ones((h, wd), np.float32)
    A, t = xform[:3, :3], xform[:3, 3]
    normal_m = np.linalg.inv(A).T
    for prim in d.g["meshes"][0]["primitives"]:
        at = prim["attributes"]
        if "TANGENT" not in at:
            continue
        pos = d.acc(at["POSITION"]).astype(np.float64) @ A.T + t
        nrm = d.acc(at["NORMAL"]).astype(np.float64) @ normal_m.T
        tan4 = d.acc(at["TANGENT"]).astype(np.float64)
        tan = tan4[:, :3] @ A.T
        uv = d.acc(at["TEXCOORD_0"]).astype(np.float64) * (wd, h) - 0.5
        for tri in d.acc(prim["indices"]).reshape(-1, 3):
            q = uv[tri]
            x0, y0 = np.floor(q.min(0)).astype(int)
            x1, y1 = np.ceil(q.max(0)).astype(int)
            x0, y0, x1, y1 = max(x0, 0), max(y0, 0), min(x1, wd - 1), min(y1, h - 1)
            if x1 < x0 or y1 < y0:
                continue
            gx, gy = np.meshgrid(np.arange(x0, x1 + 1), np.arange(y0, y1 + 1))
            (ax, ay), (bx_, by_), (cx, cy) = q
            den = (by_ - cy) * (ax - cx) + (cx - bx_) * (ay - cy)
            if abs(den) < 1e-12:
                continue
            l0 = ((by_ - cy) * (gx - cx) + (cx - bx_) * (gy - cy)) / den
            l1 = ((cy - ay) * (gx - cx) + (ax - cx) * (gy - cy)) / den
            l2 = 1.0 - l0 - l1
            inside = (l0 >= -1e-4) & (l1 >= -1e-4) & (l2 >= -1e-4)
            if not inside.any():
                continue
            lam = np.stack([l0[inside], l1[inside], l2[inside]], -1)
            ys, xs = gy[inside], gx[inside]
            out_p[ys, xs] = lam @ pos[tri]
            out_n[ys, xs] = lam @ nrm[tri]
            out_t[ys, xs] = lam @ tan[tri]
            out_s[ys, xs] = np.sign(tan4[tri[0], 3]) or 1.0
    return out_p, out_n, out_t, out_s


def _unit(v):
    return v / np.maximum(np.linalg.norm(v, axis=-1, keepdims=True), 1e-6)


def _planet(m, L, lm, d, xform, view):
    # ctron_CLR_Mat: Cybertron, far off, lit as the game lights it. The material
    # (High Moon's normalized Blinn-Phong, MLM_NormalizedBlinnPhong) takes its light
    # from the planet's own light map, as the game's compiled base pass shades it
    # (TBasePassPixelShaderFDirectionalTextureLightMapPolicy):
    #   diffuse   = its colour map x 2 x the light map's colour x each of its three
    #               directions' light weighed by (1.127 x the normal map's cosine to
    #               it)^2;
    #   highlight = ctron_SPEC x (25, 0.5, 0) (a red sheen) x (n + 8) / (8 pi) x
    #               (N.H)^n x the light map's colour x |d|^2 / sum d, n 50 x the mask's
    #               green, H halfway between the camera and where the light comes
    #               from (the directions weighed by their light, as long as 1 at
    #               most); d: the three directions' light.
    # That light never changes and the planet is seen from the Ark alone, so both
    # are baked into its colour here (the highlight as seen from the middle of the
    # play area, `view`), and the planet is drawn lit by plain white light (a texel
    # of the vertex-light page: the light map itself holds more light than a page
    # keeps). Its glow (ctron_GLOW2 x (50, 1, 0)), and its outline cut out by its
    # mask's blue (the material's opacity mask, clipped at a third). No highlights of
    # Bevy's own (they would be white, from the level's point lights, whose light the
    # game bakes). Outside the fog; "prelit": lighting.rs gives its white light as it
    # is.
    clr = linear(T("ctron_CLR")[..., :3])
    h, wd = clr.shape[:2]
    nrm = _sized(T("ctron_NORM")[..., :3], h, wd).astype(np.float32) / 127.5 - 1.0
    nrm = _unit(nrm)
    color, weights = lightmap_texels(L, lm, h, wd)
    cosines = np.clip(nrm @ LM_BASIS.T, 0.0, 1.0)
    lit = 2.0 * clr * color * (weights * (1.127 * cosines) ** 2).sum(-1, keepdims=True)
    # The highlight, from where the play area sees it.
    pos, n_ws, t_ws, sign = uv_frames(d, xform, h, wd)
    n_ws = _unit(n_ws)
    t_ws = _unit(t_ws - n_ws * (t_ws * n_ws).sum(-1, keepdims=True))
    b_ws = np.cross(n_ws, t_ws) * sign[..., None]
    v_ws = _unit(np.asarray(view, np.float32) - pos)
    v_ts = np.stack([(v_ws * t_ws).sum(-1), (v_ws * b_ws).sum(-1), (v_ws * n_ws).sum(-1)], -1)
    lum = color @ np.array([0.3, 0.59, 0.11], np.float32)
    l_ts = (weights * lum[..., None]) @ LM_BASIS
    length = np.linalg.norm(l_ts, axis=-1, keepdims=True) + 0.001
    l_ts = np.where(length > 1.0, l_ts / length, l_ts)
    half = _unit(v_ts + l_ts)
    nh = np.clip((nrm * half).sum(-1), 0.0, 1.0)
    mask = linear(_sized(T("ctron_MASK")[..., :3], h, wd))
    power = 50.0 * mask[..., 1]
    spec_color = linear(_sized(T("ctron_SPEC")[..., :3], h, wd)) * np.array([25.0, 0.5, 0.0], np.float32)
    light = color * ((weights ** 2).sum(-1) * lum / (lum * weights.sum(-1) + 1e-4))[..., None]
    sheen = np.where(nh > 1e-4, np.exp(np.log(np.maximum(nh, 1e-4)) * power), 0.0)
    lit = lit + spec_color * ((power + 8.0) * 0.0397887 * sheen)[..., None] * light
    lit = np.where((n_ws ** 2).sum(-1, keepdims=True) > 0, lit, 0.0)
    # (The game's tone curve shows anything past 11.2 as white: the few sharpest
    # glints, up to 200, are held at 12 so that the texture keeps its precision.)
    lit = np.minimum(lit, 12.0)
    # (The mask is a colour map: the game compares its decoded value with the third.)
    alpha = np.round(mask[..., 2] * 255).astype(np.uint8)
    tex, peak = m.hdr("ctron_CLR_Mat_lit", lit, alpha)
    glow, glow_peak = m.hdr("ctron_CLR_Mat_e", linear(T("ctron_GLOW2")[..., :3]) * np.array([50.0, 1.0, 0.0], np.float32))
    m.use_ext("KHR_materials_emissive_strength")
    m.use_ext("KHR_materials_specular")
    return {"name": "ctron_CLR_Mat",
            "pbrMetallicRoughness": {"baseColorTexture": tex, "baseColorFactor": [peak, peak, peak, 1.0],
                                     "metallicFactor": 0.0, "roughnessFactor": 1.0},
            "emissiveTexture": glow, "emissiveFactor": [1, 1, 1],
            "extensions": {"KHR_materials_emissive_strength": {"emissiveStrength": glow_peak},
                           "KHR_materials_specular": {"specularFactor": 0.0}},
            "alphaMode": "MASK", "alphaCutoff": 0.3333, "extras": foc_extras("nofog", "prelit")}


def _arrows(m):
    c = T("ENV_yellowarrowsM_CLR")
    rgba = np.concatenate([(c[..., :3].astype(np.float32) * 0.5).astype(np.uint8), c[..., 3:4]], -1)
    return {"name": "ENV_yellowarrows_CLR_Mat",
            "pbrMetallicRoughness": {"baseColorTexture": m.tex("ENV_yellowarrows", rgba), "metallicFactor": 0.0,
                                     "roughnessFactor": 0.6},
            "alphaMode": "MASK", "alphaCutoff": 0.333}


def _grate(m):
    c = T("ENV_RST_SewerGrate_CLR")[..., :3]
    mask = T("ENV_RST_SewerGrate_MASK")[..., 0]
    rgba = np.concatenate([c, mask[..., None]], -1)
    return {"name": "ENV_RST_SewerGrate_MAT",
            "pbrMetallicRoughness": {"baseColorTexture": m.tex("ENV_RST_SewerGrate", rgba), "metallicFactor": 0.0,
                                     "roughnessFactor": 0.5},
            "normalTexture": m.tex("ENV_RST_SewerGrate_n", normal_xy(T("ENV_RST_SewerGrate_NORM")), "normal"),
            "alphaMode": "MASK", "alphaCutoff": 0.333, "doubleSided": True}


def tiled(m, info, tiling):
    """A glTF texture reference, repeated `tiling` (u, v) times over the UVs
    (KHR_texture_transform: Bevy takes the base colour's for the whole material)."""
    if tiling is None or tuple(tiling) == (1, 1):
        return info
    m.use_ext("KHR_texture_transform")
    return dict(info, extensions={"KHR_texture_transform": {"scale": [float(tiling[0]), float(tiling[1])]}})


def _plain(name, clr, nrm, mul=1.0, glow=None, mask=False, tiling=None, gain=1.0, key=None):
    """A lit material from a colour map (times `mul`: a number or an RGB triple), its
    normal map, a glow (a function giving the emissive light in the game's units,
    linear, at any size) and, with `mask`, cut out by the colour map's alpha.
    `tiling` repeats its textures (u, v) times over the mesh's UVs (the material's
    TextureCoordinate tiling), `gain` multiplies its colour as light (the material's
    factor, which may pass 1) and `key` names its textures, for materials that share
    them."""
    def f(m):
        k = key or name
        c = T(clr)
        rgb = (c[..., :3].astype(np.float32) * np.asarray(mul, np.float32)).clip(0, 255).astype(np.uint8)
        g = {"name": name, "pbrMetallicRoughness": {"metallicFactor": 0.0, "roughnessFactor": 0.6}}
        if gain != 1.0:
            g["pbrMetallicRoughness"]["baseColorFactor"] = [gain, gain, gain, 1.0]
        if mask:
            g["pbrMetallicRoughness"]["baseColorTexture"] = tiled(m, m.tex(k + "_c", np.concatenate([rgb, c[..., 3:4]], -1)),
                                                                  tiling)
            g["alphaMode"] = "MASK"
            g["alphaCutoff"] = 0.333
        else:
            g["pbrMetallicRoughness"]["baseColorTexture"] = tiled(m, m.tex(k + "_c", rgb), tiling)
        if nrm:
            g["normalTexture"] = tiled(m, m.tex(k + "_n", normal_xy(T(nrm)), "normal"), tiling)
        if glow is not None:
            tex, peak = m.hdr(k + "_e", glow())
            g["emissiveTexture"] = tiled(m, tex, tiling)
            g["emissiveFactor"] = [1, 1, 1]
            g["extensions"] = {"KHR_materials_emissive_strength": {"emissiveStrength": peak}}
            m.use_ext("KHR_materials_emissive_strength")
        return g
    return f


def _glass(m):
    # ENV__ARK__Glass3_MATINST (ENV_Refractive_Glass_Trans_Mat): nearly clear blue
    # glass; here a faint blue tint.
    return {"name": "ENV__ARK__Glass3_MATINST",
            "pbrMetallicRoughness": {"baseColorFactor": [0.06, 0.35, 0.5, 0.18], "metallicFactor": 0.0,
                                     "roughnessFactor": 0.1},
            "alphaMode": "BLEND", "doubleSided": True}


def _space_dome(m):
    raise RuntimeError("the space dome is built by space_dome()")


# Downfall's (ESC_Iacon) own materials. The vintage props (ENV_NEU_vintageprops_p)
# are plain colour and normal maps, some with glows; the computers' glow is
# VT_pc01_GLOW x (1 + 2 x VT_pc01_GLOW2) x 5, GLOW2 panning across it (here as it
# stands at the start), white or tinted (0, 2, 10).
def _pc_glow(tint):
    def glow():
        g = linear(T("VT_pc01_GLOW")[..., :3])
        h, w = g.shape[:2]
        g2 = linear(np.asarray(Image.fromarray(T("VT_pc01_GLOW2")[..., :3]).resize((w, h), Image.BILINEAR)))
        return g * (1.0 + 2.0 * g2) * 5.0 * np.asarray(tint, np.float32)
    return glow


def _lights_glow():
    return linear(T("vint_lights_GLOW")[..., :3]) * 6.0


def _hologram(m):
    # ENV_ASH_MaproomMonitorsTop_MATINST (ENV_Hologram_Mat): the map room's hologram
    # panels, ENV_IAC_MaproomMonitorBIG02_CLR glowing in its Emissive_Color
    # (0.02, 0.906, 1.0) x EmissiveIntensity 2, over a dim desaturated copy.
    c = T("ENV_IAC_MaproomMonitorBIG02_CLR")[..., :3]
    gray = c.astype(np.float32).mean(-1, keepdims=True) * 0.25 + c.astype(np.float32) * 0.0
    tex, peak = m.hdr("ENV_ASH_MaproomMonitorsTop_e", linear(c) * np.array([0.02, 0.906, 1.0]) * 2.0)
    m.use_ext("KHR_materials_emissive_strength")
    return {"name": "ENV_ASH_MaproomMonitorsTop_MATINST",
            "pbrMetallicRoughness": {"baseColorTexture": m.tex("ENV_ASH_MaproomMonitorsTop_c",
                                                               np.repeat(gray, 3, -1).clip(0, 255).astype(np.uint8)),
                                     "metallicFactor": 0.0, "roughnessFactor": 0.5},
            "emissiveTexture": tex, "emissiveFactor": [1, 1, 1],
            "extensions": {"KHR_materials_emissive_strength": {"emissiveStrength": peak}}}


def _monitor_screen(m):
    # MonitorScreenActive_MAT_INST (MonitorScreen_Parent_MAT): emissive only,
    # ScreenText x 1.2 x (ScreenText x 0.05 + ((ScreenText + 0.9) x Screen_Color
    # (0.47, 0.752, 0.966) x 0.7)^2 x flicker), the scan lines panning and the flicker
    # (0.5 + a sine, clamped to 0.75-1) as they stand at the start.
    t = linear(T("ScreenText")[..., :3])
    c = np.array([0.47, 0.752, 0.966], np.float32)
    e = t * 1.2 * (t * 0.05 + ((t + 0.9) * c * 0.7) ** 2 * 0.75)
    return unlit_hdr(m, "MonitorScreenActive_MAT_INST", e, flags=("nolight",))


def _flipbook(mat, tex_name, strength, opacity=None):
    # Unlit, two-sided screens showing a TextureFlipBook x `strength` (and, with
    # `opacity`, see-through): the map room's ENV_ASH_MaproomFlipbook0n_MAT (2 x 2
    # frames, two a second, x 3), the consoles' AP_EscalationConsole*Flipbook_MAT
    # (x 2.5, 0.8 opaque). umodel doesn't export flip books, so the texture is read
    # from the packages; the demo shows its frames in turn (lighting.rs).
    def f(m):
        img, flag = m.package_flipbook(tex_name)
        lin = linear(img[..., :3]) * strength
        if opacity is None:
            return unlit_hdr(m, mat, lin, flags=("nolight", flag), double=True)
        alpha = np.full(lin.shape[:2], round(opacity * 255), np.uint8)
        return unlit_hdr(m, mat, lin, "BLEND", ("nolight", flag), True, alpha)
    return f


def _console_keyboard(m):
    # AP_EscalationConsolKeyboard_MAT: unlit, two-sided and half see-through (opacity
    # 0.5): AP_EscalationConsoleKeyboard_CLR x 10 in orange (1, 0.45, 0.1).
    c = linear(T("AP_EscalationConsoleKeyboard_CLR")[..., :3])
    alpha = np.full(c.shape[:2], 128, np.uint8)
    return unlit_hdr(m, "AP_EscalationConsolKeyboard_MAT", c, "BLEND", ("nolight",), True, alpha,
                     factor=(10.0, 4.5, 1.0))


def _tinted_glass(name, tint):
    # ENV_Refractive_Glass_Trans_Mat's instances: nearly clear glass in its light
    # colour (the consoles' AP_EscalationConsoleGlass_MATINST: (0.085, 0.598, 1.0));
    # here a faint tint, as Oblivion's glass.
    def f(m):
        r, g, b = (0.5 * c for c in tint)
        return {"name": name, "pbrMetallicRoughness": {"baseColorFactor": [r, g, b, 0.15], "metallicFactor": 0.0,
                                                       "roughnessFactor": 0.1},
                "alphaMode": "BLEND", "doubleSided": True}
    return f


def _pod_glass(m):
    # AP_StoragePod_Glass_MAT_INST (ENV_Refractive_Glass_Opaque_Mat as it comes):
    # unlit and opaque, black but for a sharp highlight of its light colour
    # (0.187, 0.505, 0.325) x 5 (clamp(R . L)^25); here its average over the view.
    m.use_ext("KHR_materials_unlit")
    return {"name": "AP_StoragePod_Glass_MAT_INST",
            "pbrMetallicRoughness": {"baseColorFactor": [0.036, 0.097, 0.062, 1.0], "metallicFactor": 0.0,
                                     "roughnessFactor": 1.0},
            "extensions": {"KHR_materials_unlit": {}}, "extras": foc_extras("nolight")}


def _hull(m):
    # HmDefaultDoubleSidedMaterial: the far sides of the BSP's brushes, the level's
    # outer hull. The level gives them no light (their surfaces don't accept light:
    # no light maps), so the game draws them black, from both sides, in the haze.
    m.use_ext("KHR_materials_unlit")
    return {"name": "HmDefaultDoubleSidedMaterial",
            "pbrMetallicRoughness": {"baseColorFactor": [0.0, 0.0, 0.0, 1.0], "metallicFactor": 0.0,
                                     "roughnessFactor": 1.0},
            "extensions": {"KHR_materials_unlit": {}}, "doubleSided": True, "extras": foc_extras("nolight")}


def _electro(m):
    # ElectroPlane_MAT: crackling blue energy - Energy01's arcs panning across each
    # other, x 4, in BG Color (0.1, 0.435, 1.0) squared, added on; here its arcs as
    # they stand at the start.
    e = linear(T("Energy01")[..., :3]).max(-1, keepdims=True)
    lin = np.clip(e * 4.0, 0, 1) * np.array([0.01, 0.19, 1.0], np.float32) * 2.0
    return unlit_hdr(m, "ElectroPlane_MAT", lin, "BLEND", ("add", "nolight"), True)


def _iacon_sky(m):
    # ESC_IaconColorVariant_MAT's sky: ENV_RST_MoreBlueSky_CLR x (3, 0.75, 1).
    # (Like every sky, it casts no shadow: "noshadow". The sun's shadows would
    # otherwise start at the dome, which surrounds the map, and cover all of it.)
    c = linear(T("ENV_RST_MoreBlueSky_CLR")[..., :3])
    return unlit_hdr(m, "ESC_IaconColorVariant_Sky", c, flags=("nofog", "nolight", "noshadow"), double=True,
                     factor=(3.0, 0.75, 1.0))


# A layer added on over a sky dome, on the dome's own triangles: its depth pulled
# nearer by this many of the depth buffer's smallest steps (a centimetre or two at the
# dome's kilometres), so it never loses the depth test to the dome under it. Drawn by
# a pipeline of its own, it can work its depth out a step from the dome's, and the
# layer would flicker in and out as the camera moves. (Not always enough, so the demo
# also draws the dome itself a fraction further out: lighting.rs's SKY_BASE_PUSH.)
SKY_LAYER_BIAS = "depthbias:16"


def _iacon_sun(m):
    # ... and the sun's glow on it: MoreBlueSky x ENV_RST_BlueSky_MASK2 x 40 x
    # (15, 2, 1), far brighter than the sky, so drawn added on with its own texture
    # scale. (Both sRGB textures: the game samples them to linear light.)
    c = linear(T("ENV_RST_MoreBlueSky_CLR")[..., :3])
    h, wd = c.shape[:2]
    mask = linear(np.asarray(Image.fromarray(T("ENV_RST_BlueSky_MASK2")[..., :3]).resize((wd, h), Image.BILINEAR)))
    return unlit_hdr(m, "ESC_IaconColorVariant_Sun", c * mask, "BLEND", ("add", "nofog", "nolight", "noshadow", SKY_LAYER_BIAS),
                     True, factor=(40.0 * 15.0, 40.0 * 2.0, 40.0))


# A far piece of sky letting through less of itself than this (the share of its
# light the level's fog lets through, at most) is hidden by the fog: none of it is
# drawn, and a dome so hidden is drawn as the fog's haze alone (_sky_haze_opaque).
SKY_HIDDEN = 1e-4


def _sky_haze_opaque(m):
    # A sky dome the level's height fog hides completely (fogged_sky): the fog's haze
    # alone, its light in the dome's vertex colours, drawn opaque (a dome left black
    # under an added-on haze shows black wherever the two meet unevenly in depth).
    if m is not None:
        m.use_ext("KHR_materials_unlit")
    return {"name": "SkyHazeOpaque", "pbrMetallicRoughness": {"baseColorFactor": [1.0, 1.0, 1.0, 1.0],
                                                              "metallicFactor": 0.0, "roughnessFactor": 1.0},
            "extensions": {"KHR_materials_unlit": {}}, "extras": foc_extras("nofog", "nolight", "noshadow")}


def _sky_haze(m):
    # The level's height fog seen against the sky: added on, its light in the dome's
    # vertex colours (iacon_sky). (Like the dome under it, it casts no shadow: it
    # surrounds the map, and the sun's shadows would start at it.)
    m.use_ext("KHR_materials_unlit")
    return {"name": "SkyHaze", "pbrMetallicRoughness": {"baseColorFactor": [1.0, 1.0, 1.0, 1.0], "metallicFactor": 0.0,
                                                        "roughnessFactor": 1.0},
            "extensions": {"KHR_materials_unlit": {}}, "alphaMode": "BLEND", "doubleSided": True,
            "extras": foc_extras("add", "nofog", "nolight", "noshadow", SKY_LAYER_BIAS)}


# Ancients' (ESC_Temple) own materials. The rocks and debris (AAA_RS_ENV_01_p,
# ENV_NEU_Variation_p, ENV_NEU_floortest_p, ENV_NEU_vintageprops_p) are plain colour
# and normal maps, some tiled, scaled or glowing (their specular, and the vintage
# props' cube-map reflections, are left out).
def _cagelight_glow():
    # cagelight_CLR_Mat's emissive: cagelight_GLOW x (1, 10, 20).
    return linear(T("cagelight_GLOW")[..., :3]) * np.array([1.0, 10.0, 20.0], np.float32)


def _scaled_glow(tex_name, gain):
    def glow():
        return linear(T(tex_name)[..., :3]) * gain
    return glow


def _fake_shadow(m):
    # DebrisFakeShadow_MAT: the floor debris's painted shadows, unlit and see-through:
    # black (an unlit material shows only its emissive, which it hasn't), at
    # DebrisShadowCLR_MASK's alpha (half at most).
    c = T("DebrisShadowCLR_MASK")
    return unlit(m, "DebrisFakeShadow_MAT", np.zeros_like(c[..., :3]), (1, 1, 1), "BLEND", ("nolight",), False,
                 c[..., 3])


def _toxic_sludge(m):
    # ENV_RST_ToxicSludge_MAT: an added-on glow - ENV_RST_ToxicSludge panning both ways
    # over itself, plus BeamTongue_Pulse_2_CLR twice, drifting slowly (one at twice its
    # width); as they stand at the start, where the sludge's two copies coincide.
    s = linear(T("ENV_RST_ToxicSludge")[..., :3])
    h, wd = s.shape[:2]
    beam = T("BeamTongue_Pulse_2_CLR")[..., :3]
    b1 = linear(np.asarray(Image.fromarray(beam).resize((wd, h), Image.BILINEAR)))
    b2 = linear(np.asarray(Image.fromarray(np.ascontiguousarray(np.tile(beam, (1, 2, 1)))).resize((wd, h), Image.BILINEAR)))
    return unlit_hdr(m, "ENV_RST_ToxicSludge_MAT", s * 2.0 + b1 + b2, "BLEND", ("add", "nolight"), True)


def _escalation_field(name, wave, fresnel, tiling):
    # (Only if its game material can't be compiled: FX_MATERIALS draws them.)
    # ENV_ForceField_ALL_MAT's Escalation instances: faint, added-on energy closing off
    # the map's edges - the wave colour x WaveIntensity 2 on the ChainLinkFence_02_MASK
    # pattern (repeated Wave_Alpha_U/VScale times), over the fresnel colour x
    # BgIntensity 0.75, all x OverallIntensity 0.05; here without the waves' movement
    # and the fresnel's turn with the view.
    def f(m):
        mask = T("ChainLinkFence_02_MASK")[..., 0].astype(np.float32) / 255.0
        lin = (mask[..., None] * np.array(wave, np.float32) * 2.0
               + np.array(fresnel, np.float32) * 0.75) * 0.05
        g = unlit_hdr(m, name, lin, "BLEND", ("add", "nolight"), True)
        g["pbrMetallicRoughness"]["baseColorTexture"] = tiled(m, g["pbrMetallicRoughness"]["baseColorTexture"], tiling)
        return g
    return f


# ForceField_ESC_Matinst (Ancients): orange (1, 0.42, 0) waves, 2 x 2, over (0.44, 0.24, 0.16).
_orange_field = _escalation_field("ForceField_ESC_Matinst", (1.0, 0.418, 0.0), (0.443, 0.235, 0.161), (2, 2))


def _grey_dome(m):
    # greytest: the sky dome's grey (0.2), lit by the sun - baked, with the level's fog
    # over it, into the dome's vertex colours (fog_dome), so drawn unlit and outside
    # the demo's fog. (Like every sky, it casts no shadow: "noshadow".)
    m.use_ext("KHR_materials_unlit")
    return {"name": "greytest", "pbrMetallicRoughness": {"baseColorFactor": [1.0, 1.0, 1.0, 1.0], "metallicFactor": 0.0,
                                                         "roughnessFactor": 1.0},
            "extensions": {"KHR_materials_unlit": {}}, "extras": foc_extras("nofog", "nolight", "noshadow")}


# Ignition's (ESC_Gladiator) own materials. Its sky (gladiator_sky, sky_card) is a dome
# of IaconSunset3_CLR with stars, banks of bright cloud (rs_skydome_STAT), cards of
# stars and cloud and two moons, all far out and seen through the level's height fog
# (level_fog_layers), which hides most of it in its dark green; the moons, nearer,
# show through it, each in its glow spheres (GlowSphere_STAT: effect meshes in the
# sky, SKY_FX_DISTANCE).
# ForceField_ESC_GREEN_Matinst: green (0.27, 1, 0.19) waves, 1 x 1, over white.
_green_field = _escalation_field("ForceField_ESC_GREEN_Matinst", (0.271, 1.0, 0.188), (1.0, 1.0, 1.0), (1, 1))


def _dropship_shield(m):
    # Sentinel_ForceField_Mat_INST (WEP_ForceField_B_Mat): the far dropship's shield, a
    # fresnel glow - Fresnel_Color (0.16, 0.43, 0.97) x Fresnelntensity 15 x
    # (1 - N.V)^6 (its electric arcs are off: EnergonElecticIntensity 0); here that
    # glow's average over the shield as it is seen (15 x 2/56), added on.
    lin = np.ones((4, 4, 3), np.float32) * np.array([0.160, 0.426, 0.966], np.float32) * (15.0 * 2.0 / 56.0)
    return unlit_hdr(m, "Sentinel_ForceField_Mat_INST", lin, "BLEND", ("add", "nolight"), True)


def _ash_dome(m):
    # Spacedome_ashsunset_matINST (SkyDome_PARENT_MAT): (Intensity 2 x SkyColorTint
    # (0.52, 0.53, 0.53) x (IaconSunset3_CLR + the stars))^Contrast 1.89, unlit. The
    # stars are Starfield_CLR at its UVs and at 5 times them (LargeStarsTileXY and
    # SmallStarsTileXY) x 400, let through where Cloud_Mask^StarQty 10 and Cloud_Mask
    # at half its size x 2 allow, as they stand at the start (they turn and pan very
    # slowly), their brightest held to 2 before the contrast. Seen through the fog
    # (gladiator_sky's vertex colours), so drawn outside the demo's.
    n = 2048
    u = (np.arange(n) + 0.5) / n
    U, V = np.meshgrid(u, u)

    def samp(img, uu, vv):
        h, wd = img.shape[:2]
        return img[(np.floor(vv % 1.0 * h)).astype(int) % h, (np.floor(uu % 1.0 * wd)).astype(int) % wd]

    stars = T("Starfield_CLR")[..., :3].astype(np.float32) / 255
    cloud = T("Cloud_Mask")[..., 0].astype(np.float32) / 255
    s = samp(stars, U, V) + 2.0 * samp(stars, U * 5.0, V * 5.0)
    let = samp(cloud, U, V) ** 10.0 * (samp(cloud, U * 0.5, V * 0.5) * 2.0)
    sky = linear(np.asarray(Image.fromarray(T("IaconSunset3_CLR")[..., :3]).resize((n, n), Image.BILINEAR)))
    x = 2.0 * np.array([0.5216, 0.5255, 0.5294], np.float32) * (sky + s * let[..., None] * 400.0)
    lin = np.minimum(x, 2.0) ** 1.894
    return unlit_hdr(m, "Spacedome_ashsunset_matINST", lin, flags=("nofog", "nolight", "noshadow"), double=True)


def _sky_piece(name, tex_name, gain, opacity="A"):
    # A far piece of Ignition's sky, unlit and see-through: its colour map x `gain`
    # (the material's factor) x its opacity (the map's alpha, or its red for "R"; an
    # additive material's too: the game's base pass multiplies what it adds by its
    # opacity, vint_clouds_CLR_Mat's the cloud map's alpha, which fades the card out
    # to its edges), drawn added on, with what gets through the fog in its vertex
    # colours (sky_card). The clouds' colour maps drift in the game (panned by
    # SmokeTile_CLR); here as they stand at the start.
    def f(m):
        c = T(tex_name)
        lin = linear(c[..., :3]) * gain
        if opacity == "A":
            lin = lin * (c[..., 3:4].astype(np.float32) / 255)
        elif opacity == "R":
            lin = lin * linear(c[..., 0:1])
        return unlit_hdr(m, name, lin, "BLEND", ("add", "nofog", "nolight", "noshadow"), True)
    return f


def level_fog_layers(pkgs):
    """The height-fog layers the level's sky is seen through, as the game draws them
    (`pkgs`: the level's packages, its art package first; lighting.fog_and_post:
    Unreal units): its post-process volume's fog, if it has one (the play area's,
    as write_lights picks it), and its HeightFogs that are on, each a layer of its
    own (the game draws up to four), their densities scaled by the level's
    FOG_DensityWorldScale (lighting.world_fog_scale). Ignition has both: the
    volume's pale green fog up to 117 m and a HeightFog's dense dark green one from
    350 m out up to 616 m, at 0.3 of their densities. Where the volume's fog copies
    a HeightFog that is off (Oblivion's), there is just the one."""
    fp = lighting.fog_and_post(pkgs[0])
    layers = [fp["PostProcessVolumeHeightFogComponent"]] if "PostProcessVolumeHeightFogComponent" in fp else []
    layers += fp.get("HeightFogLayers") or []
    if not layers and "HeightFogComponent" in fp:
        layers = [fp["HeightFogComponent"]]
    scale = lighting.world_fog_scale(pkgs)
    return [dict(f, density=f["density"] * scale) for f in layers]


def subdivide(pos, uv, nrm, tris, levels):
    """Splits every triangle into four, `levels` times (the new corners at the edges'
    middles, attributes averaged as the GPU interpolates them)."""
    for _ in range(levels):
        mid = {}
        pos, uv, nrm = list(pos), list(uv), list(nrm)

        def middle(a, b):
            k = (min(a, b), max(a, b))
            if k not in mid:
                mid[k] = len(pos)
                pos.append((pos[a] + pos[b]) / 2)
                uv.append((uv[a] + uv[b]) / 2)
                n = nrm[a] + nrm[b]
                nrm.append(n / max(np.linalg.norm(n), 1e-9))
            return mid[k]
        out = []
        for a, b, c in tris:
            ab, bc, ca = middle(a, b), middle(b, c), middle(c, a)
            out += [(a, ab, ca), (ab, b, bc), (ca, bc, c), (ab, bc, ca)]
        pos, uv, nrm, tris = np.array(pos), np.array(uv), np.array(nrm), np.array(out)
    return pos, uv, nrm, tris


# What the game takes for a ray's rise when it runs level (Unreal units): its
# height fog's shaders divide by it.
FOG_LEVEL_RISE = 0.001


def ray_fog(world, center, layers):
    """How the level's height fog (level_fog_layers) covers far points `world` (Bevy
    metres) seen from `center`, as the game's fog does (UE3's layered HeightFog as
    Fall of Cybertron sets it up, FSceneRenderer::InitFogConstants, and its shaders'
    height fog, which its translucent materials work out at each vertex): the layers
    taken highest first, each fills the slab from the next one's top up to its own
    (the lowest, everything below it); a ray thins in a layer by 1 - its density for
    each unit of the ray's length past the layer's start distance, times the part of
    the ray that lies in its slab; and the light it adds is 1 / ln 2 of its colour x
    brightness for what it hides (build_level.fog_light). The layers are laid one
    over another from the far side of the camera's own (those wholly below the
    camera from the lowest up, then those above it from the highest down). Returns,
    for each point, the part of its own light that gets through and the light the
    fog adds."""
    order = sorted(layers, key=lambda f: -f["height"])
    cz = center[1] * 100.0
    below = [k for k in range(len(order)) if order[k]["height"] <= cz]
    seq = sorted(below, reverse=True) + [k for k in range(len(order)) if k not in below]
    ray = (world - np.asarray(center, np.float64)) * 100.0
    dist = np.linalg.norm(ray, axis=1)
    rise = np.where(np.abs(ray[:, 1]) <= FOG_LEVEL_RISE, FOG_LEVEL_RISE, ray[:, 1])
    through = np.ones(len(world))
    light = np.zeros((len(world), 3))
    for k in seq:
        fog = order[k]
        top = fog["height"]
        bottom = order[k + 1]["height"] if k + 1 < len(order) else -327680.0
        part = np.abs(np.clip((top - cz) / rise, 0.0, 1.0) - np.clip((bottom - cz) / rise, 0.0, 1.0))
        length = part * np.maximum(dist - fog["start"], 0.0)
        t = (1.0 - min(fog["density"], 0.999999)) ** length
        through = through * t
        light = light * t[:, None] + (1.0 - t)[:, None] * bl.fog_light(fog) * linear(np.array(fog["color"], np.float32))
    return through, light


def fogged_sky(m, w, d, M, center, draws, levels=2):
    """A far mesh of the sky placed by `M`, seen through the level's height fog: the
    game's fog covers the sky too (ray_fog from the play area's `center`, Bevy
    metres), which is baked into the mesh's vertices (cut finer for it, `levels`
    times). `draws(primitive index, primitive)` gives what each primitive is drawn
    as: [(material, "clear" | "haze")], unlit materials whose colour the vertex
    colours multiply - "clear" by what gets through the fog, "haze" (_sky_haze) giving
    the light the fog adds. Where the fog hides the piece completely (less than
    SKY_HIDDEN of it gets through), its "clear" draws are left out and its haze is
    drawn opaque (_sky_haze_opaque). Returns the primitives."""
    layers = level_fog_layers(m.packages)
    A = bl.P @ M[:3, :3].T @ bl.P
    t = bl.P @ M[3, :3] * 0.01
    out = []
    for pi, p in enumerate(d.g["meshes"][0]["primitives"]):
        at = p["attributes"]
        pos, uv, nrm, tris = subdivide(d.acc(at["POSITION"]).astype(np.float64), d.acc(at["TEXCOORD_0"]).astype(np.float64),
                                       d.acc(at["NORMAL"]).astype(np.float64), d.acc(p["indices"]).reshape(-1, 3), levels)
        through, light = ray_fog(pos @ A.T + t, center, layers)
        clear = np.repeat(through[:, None], 4, 1).astype(np.float32)
        haze = np.concatenate([light, np.ones((len(pos), 1))], 1).astype(np.float32)
        clear[:, 3] = 1.0
        attrs = {"POSITION": w.accessor(pos.astype(np.float32), 34962, True),
                 "NORMAL": w.accessor(nrm.astype(np.float32), 34962),
                 "TEXCOORD_0": w.accessor(uv.astype(np.float32), 34962)}
        idx = w.accessor(tris.reshape(-1).astype(np.uint32), 34963)
        todo = draws(pi, p)
        if through.max() < SKY_HIDDEN:
            todo = [("SkyHazeOpaque", kind) for _, kind in todo if kind == "haze"]
        colors = {"clear": w.accessor(clear, 34962)}
        if any(kind == "haze" for _, kind in todo):
            colors["haze"] = w.accessor(haze, 34962)
        for name, kind in todo:
            out.append({"attributes": dict(attrs, COLOR_0=colors[kind]), "indices": idx, "material": m.get(name)})
    return out


def iacon_sky(m, w, d, M, center, over=()):
    """RST_SkydomeTransportLevel_STAT with ESC_IaconColorVariant_MAT, placed by `M`:
    Downfall's sunset sky (_iacon_sky, _iacon_sun), unlit, seen through the level's
    height fog (fogged_sky): from the play area Downfall's three layers hide about
    three quarters of the sky straight up and all of it at the horizon. The sky and the
    glow x what gets through, plus the haze. Returns the dome's primitives."""
    return fogged_sky(m, w, d, M, center, lambda pi, p: [("ESC_IaconColorVariant_Sky", "clear"),
                                                         ("ESC_IaconColorVariant_Sun", "clear"), ("SkyHaze", "haze")])


def gladiator_sky(m, w, d, M, center, over=()):
    """ENV_SkyDome_STAT with Spacedome_ashsunset_matINST (_ash_dome), placed by `M`:
    Ignition's sky dome, seen through the level's height fog (fogged_sky) - its
    post-process volume's, pale green up to 117 m, and its HeightFog's, dark green
    above that up to 616 m from 350 m out, at 0.3 of their densities (the level's
    FOG_DensityWorldScale): the sky is that fog's dark green with the dome a faint
    shading in it, as the game shows it (a dome the fog hides completely is drawn
    as the haze alone, opaque: fogged_sky)."""
    mat = next((o for o in over if o), "Spacedome_ashsunset_matINST")
    return fogged_sky(m, w, d, M, center, lambda pi, p: [(mat, "clear"), ("SkyHaze", "haze")])


def sky_card(m, w, d, M, center, over=(), levels=1):
    """A far piece of Ignition's sky (SKY_PIECES: the cloud banks of rs_skydome_STAT, the
    star and cloud cards, the moons), placed by `M`: drawn added on (_sky_piece) with
    what gets through the fog in its vertex colours (fogged_sky, its mesh cut `levels`
    times; the fog the dome behind already shows). Returns its primitives."""
    def draws(pi, p):
        name = over[pi] if pi < len(over) and over[pi] else (
            d.g["materials"][p["material"]]["name"] if "material" in p else None)
        return [(name, "clear")]
    return fogged_sky(m, w, d, M, center, draws, levels)


def cloud_banks(m, w, d, M, center, over=()):
    """rs_skydome_STAT (RS_sky_strip_Mat, RS_sky_top_Mat): Ignition's banks of cloud
    around and above the map, 2.7 km across - a sky_card, cut finer."""
    return sky_card(m, w, d, M, center, over, 2)


def fog_dome(m, w, d, M, center, over=()):
    """ENV_SkyDome_BLANK_STAT with greytest, placed by `M`: Ancients' sky, a grey dome
    (0.2) lit by the sun, seen through all of the level's height-fog layers (both
    packages': the bright haze far out, the play area's own, and the green one in its
    pits, which views of the sky don't reach). Both are baked into its vertices, as
    iacon_sky does: the sunlit grey x what gets through the fog (_grey_dome), plus
    the haze (_sky_haze). Returns the dome's primitives."""
    layers = [f for pkg in m.packages for f in lighting.fog_and_post(pkg).get("HeightFogLayers", [])]
    sun = lighting.directional_light(m.packages[0])
    A = bl.P @ M[:3, :3].T @ bl.P
    t = bl.P @ M[3, :3] * 0.01
    to_normals = np.linalg.inv(A).T
    out = []
    for p in d.g["meshes"][0]["primitives"]:
        at = p["attributes"]
        pos, uv, nrm, tris = subdivide(d.acc(at["POSITION"]).astype(np.float64), d.acc(at["TEXCOORD_0"]).astype(np.float64),
                                       d.acc(at["NORMAL"]).astype(np.float64), d.acc(p["indices"]).reshape(-1, 3), 2)
        through, light = ray_fog(pos @ A.T + t, center, layers)
        n = nrm @ to_normals.T
        n /= np.maximum(np.linalg.norm(n, axis=1, keepdims=True), 1e-9)
        lit = np.zeros((len(pos), 3))
        if sun:
            to_sun = -(bl.P @ np.asarray(sun["direction"], np.float64))
            lit = (0.2 * sun["brightness"] * linear(np.array(sun["color"], np.float32))
                   * np.clip(n @ to_sun, 0.0, None)[:, None])
        own = np.concatenate([lit * through[:, None], np.ones((len(pos), 1))], 1).astype(np.float32)
        haze = np.concatenate([light, np.ones((len(pos), 1))], 1).astype(np.float32)
        attrs = {"POSITION": w.accessor(pos.astype(np.float32), 34962, True),
                 "NORMAL": w.accessor(nrm.astype(np.float32), 34962),
                 "TEXCOORD_0": w.accessor(uv.astype(np.float32), 34962)}
        idx = w.accessor(tris.reshape(-1).astype(np.uint32), 34963)
        mat = d.g["materials"][p["material"]]["name"] if "material" in p else "greytest"
        out.append({"attributes": dict(attrs, COLOR_0=w.accessor(own, 34962)), "indices": idx, "material": m.get(mat)})
        out.append({"attributes": dict(attrs, COLOR_0=w.accessor(haze, 34962)), "indices": idx, "material": m.get("SkyHaze")})
    return out


SPECIAL = {
    "Black_Mat": _black,
    "ESC_Shield_tiled2": _forcefield,
    "spacehole_CLR_Mat": _spacehole,
    "EnergonSigns_CLR_Mat": _energon_signs("EnergonSigns_CLR", 3.0),
    "BlueEnergonSigns_MATINST": _energon_signs("EnergonSigns2_CLR", 3.0),
    "ENV_RST_VortexClouds2_MAT": _vortex_clouds,
    "ENV_RST_VortexBlackSwril2_MAT": _vortex_eye,
    "Vortex_MAT_INST": _vortex,
    "ctron_CLR_Mat": _planet_stub,
    "ENV_yellowarrows_CLR_Mat": _arrows,
    "ENV_RST_SewerGrate_MAT": _grate,
    "vint_floorstripe_CLR_Mat": _plain("vint_floorstripe", "vint_floorstripe_CLR", "vint_floorstripe_NORM"),
    "FLR_ORbump_CLR_Mat": _plain("FLR_ORbump", "FLR_orangebump_CLR", "FLR_redbump_NORM", 0.8),
    "ENV__ARK__Glass3_MATINST": _glass,
    "ENV__ARK__Glass2_MATINST": _glass,
    # Downfall
    "vint_pipeset_CLR_Mat": _plain("vint_pipeset", "vint_pipeset_CLR", "vint_pipeset_NORM"),
    "vint_wires_CLR_Mat": _plain("vint_wires", "vint_wires_CLR", "vint_wires_NORM"),
    "vint_box_CLR_Mat": _plain("vint_box", "vint_box_CLR", "vint_box_NORM"),
    "vint_signs_CLR_Mat": _plain("vint_signs", "vint_signs_CLR", "vint_signs_NORM"),
    "vint_lights_CLR_Mat": _plain("vint_lights", "vint_lights_CLR", "vint_lights_NORM", glow=_lights_glow),
    "vint_backgroundBLDG1_CLR_Mat": _plain("vint_backgroundBLDG1", "vint_backgroundBLDG1_CLR",
                                           "vint_backgroundBLDG1_NORM", mask=True),
    "vint_backgroundBLDG2_CLR_Mat": _plain("vint_backgroundBLDG2", "vint_backgroundBLDG2_CLR",
                                           "vint_backgroundBLDG2_NORM", mask=True),
    "VT_pc01_CLR_Mat": _plain("VT_pc01", "VT_pc01_CLR", "VT_pc01_NORM", glow=_pc_glow((1.0, 1.0, 1.0))),
    "VT_pc01_blue_CLR_Mat": _plain("VT_pc01_blue", "VT_pc01_CLR", "VT_pc01_NORM", glow=_pc_glow((0.0, 2.0, 10.0))),
    "VT_pc01_GOLD_CLR_Mat": _plain("VT_pc01_GOLD", "VT_pc01_CLR", "VT_pc01_NORM", (0.5, 0.4, 0.2),
                                   glow=_pc_glow((0.0, 2.0, 10.0))),
    "ENV_ASH_MaproomMonitorsTop_MATINST": _hologram,
    "MonitorScreenActive_MAT_INST": _monitor_screen,
    "ENV_ASH_MaproomFlipbook01_MAT": _flipbook("ENV_ASH_MaproomFlipbook01_MAT", "ENV_ASH_MaproomFlipbook01_CLR", 3.0),
    "ENV_ASH_MaproomFlipbook02_MAT": _flipbook("ENV_ASH_MaproomFlipbook02_MAT", "ENV_ASH_MaproomFlipbook02_CLR", 3.0),
    "ENV_ASH_MaproomFlipbook03_MAT": _flipbook("ENV_ASH_MaproomFlipbook03_MAT", "ENV_ASH_MaproomFlipbook03_CLR", 3.0),
    "ElectroPlane_MAT": _electro,
    "ESC_IaconColorVariant_Sky": _iacon_sky,
    "ESC_IaconColorVariant_Sun": _iacon_sun,
    "SkyHaze": _sky_haze,
    "SkyHazeOpaque": _sky_haze_opaque,
    "HmDefaultDoubleSidedMaterial": _hull,
    # The Escalation consoles (TPRO_EscalationConsoles_p) and loot pods (TPRO_NEU_StoragePod_p).
    "AP_EscalationConsolKeyboard_MAT": _console_keyboard,
    "AP_EscalationConsoleGlass_MATINST": _tinted_glass("AP_EscalationConsoleGlass_MATINST", (0.085, 0.598, 1.0)),
    "AP_EscalationConsoleDoorFlipbook_MAT": _flipbook("AP_EscalationConsoleDoorFlipbook_MAT",
                                                      "AP_EscScreenFlipbook_Door", 2.5, 0.8),
    "AP_EscalationConsoleTrap3Flipbook_MAT": _flipbook("AP_EscalationConsoleTrap3Flipbook_MAT",
                                                       "AP_EscScreenFlipbook_Trap3", 2.5, 0.8),
    "AP_EscalationConsoleUpgradeFlipbook_MAT": _flipbook("AP_EscalationConsoleUpgradeFlipbook_MAT",
                                                         "AP_EscScreenFlipbook_Upgrade", 2.5, 0.8),
    "AP_StoragePod_Glass_MAT_INST": _pod_glass,
    # Ancients
    "RS_cliffwallbigtile_CLR_Mat": _plain("RS_cliffwallbigtile_CLR_Mat", "RS_cliffwall_CLR", "RS_cliffwall_NORM",
                                          tiling=(5.0, 10.0), key="RS_cliffwall"),
    "RS_cliffwallmedtile_CLR_Mat": _plain("RS_cliffwallmedtile_CLR_Mat", "RS_cliffwall_CLR", "RS_cliffwall_NORM",
                                          tiling=(2.5, 5.0), key="RS_cliffwall"),
    "Rustwall_CLR_Mat": _plain("Rustwall_CLR_Mat", "Rustwall_CLR", "Rustwall_NORM"),
    "cagelight_CLR_Mat": _plain("cagelight_CLR_Mat", "cagelight_CLR", "cagelight_NORM", glow=_cagelight_glow),
    "fan_rustTEST_CLR_Mat": _plain("fan_rustTEST_CLR_Mat", "fan_rust2_CLR", "fan_rust2_NORM"),
    "guardianwreck_02_CLR_Mat": _plain("guardianwreck_02_CLR_Mat", "guardianwreck_CLR", "guardianwreck_NORM", gain=2.0,
                                       glow=_scaled_glow("guardianwreck_GLOW", 2.0)),
    "vint_grinder_CLR_Mat": _plain("vint_grinder_CLR_Mat", "vint_grinder_CLR", "vint_grinder_NORM"),
    "ENV_cboard_CLR_Mat": _plain("ENV_cboard_CLR_Mat", "ENV_cboard_CLR", "ENV_cboard_NORM", tiling=(2.0, 2.0)),
    "ENV_manholes_CLR_Mat": _plain("ENV_manholes_CLR_Mat", "ENV_manholes_CLR", "ENV_manholes_NORM", gain=0.5,
                                   glow=_scaled_glow("ENV_manholes_GLOW", 1.0)),
    "DebrisFakeShadow_MAT": _fake_shadow,
    "ENV_RST_ToxicSludge_MAT": _toxic_sludge,
    "ForceField_ESC_Matinst": _orange_field,
    "greytest": _grey_dome,
    # Ignition
    "ForceField_ESC_GREEN_Matinst": _green_field,
    "Sentinel_ForceField_Mat_INST": _dropship_shield,
    "ENV_manholes_MATINST": _plain("ENV_manholes_MATINST", "ENV_manholes_CLR", "ENV_manholes_NORM", gain=0.5,
                                   glow=_scaled_glow("ENV_manholes_GLOW", 1.0), key="ENV_manholes_CLR_Mat"),
    "Spacedome_ashsunset_matINST": _ash_dome,
    "RS_sky_strip_Mat": _sky_piece("RS_sky_strip_Mat", "RS_sky_strip", 30.0),
    "RS_sky_top_Mat": _sky_piece("RS_sky_top_Mat", "RS_sky_top", 30.0),
    "starmap_CLR_Mat": _sky_piece("starmap_CLR_Mat", "starmap_CLR", 50.0, "R"),
    "vint_clouds_CLR_Mat": _sky_piece("vint_clouds_CLR_Mat", "vint_clouds_CLR", 1.0),
    "cybermoon_CLR_Mat": _sky_piece("cybermoon_CLR_Mat", "cybermoon_CLR", 10.0),
}
# The materials of Ignition's far sky pieces (_sky_piece): wherever they are placed
# (the cards they are on are effect planes, FogPlane_STAT, otherwise left out), they
# are drawn by sky_card.
SKY_PIECES = ("RS_sky_strip_Mat", "RS_sky_top_Mat", "starmap_CLR_Mat", "vint_clouds_CLR_Mat", "cybermoon_CLR_Mat")
# Animated flip-book screens on Oblivion's energon signs (TextureFlipBook, which
# umodel doesn't export): drawn by their game materials (FX_MATERIALS), or left out
# where those can't be.
SKIP_MATERIALS = ("fbook_sinewave2_CLR_Mat", "fbook_sinewave6_CLR_Mat")
# Meshes drawn by a function of their own (the sky domes), for each placement:
# (materials, writer, doc, placement matrix, the play area's centre, over=the
# placement's material overrides) -> primitives.
SPECIAL_MESHES = {"RST_SkydomeTransportLevel_STAT": iacon_sky, "ENV_SkyDome_BLANK_STAT": fog_dome,
                  "ENV_SkyDome_STAT": gladiator_sky, "rs_skydome_STAT": cloud_banks}


def special_mesh(name, over):
    """The function that draws placed mesh `name` with material overrides `over`, if
    one does: a sky piece's (SKY_PIECES) material makes any mesh one."""
    if any(o in SKY_PIECES for o in over):
        return sky_card
    return SPECIAL_MESHES.get(name)


def space_dome(m, w, d, M=None, center=None, over=()):
    """SpaceDome_STAT with ENV_SpaceDome_MAT: three layers of Starfield_CLR (its own UVs
    at 1, 2 and 3 times, slowly turning and panning in the game) seen through two
    Cloud_Mask layers, x 400 x 5, plus KaonNebula_testColor06_CLR on the second UV set.
    Here: the stars baked into one texture over the first UVs (as they stand at the
    start), drawn unlit, and the nebula as a second, additive copy of the dome on the
    second UVs. Returns the dome's primitives."""
    stars = T("Starfield_CLR").astype(np.float32) / 255  # RGBA; the stars are in all four
    cloud = T("Cloud_Mask")[..., 0].astype(np.float32) / 255
    n = 2048
    u = (np.arange(n) + 0.5) / n
    U, V = np.meshgrid(u, u)

    def samp(img, uu, vv):
        h, wd = img.shape[:2]
        x = (np.floor(uu % 1.0 * wd)).astype(int) % wd
        y = (np.floor(vv % 1.0 * h)).astype(int) % h
        return img[y, x]

    s = sum(samp(stars[..., :3], U * t, V * t) for t in (1, 2, 3))
    mask = samp(cloud, U, V) * (samp(cloud, U * 0.5, V * 0.5) * 5.0)
    rgb = np.clip(s * mask[..., None] * 400.0, 0, 1)
    rgb8 = (rgb * 255).astype(np.uint8)
    star_mat = unlit(m, "SpaceDome_Stars", rgb8, (1, 1, 1), "OPAQUE", ("nofog", "nolight", "noshadow"), True)
    neb = T("KaonNebula_testColor06_CLR")[..., :3]
    neb_mat = unlit(m, "SpaceDome_Nebula", neb, (1, 1, 1), "BLEND", ("add", "nofog", "nolight"), True)
    p = d.g["meshes"][0]["primitives"][0]
    at = p["attributes"]
    pos = w.accessor(d.acc(at["POSITION"]).astype(np.float32), 34962, True)
    nrm = w.accessor(d.acc(at["NORMAL"]).astype(np.float32), 34962)
    idx = w.accessor(d.acc(p["indices"]).reshape(-1).astype(np.uint32), 34963)
    uv0 = w.accessor(d.acc(at["TEXCOORD_0"]).astype(np.float32), 34962)
    uv1 = w.accessor(d.acc(at["TEXCOORD_1"]).astype(np.float32), 34962)
    return [{"attributes": {"POSITION": pos, "NORMAL": nrm, "TEXCOORD_0": uv0}, "indices": idx,
             "material": w.material(star_mat)},
            {"attributes": {"POSITION": pos, "NORMAL": nrm, "TEXCOORD_0": uv1}, "indices": idx,
             "material": w.material(neb_mat)}]


SPECIAL_MESHES["SpaceDome_STAT"] = space_dome


# ---------------------------------------------------------------------------
# Terrain (ue3/terrain.py)
# ---------------------------------------------------------------------------

# How closely the terrain's mesh keeps to the game's, which cuts each quad 4 x 4 near
# the camera (Unreal units): each quad is cut 1, 2 or 4 times, the fewest that keep it
# within this of the game's surface.
TERRAIN_TOLERANCE = 2.0
# The terrain's first layer is drawn opaque, and the others over it, see-through by
# their weights. Their materials' "depthbias" (foc_extras) pushes the first back by
# this many of the depth buffer's steps (a fraction of a millimetre a metre away), and
# each layer over it this many nearer than the one below, so it passes the depth test
# just where the terrain is. As Bevy sorts see-through things by their distance plus
# the bias, the layers are drawn before all others (a shadow on the ground, say).
TERRAIN_DEPTH_BIAS = -1024
TERRAIN_LAYER_BIAS = 8


def _desaturate(lin, amount):
    """UE3's Desaturation: towards the colour's luminance (0.3, 0.59, 0.11) by `amount`."""
    lum = lin @ np.array([0.3, 0.59, 0.11], np.float32)
    return lin + (lum[..., None] - lin) * amount


def _plainrust3():
    # plainrust3ti3_CLR_Mat: plainrust3_CLR x 3 x (1 - plainrust3_SPEC's luminance).
    c = linear(T("plainrust3_CLR")[..., :3])
    h, wd = c.shape[:2]
    s = linear(np.asarray(Image.fromarray(T("plainrust3_SPEC")[..., :3]).resize((wd, h), Image.BILINEAR)))
    return c * 3.0 * (1.0 - _desaturate(np.clip(s, 0, 1), 1.0)), "plainrust3_NORM", (1.0, 1.0)


# The terrain layers' materials: their colour (linear light), normal map and their own
# tiling of the terrain's mapping (TextureCoordinate). Ancients'.
TERRAIN_LAYERS = {
    "ENV_RST_RustyFloorPannel2": lambda: (linear(T("ENV_RST_RustyFloor2_CLR")[..., :3]), "ENV_RST_RustyFloor2_NORM",
                                          (1.0, 1.0)),
    "plainrust3ti3_CLR_Mat": _plainrust3,
    # RS_cliffwall_CLR_Mat: RS_cliffwall_CLR, 30% desaturated.
    "RS_cliffwall_CLR_Mat": lambda: (_desaturate(linear(T("RS_cliffwall_CLR")[..., :3]), 0.3), "RS_cliffwall_NORM",
                                     (0.25, 0.5)),
}


def terrain_material(m, k, path, mapping):
    """The material of a terrain's layer k (its material `path` and the terrain's
    LocalToMapping for it, which takes a place in quads to texture coordinates): the
    layer's colour and normal map, repeated over the terrain as the game maps them;
    the first opaque, the others see-through by the vertex colours' alpha (their
    weights)."""
    name = bl.last(path) if path else None
    tile = (float(mapping[0, 0]), float(mapping[1, 1]))
    bias = TERRAIN_DEPTH_BIAS + k * TERRAIN_LAYER_BIAS
    if name in TERRAIN_LAYERS:
        lin, nrm, own = TERRAIN_LAYERS[name]()
        tile = (tile[0] * own[0], tile[1] * own[1])
        peak = max(float(lin.max()), 1.0)
        g = {"name": f"Terrain_{name}",
             "pbrMetallicRoughness": {"baseColorTexture": tiled(m, m.tex(f"Terrain_{name}_c", srgb_bytes(lin / peak)), tile),
                                      "baseColorFactor": [peak, peak, peak, 1.0], "metallicFactor": 0.0,
                                      "roughnessFactor": 0.6},
             "normalTexture": tiled(m, m.tex(nrm.replace("_NORM", "") + "_n", normal_xy(T(nrm)), "normal"), tile)}
    else:
        m.missing.add(name)
        g = {"name": f"Terrain_{name}", "pbrMetallicRoughness": {"baseColorFactor": [0.32, 0.31, 0.33, 1.0],
                                                                 "metallicFactor": 0.0, "roughnessFactor": 0.6}}
    g["extras"] = foc_extras(f"depthbias:{bias}")
    if k:
        g["alphaMode"] = "BLEND"
    return m.w.material(g)


def terrain_levels(Z, tol):
    """How many times to cut each quad (1, 2 or 4): the fewest that keep its surface
    within `tol` of Z, the game's 4 x 4 cut (heights at a quarter of a quad apart),
    whichever way its cells are split into triangles."""
    fy, fx = Z.shape
    nyq, nxq = (fy - 1) // 4, (fx - 1) // 4
    jj, ii = np.mgrid[0:fy, 0:fx]
    lev = np.full((nyq, nxq), 4)
    for L in (2, 1):
        s = 4 // L
        ci = np.minimum(ii // s, (fx - 1) // s - 1)
        cj = np.minimum(jj // s, (fy - 1) // s - 1)
        u, v = ii / s - ci, jj / s - cj
        z00, z10 = Z[cj * s, ci * s], Z[cj * s, (ci + 1) * s]
        z01, z11 = Z[(cj + 1) * s, ci * s], Z[(cj + 1) * s, (ci + 1) * s]
        a = np.where(u >= v, z00 + u * (z10 - z00) + v * (z11 - z10), z00 + v * (z01 - z00) + u * (z11 - z01))
        b = np.where(u + v <= 1, z00 + u * (z10 - z00) + v * (z01 - z00),
                     z11 + (1 - u) * (z01 - z11) + (1 - v) * (z10 - z11))
        err = np.maximum(np.abs(Z - a), np.abs(Z - b))
        worst = np.zeros((nyq, nxq))
        for dj in range(5):
            for di in range(5):
                worst = np.maximum(worst, err[dj:dj + 4 * nyq:4, di:di + 4 * nxq:4])
        lev[worst <= tol] = L
    return lev


def quad_triangles(qx, qy, L, edges):
    """The triangles of quad (qx, qy), cut L times, as their corners' places in
    quarters of a quad. Its edges (bottom, top, left, right) are cut as `edges` says,
    each as often as the coarser of the two quads they lie between, so that
    neighbours meet at the same points; the cells along a coarser edge are joined up
    to its points (each point between moves to the nearest of them)."""
    s = 4 // L

    def snap(k, e):
        r = L // e
        return r * int(math.floor(k / r + 0.5 - 1e-9))

    def corner(i, j):
        if j in (0, L):
            i = snap(i, edges[0] if j == 0 else edges[1])
        if i in (0, L):
            j = snap(j, edges[2] if i == 0 else edges[3])
        return (4 * qx + i * s, 4 * qy + j * s)

    out = []
    for j in range(L):
        for i in range(L):
            a, b, c, d = corner(i, j), corner(i + 1, j), corner(i + 1, j + 1), corner(i, j + 1)
            for tri in ((a, b, c), (a, c, d)):
                if len(set(tri)) == 3:
                    out.append(tri)
    return out


def terrain_meshes(t, m, w, light, root):
    """Adds the terrain `t` to the map: a node for each of its components (each has a
    light map of its own), its mesh the game's smooth surface (the quads cut as
    terrain_levels says) with its normals, its layers drawn one over another
    (terrain_material) and the component's light map. Returns its collision: the same
    surface (the game's terrain collides at its CollisionTesselationLevel, 4 here, so
    as smooth as it is drawn), as triangles of 9 (Bevy metres)."""
    _, _, Z, sx_, sy_ = t.surface(4, slopes=True)
    lev = terrain_levels(Z, TERRAIN_TOLERANCE)
    nyq, nxq = lev.shape
    ve = np.empty((nyq, nxq + 1), int)
    ve[:, 0], ve[:, -1] = lev[:, 0], lev[:, -1]
    ve[:, 1:-1] = np.minimum(lev[:, :-1], lev[:, 1:])
    he = np.empty((nyq + 1, nxq), int)
    he[0], he[-1] = lev[0], lev[-1]
    he[1:-1] = np.minimum(lev[:-1], lev[1:])
    layer_mats = [terrain_material(m, k, path, mapping) for k, (path, mapping) in enumerate(t.materials)]
    (lx, ly, _), (gx, gy, _) = t.location, t.scale
    n_tris = n_verts = 0
    col = []
    for comp in t.components:
        (bx, by), (cw, ch) = comp.base, comp.size
        tris = []
        for qy in range(by, by + ch):
            for qx in range(bx, bx + cw):
                tris += quad_triangles(qx, qy, int(lev[qy, qx]),
                                       (he[qy, qx], he[qy + 1, qx], ve[qy, qx], ve[qy, qx + 1]))
        keys = {}
        idx = np.array([[keys.setdefault(c, len(keys)) for c in tri] for tri in tris], np.int64)
        IJ = np.array(list(keys), np.int64)
        I, J = IJ[:, 0], IJ[:, 1]
        qx, qy = I / 4.0, J / 4.0
        z = Z[J, I]
        ue = np.stack([lx + qx * gx, ly + qy * gy, z], 1)
        pos = (ue @ bl.P.T) * 0.01
        # Normal and tangent from the surface's slopes (Unreal units an Unreal unit).
        dx, dy = sx_[J, I] / gx, sy_[J, I] / gy
        one, zero = np.ones_like(dx), np.zeros_like(dx)
        nrm = np.stack([-dx, -dy, one], 1) @ bl.P.T
        nrm /= np.linalg.norm(nrm, axis=1, keepdims=True)
        tan = np.stack([one, zero, dx], 1) @ bl.P.T
        tan /= np.linalg.norm(tan, axis=1, keepdims=True)
        bit = np.stack([zero, one, dy], 1) @ bl.P.T  # the way v grows
        sign = np.where(np.sum(np.cross(nrm, tan) * bit, 1) >= 0, 1.0, -1.0)
        # Triangles facing up.
        p0, p1, p2 = pos[idx[:, 0]], pos[idx[:, 1]], pos[idx[:, 2]]
        down = np.cross(p1 - p0, p2 - p0)[:, 1] < 0
        idx[down] = idx[down][:, [0, 2, 1]]
        # Each layer over the ones below it at alpha weight / (its weight and those below),
        # which leaves each layer weighing as the game's weight maps say.
        wts = t.weights_at(qx, qy)
        below = np.cumsum(wts, 1)
        alpha = np.where(below > 1e-6, wts / np.maximum(below, 1e-6), 0.0)
        uv1 = np.stack([(qx - bx) / cw, (qy - by) / ch], 1)
        attrs = {"POSITION": w.accessor(pos.astype(np.float32), 34962, True),
                 "NORMAL": w.accessor(nrm.astype(np.float32), 34962),
                 "TANGENT": w.accessor(np.concatenate([tan, sign[:, None]], 1).astype(np.float32), 34962),
                 "TEXCOORD_0": w.accessor(np.stack([qx, qy], 1).astype(np.float32), 34962),
                 "TEXCOORD_1": w.accessor(uv1.astype(np.float32), 34962)}
        prims = [{"attributes": attrs, "indices": w.accessor(idx.reshape(-1).astype(np.uint32), 34963),
                  "material": layer_mats[0]}]
        for k in range(1, len(layer_mats)):
            a8 = np.round(np.clip(alpha[:, k], 0, 1) * 255).astype(np.uint8)
            used = a8[idx].max(1) > 0
            if not used.any():
                continue
            rgba = np.concatenate([np.full((len(a8), 3), 255, np.uint8), a8[:, None]], 1)
            prims.append({"attributes": dict(attrs, COLOR_0=w.accessor(rgba, 34962, normalized=True)),
                          "indices": w.accessor(idx[used].reshape(-1).astype(np.uint32), 34963),
                          "material": layer_mats[k]})
        name = f"{t.name}_{bx}_{by}"
        w.g["meshes"].append({"name": name, "primitives": prims})
        node = {"name": name, "mesh": len(w.g["meshes"]) - 1}
        lm = comp.lightmap
        if lm is not None and light.ok and lm["dirmax"] in light.baked:
            # Vertex q sits on texel 4 + 4 q of the component's light map (terrain.py),
            # its first and last quads' at the rectangle's ends here.
            (su, sv), (ou, ov) = lm["coord_scale"], lm["coord_bias"]
            nu = cw * terrain.LIGHTMAP_TEXELS + 2 * terrain.LIGHTMAP_MARGIN + 1
            nv = ch * terrain.LIGHTMAP_TEXELS + 2 * terrain.LIGHTMAP_MARGIN + 1
            first = terrain.LIGHTMAP_MARGIN + 0.5
            rect = (ou + first / nu * su, ov + first / nv * sv,
                    ou + (first + cw * terrain.LIGHTMAP_TEXELS) / nu * su,
                    ov + (first + ch * terrain.LIGHTMAP_TEXELS) / nv * sv)
            node["extras"] = light.extras(lm["dirmax"], rect)
        w.g["nodes"][root]["children"].append(w.node(node))
        col.append(pos[idx].reshape(-1, 9))
        n_tris += len(idx)
        n_verts += len(IJ)
    print(f"terrain {t.name}: {t.nx} x {t.ny} heights, {len(t.components)} parts, {n_verts} vertices, "
          f"{n_tris} triangles (quads cut " + ", ".join(f"{L}: {np.mean(lev == L):.0%}" for L in (1, 2, 4)) + ")")
    return np.concatenate(col).astype(np.float32)


# ---------------------------------------------------------------------------
# The build
# ---------------------------------------------------------------------------

def export_dirs(root, sub):
    """Every `sub` folder of the umodel export (one per package umodel wrote)."""
    out = []
    for d in sorted(os.listdir(root)):
        p = os.path.join(root, d, sub)
        if os.path.isdir(p):
            out.append(p)
    return out


def ue_yaw_to_heading(yaw):
    """Unreal yaw (1/65536 turns) -> the demo's heading in degrees (about Bevy's up
    axis, 0 looking down -Z, -90 down +X)."""
    t = yaw * 2 * math.pi / 65536.0
    return math.degrees(math.atan2(-math.cos(t), -math.sin(t)))


def placements(spec, art, base, art_file, base_file):
    """What the level places: [{mesh or skel, matrix, source, package, ...}]."""
    items = []
    open_map = spec.get("open")
    for pkg, pkg_file in ((art, art_file), (base, base_file)):
        _, its = lay.extract(pkg.path if hasattr(pkg, "path") else pkg_file)
        state, hidden, deltas, trans = {}, set(), {}, {}
        if open_map:
            graph = kismet.Graph(pkg)
            state = kismet.opened_state(pkg, graph)
            hidden = kismet.hidden_at_start(pkg, graph)
            deltas = actor_deltas(pkg, state)
            trans = kismet.transformed(pkg, graph)
        names = {pkg.exports[i - 1]["name"]: i for i in level_actors(pkg)}
        kept = []
        for it in its:
            it["package"] = pkg_file
            src = names.get(it["source"])
            if src in hidden:
                continue
            if pkg_file == base_file and any(s in it["mesh"] for s in spec.get("skip_base", ())):
                continue
            if src in deltas:
                it["matrix"] = (np.array(it["matrix"]).reshape(4, 4) @ deltas[src]).ravel().tolist()
            kept.append(it)
        items += kept
        if pkg_file == base_file:
            items += furniture(pkg, pkg_file, hidden)
        if open_map:
            for it in skeletal_props(pkg, pkg_file, state, hidden, deltas, trans):
                it["mesh"] = pose_key(it)
                items.append(it)
            items += destructibles(pkg, pkg_file, hidden)
            moved = [k for k in state if not (isinstance(k, tuple) and k[0] == "animsets")]
            print(f"  {pkg_file}: {len(moved)} actors moved or posed by its Matinees, {len(hidden)} hidden at the start"
                  + (f", {sum(trans.values())} transformed" if trans else ""))
        elif pkg_file == art_file:
            for door in doors(pkg, pkg_file):
                door["mesh"] = pose_key(door)
                items.append(door)
    return items


def build(key, export_root, cooked, out):
    spec = MAPS[key]
    art_file, base_file = spec["art"] + ".xxx", spec["base"] + ".xxx"
    art_path, base_path = os.path.join(cooked, art_file), os.path.join(cooked, base_file)
    bl.configure(os.path.join(export_root, spec["art"]), None, out)
    bl.EXTRA_TX = [d for d in export_dirs(export_root, "Texture2D") if d != bl.TX]
    mesh_dirs = export_dirs(export_root, "StaticMesh3")
    skel_dirs = export_dirs(export_root, "SkeletalMesh3")
    anim_dirs = export_dirs(export_root, "AnimSet")
    art, base = Package(art_path), Package(base_path)
    art.path, base.path = art_path, base_path
    packages = {art_file: art, base_file: base}
    terrains = terrain.read_terrains(art)

    # ---- what the level places ----
    items = placements(spec, art, base, art_file, base_file)
    texsets, sections = {}, {}
    for pkg in (art, base):
        texsets.update(file_texsets(pkg))
        sections.update(section_materials(pkg))
    print(f"{spec['title']}: {len(items)} placed meshes, {len(texsets)} materials with texture sets")

    lm_dir = f"levels/{key}_lightmaps"
    terrain_lms = [c.lightmap for t in terrains for c in t.components if c.lightmap]
    lights = {art_file: bl.Lighting(art_path, "auto", lm_dir, "", terrain_lms),
              base_file: bl.Lighting(base_path, None, lm_dir, "b")}
    light = lights[art_file]
    w = Writer(image_dir=os.path.join(out, "models", key), image_uri=key)
    # Old textures out of the way first (names change with the materials).
    if os.path.isdir(w.image_dir):
        for f in os.listdir(w.image_dir):
            os.remove(os.path.join(w.image_dir, f))
    mats = MapMats(w, texsets, spec.get("aliases", {}), (art, base), cooked)
    # The effects (levelfx.py): effect meshes' materials, emitters and lens flares.
    donor_file, donor_mats = spec.get("fx_donor", (None, ()))
    lfx = levelfx.LevelFx(key, cooked, [art_file, base_file, "mp_shared.xxx", "all_shared.xxx"]
                          + ([donor_file] if donor_file else []))
    fx_mats = {}
    # (Their copies for effect meshes in the sky, seen through its fog.)
    fx_sky_mats = {}
    solid_fx = set()
    # (Light-mapped meshes with one set of UVs: their light-mapped copies, by mesh.
    # Cybertron's lit copies: their mesh and white light's texel, by placement.)
    uv0_meshes, one_uv = {}, set()
    planet_meshes = {}
    root = w.node({"name": spec["title"], "children": []}, root=True)
    col = []

    # ---- BSP ----
    model = bl.level_model(art)
    # (Not cut up: the map is lit by its light maps and Bevy's fog is per pixel.)
    groups = bl.bsp_geometry(art_path, light.nodes, light.baked if light.ok else (), model, split=False)
    by_page = {}
    for (mat, page), g in groups.items():
        pos = np.array(g["pos"], np.float32)
        col.append(pos.reshape(-1, 9))
        allv = np.concatenate([pos, np.array(g["nrm"], np.float32), np.array(g["tan"], np.float32),
                               np.array(g["uv"], np.float32)], 1)
        uniq, inv = np.unique(np.round(allv, 5), axis=0, return_inverse=True)
        uniq = uniq.astype(np.float32)
        attrs = {"POSITION": w.accessor(np.ascontiguousarray(uniq[:, 0:3]), 34962, True),
                 "NORMAL": w.accessor(np.ascontiguousarray(uniq[:, 3:6]), 34962),
                 "TANGENT": w.accessor(np.ascontiguousarray(uniq[:, 6:10]), 34962),
                 "TEXCOORD_0": w.accessor(np.ascontiguousarray(uniq[:, 10:12]), 34962)}
        if page is not None:
            attrs["TEXCOORD_1"] = w.accessor(np.ascontiguousarray(uniq[:, 12:14]), 34962)
        by_page.setdefault(page, []).append({"attributes": attrs, "indices": w.accessor(inv.reshape(-1).astype(np.uint32), 34963),
                                             "material": mats.get(mat)})
    for page, prims in by_page.items():
        w.g["meshes"].append({"name": f"bsp_{page}", "primitives": prims})
        node = {"name": f"bsp_{page}", "mesh": len(w.g["meshes"]) - 1}
        if page is not None:
            node["extras"] = light.extras(page)
        w.g["nodes"][root]["children"].append(w.node(node))
    print("bsp:", sum(len(np.array(g["pos"])) // 3 for g in groups.values()), "triangles,",
          sum(len(p) for k, p in by_page.items() if k is not None), "of", len(groups), "parts light-mapped")

    # ---- terrain ----
    for t in terrains:
        col.append(terrain_meshes(t, mats, w, light, root))

    # ---- placed meshes ----
    pts = points(base)
    conv = lambda v: [round(v[0] * 0.01, 2), round(v[2] * 0.01, 2), round(v[1] * 0.01, 2)]
    # The middle of the play area (its AI paths), where the sky is seen from.
    center = np.mean([conv(v) for v in pts["TnTacticalPathNode"]], 0)
    # (The fog the sky is seen through.)
    sky_layers = level_fog_layers((art, base))
    docs = {}
    meshes = {}
    # (The meshes that collide by their hulls.)
    hulls = {n: mesh_hull((art, base), n) for n in spec.get("hulls", ())}
    for n_, h_ in hulls.items():
        print(f"  {n_} collides by its hulls ({len(h_)})" if h_ else f"  ! {n_}: no collision hulls")
    placed = skipped = 0
    lit = {"2d": 0, "1d": 0, "none": 0}
    missing = set()
    for it in items:
        name = bl.last(it["mesh"])
        over = tuple(bl.last(m) for m in it.get("materials") or [])
        special = special_mesh(name, over)
        # (Oblivion's space dome: its game material, ENV_SpaceDome_MAT, if it compiles;
        # space_dome's stand-in if not.)
        if special is space_dome and lfx.material("ENV_SpaceDome_MAT", fog=False) is not None:
            special = None
        # (An effect mesh: drawn with its game material by levelfx.rs. One far out is
        # part of the sky, seen through its fog: SKY_FX_DISTANCE.)
        fx_mesh = any(s in name for s in FX_MESHES) and special is None
        sky_fx = fx_mesh and np.linalg.norm(
            bl.P @ np.asarray(it["matrix"], np.float64)[12:15] * 0.01 - center) > SKY_FX_DISTANCE
        if "skel" in it:
            # A skeletal prop: its mesh posed as the level leaves it.
            f = next((os.path.join(d, it["skel"] + ".gltf") for d in skel_dirs
                      if os.path.exists(os.path.join(d, it["skel"] + ".gltf"))), None)
            if not f:
                missing.add(it["skel"])
                skipped += 1
                continue
            if name not in docs:
                docs[name] = Posed(f, it.get("pose"), clip_reader(it, anim_dirs, packages))
                if it.get("pose") is not None and not docs[name].posed:
                    missing.add(f"{it.get('animset')} (its clips: {name} is in its bind pose)")
        else:
            f = next((os.path.join(d, name + ".gltf") for d in mesh_dirs if os.path.exists(os.path.join(d, name + ".gltf"))), None)
            if not f:
                missing.add(name)
                skipped += 1
                continue
            if name not in docs:
                docs[name] = Doc(f)
        d = docs[name]
        # (A mesh drawn by a function of its own, or in the sky, is made for each
        # placement.)
        key_ = (name, over) if special is None and not sky_fx else (name, over, tuple(it["matrix"]))
        if key_ not in meshes:
            if special is not None:
                M = np.array(it["matrix"], np.float64).reshape(4, 4)
                ps = [dict(q, _range=(0, 0)) for q in special(mats, w, d, M, center, over=over)]
                first = sum(d.g["accessors"][p["attributes"]["POSITION"]]["count"] for p in d.g["meshes"][0]["primitives"])
            else:
                ps = []
                first = 0
                own = sections.get(name, [])
                for pi, p in enumerate(d.g["meshes"][0]["primitives"]):
                    mname = d.g["materials"][p["material"]]["name"] if "material" in p else None
                    if mname and mname.startswith("dummy_material_") and "material" in p and p["material"] < len(own):
                        mname = own[p["material"]] or mname
                    if pi < len(over) and over[pi]:
                        mname = over[pi]
                    at = p["attributes"]
                    nverts = d.g["accessors"][at["POSITION"]]["count"]
                    start, first = first, first + nverts
                    if nverts == 0 or d.g["accessors"][p["indices"]]["count"] == 0:
                        continue
                    fi = lfx.material(mname, fog=mname not in FX_BACKDROP) if mname in FX_MATERIALS else None
                    if mname in SKIP_MATERIALS and fi is None:
                        continue
                    if fx_mesh or fi is not None:
                        if fi is None:
                            fi = lfx.material(mname)
                        if fi is None:
                            continue
                        if mname in FX_GLASS:
                            solid_fx.add(fi)
                        vals = levelfx.fx_mesh_attributes(d, at, nverts, lfx.mesh_has_colours(name))
                        if sky_fx:
                            # (Seen through the sky's fog, worked out at each vertex as
                            # the game does: its colours, levelfx.rs's FX_SKYFOG.)
                            M = np.array(it["matrix"], np.float64).reshape(4, 4)
                            world = vals[0].astype(np.float64) @ (bl.P @ M[:3, :3].T @ bl.P).T + bl.P @ M[3, :3] * 0.01
                            through, light = ray_fog(world, center, sky_layers)
                            vals = (*vals[:5], np.concatenate([light, through[:, None]], 1).astype(np.float32))
                            if fi not in fx_sky_mats:
                                fx_sky_mats[fi] = w.material(levelfx.fx_gltf_material(key, fi, mname, sky=True))
                        elif fi not in fx_mats:
                            fx_mats[fi] = w.material(levelfx.fx_gltf_material(key, fi, mname))
                        attrs = {k: w.accessor(v, 34962, k == "POSITION") for k, v in
                                 zip(("POSITION", "NORMAL", "TANGENT", "TEXCOORD_0", "TEXCOORD_1", "COLOR_0"), vals)}
                        ps.append({"attributes": attrs,
                                   "indices": w.accessor(d.acc(p["indices"]).reshape(-1).astype(np.uint32), 34963),
                                   "material": (fx_sky_mats if sky_fx else fx_mats)[fi], "_range": (start, nverts)})
                        continue
                    attrs = {"POSITION": w.accessor(d.acc(at["POSITION"]).astype(np.float32), 34962, True),
                             "NORMAL": w.accessor(d.acc(at["NORMAL"]).astype(np.float32), 34962),
                             "TEXCOORD_0": w.accessor(d.acc(at["TEXCOORD_0"]).astype(np.float32), 34962)}
                    if "TANGENT" in at:
                        attrs["TANGENT"] = w.accessor(d.acc(at["TANGENT"]).astype(np.float32), 34962)
                    if "TEXCOORD_1" in at:
                        attrs["TEXCOORD_1"] = w.accessor(d.acc(at["TEXCOORD_1"]).astype(np.float32), 34962)
                    ps.append({"attributes": attrs, "indices": w.accessor(d.acc(p["indices"]).reshape(-1).astype(np.uint32), 34963),
                               "material": mats.get(mname), "_range": (start, nverts)})
            if ps:
                w.g["meshes"].append({"name": name, "primitives": [{k: v for k, v in q.items() if k != "_range"} for q in ps]})
                meshes[key_] = (len(w.g["meshes"]) - 1, ps, first)
            else:
                meshes[key_] = None
        if meshes[key_] is None:
            skipped += 1
            continue
        mesh_index, ps, nverts = meshes[key_]
        M = np.array(it["matrix"], np.float64).reshape(4, 4)
        A = bl.P @ M[:3, :3].T @ bl.P
        t = bl.P @ M[3, :3] * 0.01
        T_ = np.eye(4); T_[:3, :3] = A; T_[:3, 3] = t
        node = {"name": name, "mesh": mesh_index, "matrix": [float(f"{x:.7g}") for x in T_.T.ravel()]}

        # Its baked lighting (from its own package's light maps).
        L = lights.get(it.get("package"))
        lm = L.meshes.get(it.get("component")) if L is not None and L.ok and "skel" not in it else None
        kind = "none"
        unlit_mesh = special is not None or all(
            "KHR_materials_unlit" in w.g["materials"][q["material"]].get("extensions", {}) for q in ps)
        planet = [q for q in ps if q["material"] is not None
                  and w.g["materials"][q["material"]].get("name") == "ctron_CLR_Mat"]
        if unlit_mesh:
            pass
        elif planet and lm is not None and lm["kind"] == "2d" and lm["nac"] in L.pages and lm["dirmax"] in L.pages:
            # Cybertron: its light map baked into its material (_planet), lit by white.
            pkey = (key_, lm["dirmax"], lm["coord_bias"])
            if pkey not in planet_meshes:
                mat = w.material(_planet(mats, L, lm, d, T_, center))
                prims = [{"attributes": dict(q["attributes"], TEXCOORD_1=q["attributes"].get(
                             "TEXCOORD_1", q["attributes"]["TEXCOORD_0"])),
                          "indices": q["indices"],
                          "material": mat if any(q is p for p in planet) else q["material"]} for q in ps]
                w.g["meshes"].append({"name": name + "_lit", "primitives": prims})
                u, v = L.vertex_texel((1.0, 1.0, 1.0))
                planet_meshes[pkey] = (len(w.g["meshes"]) - 1, (u, v, u, v))
            node["mesh"], rect = planet_meshes[pkey]
            node["extras"] = L.extras("vertex", rect)
            kind = "2d"
        elif lm is not None and lm["kind"] == "2d" and lm["dirmax"] in L.baked:
            (sx, sy), (bx, by) = lm["coord_scale"], lm["coord_bias"]
            if not all("TEXCOORD_1" in q["attributes"] for q in ps):
                # (A mesh with one set of UVs has its light map laid on those: UE3 takes
                # the light map's UV set as the last the mesh has. Oblivion's Cybertron.)
                if mesh_index not in uv0_meshes:
                    prims = [{"attributes": dict(q["attributes"], TEXCOORD_1=q["attributes"].get(
                                 "TEXCOORD_1", q["attributes"]["TEXCOORD_0"])),
                              "indices": q["indices"], "material": q["material"]} for q in ps]
                    w.g["meshes"].append({"name": name + "_uv0lit", "primitives": prims})
                    uv0_meshes[mesh_index] = len(w.g["meshes"]) - 1
                node["mesh"] = uv0_meshes[mesh_index]
                one_uv.add(name)
            node["extras"] = L.extras(lm["dirmax"], (bx, by, bx + sx, by + sy))
            kind = "2d"
        elif lm is not None and lm["kind"] == "1d" and len(lm["samples"]) == nverts:
            irr = lighting.vertex_irradiance(lm)
            peak = np.maximum(irr.max(0), 1e-4)
            rgba = np.concatenate([irr / peak, np.ones((len(irr), 1), np.float32)], 1)
            rgba = np.round(np.clip(rgba, 0, 1) * 255).astype(np.uint8)
            u, v = L.vertex_texel(peak)
            prims = []
            for q in ps:
                start, n = q["_range"]
                attrs = dict(q["attributes"])
                attrs["COLOR_0"] = w.accessor(rgba[start:start + n], 34962, normalized=True)
                if "TEXCOORD_1" not in attrs:
                    attrs["TEXCOORD_1"] = bl.zero_uv(w, n)
                prims.append({"attributes": attrs, "indices": q["indices"], "material": q["material"]})
            w.g["meshes"].append({"name": name + "_vertexlit", "primitives": prims})
            node["mesh"] = len(w.g["meshes"]) - 1
            node["extras"] = L.extras("vertex", (u, v, u, v))
            kind = "1d"
        lit[kind] += 1
        w.g["nodes"][root]["children"].append(w.node(node))
        placed += 1
        # (A window's glass, drawn by its effect material, still stands in the way.)
        glass = any(q["material"] in {fx_mats[f] for f in solid_fx if f in fx_mats} for q in ps)
        hull = hulls.get(name) if not it.get("destructible") else None
        if hull and not any(s in name for s in SKIP_COLLIDE):
            col.append(place_hull(hull, A, t))
        elif not any(s in name for s in SKIP_COLLIDE) and (not unlit_mesh or glass):
            cm = bl.collision_mesh(name, d, np.linalg.norm(A, axis=0).max())
            if cm is not None:
                v = cm[0] @ A.T + t
                # (A mirrored placement's triangles turned round, their normals by the
                # order of their corners facing out as the game draws them: the
                # movement tells a rock's underside from its top by them. col_winding.py
                # does the same to a file made before.)
                faces = cm[1][:, [0, 2, 1]] if np.linalg.det(A) < 0 else cm[1]
                col.append(v[faces.reshape(-1)].astype(np.float32).reshape(-1, 9))
    print(f"meshes: {placed} placed, {skipped} skipped, {len(meshes)} unique")
    if one_uv:
        print(f"  light maps on their only UVs: {sorted(one_uv)}")
    print(f"  lighting: {lit['2d']} light-mapped, {lit['1d']} vertex-lit, {lit['none']} without baked light")
    if missing:
        print("  ! not in the export:", sorted(missing))
    if mats.missing:
        print("  ! materials without textures:", sorted(m for m in mats.missing if m))

    os.makedirs(os.path.join(out, "models"), exist_ok=True)
    os.makedirs(os.path.join(out, "levels"), exist_ok=True)
    size = w.save(os.path.join(out, "models", f"{key}.glb"))
    tex_size = sum(os.path.getsize(os.path.join(w.image_dir, f)) for f in os.listdir(w.image_dir))
    print(f"wrote models/{key}.glb ({size / 1e6:.1f} MB) and models/{key}/ ({len(os.listdir(w.image_dir))} images, "
          f"{tex_size / 1e6:.1f} MB)")

    # ---- collision ----
    tris = np.concatenate(col).astype(np.float32)
    a, b, c = tris[:, 0:3], tris[:, 3:6], tris[:, 6:9]
    tris = tris[np.linalg.norm(np.cross(b - a, c - a), axis=1) > 1e-6]
    # (What lies 25 m or more beyond the AI's paths and spawn points is left out.)
    tris = near_play(tris, pts)
    # The blocking volumes, of both packages: walls for movement only (shots and the
    # camera pass); the players-only ones (the enemies' spawn walls) apart.
    vol, pvol = blocking_triangles(((art, art_path), (base, base_path)))
    # A lid where the map has one ("lid": metres), for the players' movement only
    # (the game has none: the AI's pawns pass it as they would the open sky, the
    # Leapers' leaps rising over it): two triangles over all of the level's BSP at
    # that height.
    if spec.get("lid") is not None:
        y = float(spec["lid"])
        bsp_pos = np.concatenate([np.array(g["pos"], np.float32) for g in groups.values()])
        (x0, z0), (x1, z1) = bsp_pos[:, [0, 2]].min(0), bsp_pos[:, [0, 2]].max(0)
        lid = np.array([[x0, y, z0, x1, y, z0, x1, y, z1], [x0, y, z0, x1, y, z1, x0, y, z1]], np.float32)
        pvol = np.concatenate([pvol.reshape(-1, 9), lid]) if len(pvol) else lid
        print(f"lid at {y} m over x {x0:.0f}..{x1:.0f}, z {z0:.0f}..{z1:.0f}")
    pvol = with_blockers(spec, pvol)
    size = write_collision(os.path.join(out, "levels", f"{key}_col.bin"), tris, vol, pvol)
    print(f"wrote levels/{key}_col.bin ({len(tris)} triangles, {len(vol)} more for movement only, "
          f"{len(pvol)} for the players' only, {size / 1e6:.1f} MB)")

    write_points(spec, key, base, pts, out)

    # ---- effects ----
    lfx.scan(art)
    lfx.scan(base)
    for name in donor_mats:
        if lfx.material(name) is None:
            print(f"  ! effect material {name} can't be drawn")
    lfx.write(out)

    n = 0
    for L in lights.values():
        if L.ok:
            n += L.write_pages()
    print(f"wrote {lm_dir}/ ({n} pages)")
    # (The fog: both packages' layers, as they reach the play area, at its AI paths' middle height.)
    floor = float(np.median([v[2] for v in pts["TnTacticalPathNode"]]))
    bl.write_lights(art, key, f"{spec['title']} lighting, read from {art_file} by tools/build_map.py", (base,), floor,
                    cooked=cooked)


def blocking_triangles(packages):
    """The blocking volumes of these (package, path) pairs as triangles (Bevy
    metres, float32, 9 a row): the BlockingVolumes', which stop all movement, and
    the HmPlayerOnlyBlockingVolumes', which stop only the players' (their
    IgnoreBlockingBy lets every other pawn through, the AI's: the enemies' spawn
    walls)."""
    def gather(classes):
        parts = [volume_triangles(pkg, path, classes) for pkg, path in packages]
        parts = [v for v in parts if len(v)]
        if not parts:
            return np.zeros((0, 9), np.float32)
        v = np.concatenate(parts).reshape(-1, 3, 3)
        return (v[:, :, [0, 2, 1]] * 0.01).reshape(-1, 9).astype(np.float32)
    return gather(("BlockingVolume",)), gather(("HmPlayerOnlyBlockingVolume",))


def box_triangles(lo, hi):
    """An axis-aligned box's six faces as 12 triangles (Bevy metres, float32, 9 a
    row), from its least and greatest corners."""
    (x0, y0, z0), (x1, y1, z1) = lo, hi
    c = [(x, y, z) for x in (x0, x1) for y in (y0, y1) for z in (z0, z1)]
    # (Corners by bit: 4 = x1, 2 = y1, 1 = z1; each face two triangles.)
    faces = ((0, 1, 3, 2), (4, 6, 7, 5), (0, 4, 5, 1), (2, 3, 7, 6), (0, 2, 6, 4), (1, 5, 7, 3))
    tris = [c[a] + c[b] + c[d] for f in faces for a, b, d in ((f[0], f[1], f[2]), (f[0], f[2], f[3]))]
    return np.array(tris, np.float32)


def with_blockers(spec, pvol):
    """The players-only triangles with the map's own blockers added (MAPS'
    "blockers": boxes the demo puts where the game lets a player go that he
    shouldn't)."""
    for lo, hi in spec.get("blockers", ()):
        box = box_triangles(lo, hi)
        pvol = np.concatenate([pvol.reshape(-1, 9), box]) if len(pvol) else box
        print(f"players' blocker over x {lo[0]}..{hi[0]}, y {lo[1]}..{hi[1]}, z {lo[2]}..{hi[2]} m")
    return pvol


def write_collision(path, tris, movement_only, players_only=None):
    """The collision file, in the demo's compact format (collision.rs): "FOCC", version
    3, the origin (3 x f32) and step (f32, metres) the corners are counted in, then
    the triangle count and their corners as 9 x i16 each, then the same for the
    triangles that only stop movement (the blocking volumes), and for those that
    only stop the players' (the players-only blocking volumes). Returns its size."""
    players_only = np.zeros((0, 9), np.float32) if players_only is None else players_only
    sets = [t.reshape(-1, 3) for t in (tris, movement_only, players_only) if len(t)]
    allv = np.concatenate(sets)
    lo, hi = allv.min(0), allv.max(0)
    origin = ((lo + hi) / 2).astype(np.float32)
    step = 0.01
    while (np.abs(allv - origin).max() / step) > 32000:
        step *= 2
    def q(t):
        return np.round((t.reshape(-1, 3) - origin) / step).astype("<i2").reshape(-1, 9)
    with open(path, "wb") as fh:
        fh.write(b"FOCC")
        fh.write(struct.pack("<I3ff", 3, *origin.tolist(), step))
        for t in (tris, movement_only, players_only):
            fh.write(struct.pack("<I", len(t)))
            if len(t):
                fh.write(q(t).tobytes())
    return os.path.getsize(path)


def read_collision(path):
    """A collision file's triangle sets (version 2: solid and movement only;
    version 3: and players only), Bevy metres, float32, 9 a row."""
    raw = open(path, "rb").read()
    assert raw[:4] == b"FOCC", path
    version = struct.unpack_from("<I", raw, 4)[0]
    origin = np.array(struct.unpack_from("<3f", raw, 8), np.float32)
    step = struct.unpack_from("<f", raw, 20)[0]
    at, out = 24, []
    for _ in range(3 if version >= 3 else 2):
        n = struct.unpack_from("<I", raw, at)[0]
        at += 4
        q = np.frombuffer(raw, "<i2", n * 9, at).astype(np.float32).reshape(-1, 3)
        at += n * 18
        out.append((origin + q * step).reshape(-1, 9).astype(np.float32))
    if len(out) == 2:
        out.append(np.zeros((0, 9), np.float32))
    return out


def volumes_only(key, cooked, out):
    """Rewrites levels/<map>_col.bin's blocking volumes (no map build): its solid
    triangles as they are (doors and all, as build_doors.py left them), its
    BlockingVolumes' for movement only, its HmPlayerOnlyBlockingVolumes', its lid and
    its blockers for the players' only."""
    spec = MAPS[key]
    path = os.path.join(out, "levels", f"{key}_col.bin")
    tris, old_vol, old_pvol = read_collision(path)
    pk = []
    for name in (spec["art"], spec["base"]):
        p = os.path.join(cooked, name + ".xxx")
        pkg = Package(p)
        pkg.path = p
        pk.append((pkg, p))
    vol, pvol = blocking_triangles(pk)
    if spec.get("lid") is not None:
        # (The lid: the old file's blocking triangles flat at its height, wide, for
        # the players' movement only.)
        y = float(spec["lid"])
        old = np.concatenate([old_vol.reshape(-1, 9), old_pvol.reshape(-1, 9)])
        t = old.reshape(-1, 3, 3)
        flat = (np.abs(t[:, :, 1] - y) < 0.05).all(1) & ((t[:, :, 0].max(1) - t[:, :, 0].min(1)) > 100)
        lid = old[flat]
        print(f"lid: {len(lid)} triangles at {y} m")
        pvol = np.concatenate([pvol.reshape(-1, 9), lid]) if len(pvol) else lid
    pvol = with_blockers(spec, pvol)
    size = write_collision(path, tris, vol, pvol)
    print(f"wrote levels/{key}_col.bin ({len(tris)} triangles, {len(vol)} for movement only (was "
          f"{len(old_vol) + len(old_pvol)}), {len(pvol)} for the players' only, {size / 1e6:.1f} MB)")


def hulls_only(key, cooked, out):
    """Rewrites levels/<map>_col.bin's triangles of MAPS' "hulls" meshes (no map
    build): each placed in the map's model (its scene's nodes: the props the
    purchases bring down, which build_doors.py takes out of it, are left as they
    are) loses the triangles of its own shape (build_level.collision_mesh, found
    again in the file's quantization, as remove_console.py finds the console's) and
    gets its hulls', as a full build makes them. Run again, it changes nothing."""
    import remove_console as rc
    spec = MAPS[key]
    names = spec.get("hulls", ())
    path = os.path.join(out, "levels", f"{key}_col.bin")
    glb = os.path.join(out, "models", f"{key}.glb")
    if not names:
        print(f"{key}: no meshes collide by their hulls")
        return
    pk = []
    for name in (spec["art"], spec["base"]):
        p_ = os.path.join(cooked, name + ".xxx")
        pkg = Package(p_)
        pkg.path = p_
        pk.append(pkg)
    hulls = {n: mesh_hull(pk, n) for n in names}
    doc = Doc(glb)
    g, _rest = rc.read_glb(glb)
    shown, todo = set(), [i for sc in g.get("scenes", []) for i in sc.get("nodes", [])]
    while todo:
        i = todo.pop()
        if i not in shown:
            shown.add(i)
            todo += g["nodes"][i].get("children", [])
    origin, step, tris, vol = rc.read_col(path)
    keys = rc.corner_sets(tris)
    index = {}
    for j, k in enumerate(keys):
        index.setdefault(k, []).append(j)

    def quantized(t):
        return np.round((t.reshape(-1, 3) - origin) / step).astype(np.int64).reshape(-1, 9)

    def found(mine):
        """The file's triangles that are these (a corner a step off still matches)."""
        if len(mine) == 0:
            return set()
        want = rc.corner_sets(quantized(mine))
        hits = {j for w in want for j in index.get(w, [])}
        missed = [np.array(w).reshape(3, 3) for w in want if w not in index]
        if missed:
            lo = np.min([w.min(0) for w in missed], 0) - 2
            hi = np.max([w.max(0) for w in missed], 0) + 2
            c = tris.reshape(-1, 3, 3)
            for j in np.nonzero(((c.min(1) >= lo) & (c.max(1) <= hi)).all(1))[0]:
                kk = np.array(keys[j]).reshape(3, 3)
                if any(np.abs(kk - w).max() <= 1 for w in missed):
                    hits.add(int(j))
        return hits

    drop, add = set(), []
    for i in sorted(shown):
        n = g["nodes"][i]
        name = n.get("name", "")
        if not hulls.get(name) or "mesh" not in n:
            continue
        T = np.array(n["matrix"], np.float64).reshape(4, 4).T
        A, t = T[:3, :3], T[:3, 3]
        own = np.zeros((0, 9), np.float32)
        cm = bl.collision_mesh(name, rc.MeshDoc(doc, n["mesh"]), np.linalg.norm(A, axis=0).max())
        if cm is not None:
            v = cm[0] @ A.T + t
            faces = cm[1][:, [0, 2, 1]] if np.linalg.det(A) < 0 else cm[1]
            own = v[faces.reshape(-1)].astype(np.float32).reshape(-1, 9)
        hull = place_hull(hulls[name], A, t)
        mine, theirs = found(own), found(hull)
        drop |= mine | theirs
        add.append(hull)
        print(f"  {name} at {np.round(t, 2).tolist()}: {len(mine)} of its shape's {len(own)} triangles out, "
              f"{len(hull)} of its hulls in" + (" (again)" if theirs else ""))
    base = pk[1]
    hull_tris = near_play(np.concatenate(add), points(base)) if add else np.zeros((0, 9), np.float32)
    keep = np.array([j not in drop for j in range(len(tris))], bool)
    new = np.concatenate([tris[keep], quantized(hull_tris).astype(np.int16)])
    rc.write_col(path, origin, step, new, vol)
    print(f"wrote levels/{key}_col.bin ({len(new)} triangles: {len(drop)} out, {len(hull_tris)} in)")


# The purchase points' kinds (TnPickupFactoryPurchasable's PresetType) the points
# file keeps, by the key points() files them under.
SHOP_PRESETS = {"PPT_Health": "HealthStation", "PPT_Ammo": "AmmoStation", "PPT_HeavyAmmo": "HeavyAmmoStation",
                "PPT_Weapon": "WeaponStation", "PPT_HeavyWeapon": "WeaponStation",
                "PPT_UtilityEquipment": "EquipmentStation", "PPT_AssaultEquipment": "EquipmentStation"}


# Purchase points a map's Kismet hides as gameplay starts (SeqEvent_GameplayStarted
# -> SeqAct_ToggleHidden Hide), which the game never shows: Oblivion's Dimensional
# Decimator. (Hiding leaves its touch cylinder, but nothing is there to buy.)
HIDDEN_FACTORIES = {"ESC_Ark_BASE_m": {"TnPickupFactoryPurchasable_7754"}}


def points(pkg):
    """Player start ([x, y, z, yaw]), enemy spawn points, AI path nodes, and the
    purchase points (TnPickupFactoryPurchasable) of a BASE package (Unreal units):
    health, ammo and heavy ammo stations ([x, y, z, yaw, cost, respawn seconds]:
    PPT_Health, PPT_Ammo's TnAmmoCratePickup, PPT_HeavyAmmo's
    TnAmmoCratePickupHeavy), weapon stations ((weapon class, [x, y, z, yaw, cost,
    respawn]): PPT_Weapon and PPT_HeavyWeapon, at the factory, which stands 2 m over
    its base, HasMovedWeaponPickup) and equipment stations ((pickup class, [...]):
    PPT_UtilityEquipment and PPT_AssaultEquipment)."""
    out = {"TnTeamPlayerStart": [], "TnSurvivalSpawnPoint": [], "TnTacticalPathNode": []}
    out.update({k: [] for k in SHOP_PRESETS.values()})
    # (A factory that doesn't set its RespawnTime has its class's.)
    respawn = 10.0
    for e in pkg.exports:
        if e["name"] == "Default__TnPickupFactoryPurchasable":
            respawn = actor_props(pkg, e).get("RespawnTime", respawn)
    for e in pkg.exports:
        c = pkg.class_name(e)
        if c not in ("TnTeamPlayerStart", "TnSurvivalSpawnPoint", "TnTacticalPathNode", "TnPickupFactoryPurchasable"):
            continue
        pr = actor_props(pkg, e)
        if "Location" not in pr:
            continue
        loc = [round(v) for v in vec3(pr, "Location")]
        if c == "TnPickupFactoryPurchasable":
            key = SHOP_PRESETS.get(pr.get("PresetType"))
            hidden = HIDDEN_FACTORIES.get(os.path.basename(getattr(pkg, "path", ""))[:-4], ())
            if key is None or e["name"] in hidden:
                continue
            row = loc + [rot3(pr)[1], pr.get("Cost", 100), pr.get("RespawnTime", respawn)]
            if key in ("WeaponStation", "EquipmentStation"):
                inv = obj_name(pkg, pr.get("InventoryType"))
                if not inv:
                    continue
                out[key].append((inv.split(".")[-1], row))
            else:
                out[key].append(row)
        elif c == "TnTeamPlayerStart":
            out[c].append(loc + [rot3(pr)[1]])
        else:
            out[c].append(loc)
    return out


def write_points(spec, key, base, pts, out):
    """levels/<map>_points.txt: the start, the level camera, the height the map takes
    what falls out of it ("killz y", metres: kill_z), the enemy spawns, the AI path
    nodes and the purchase points (shop_lines)."""
    conv = lambda v: [round(v[0] * 0.01, 2), round(v[2] * 0.01, 2), round(v[1] * 0.01, 2)]
    start = pts["TnTeamPlayerStart"][0]
    os.makedirs(os.path.join(out, "levels"), exist_ok=True)
    with open(os.path.join(out, "levels", f"{key}_points.txt"), "w") as fh:
        fh.write(f"# {spec['title']} spawn data in Bevy coordinates (metres). kind x y z\n")
        fh.write("start %.2f %.2f %.2f %.1f\n" % (*conv(start[:3]), ue_yaw_to_heading(start[3])))
        cam = level_camera(base)
        if cam:
            fh.write(camera_line(cam))
        kill = kill_z(base)
        if kill is not None:
            fh.write("killz %.2f\n" % (kill * 0.01))
        for v in pts["TnSurvivalSpawnPoint"]:
            fh.write("enemy %.2f %.2f %.2f\n" % tuple(conv(v)))
        for v in pts["TnTacticalPathNode"]:
            fh.write("nav %.2f %.2f %.2f\n" % tuple(conv(v)))
        for line in shop_lines(pts):
            fh.write(line)
    print(f"wrote levels/{key}_points.txt ({len(pts['TnSurvivalSpawnPoint'])} enemy spawns, "
          f"{len(pts['TnTacticalPathNode'])} path nodes, {len(pts['HealthStation'])} health, "
          f"{len(pts['AmmoStation'])} ammo, {len(pts['HeavyAmmoStation'])} heavy ammo, "
          f"{len(pts['WeaponStation'])} weapon and {len(pts['EquipmentStation'])} equipment stations)")


def level_camera(pkg):
    """The camera the players see the map through before they come into the match
    (on the Choose Character screen and through the countdown): the CameraActor
    the level's TnWorldInfo names as its LevelCamera (TnSpectateTargetChooser.
    GetLevelCamera). Its Location (Unreal units), Rotation's pitch and yaw (1/65536
    turns) and FOVAngle (degrees across, a CameraActor's 90 unless the level sets
    it), or None."""
    for e in pkg.exports:
        if pkg.class_name(e) != "TnWorldInfo":
            continue
        ref = actor_props(pkg, e).get("LevelCamera")
        if not (isinstance(ref, tuple) and ref[0] == "obj" and ref[1] > 0):
            return None
        pr = actor_props(pkg, pkg.exports[ref[1] - 1])
        if "Location" not in pr:
            return None
        pitch, yaw, _ = rot3(pr)
        return list(vec3(pr, "Location")) + [pitch, yaw, pr.get("FOVAngle", 90.0)]
    return None


def kill_z(pkg):
    """The height (Unreal units) below which the game takes an actor out of the map
    (Actor.FellOutOfWorld): the level's TnWorldInfo's KillZ, or None where it keeps
    WorldInfo's default (Oblivion's), which no map reaches. Ancients' is -1024, 10 m
    under its lowest floor; Downfall's and Ignition's -2048."""
    for e in pkg.exports:
        if pkg.class_name(e) == "TnWorldInfo":
            v = actor_props(pkg, e).get("KillZ")
            return float(v) if isinstance(v, (int, float)) else None
    return None


def camera_line(cam):
    """The points file's level camera: "camera x y z heading pitch fov" (metres; the
    way it looks as a heading, as the start's, and its pitch, up positive, in
    degrees; its field of view across, in degrees)."""
    x, y, z, pitch, yaw, fov = cam
    pitch = (pitch + 32768) % 65536 - 32768
    return "camera %.2f %.2f %.2f %.2f %.2f %g\n" % (
        x * 0.01, z * 0.01, y * 0.01, ue_yaw_to_heading(yaw), pitch * 360.0 / 65536.0, fov)


def shop_lines(pts):
    """The points file's purchase points (metres, degrees about the up axis, energon,
    seconds): "health x y z yaw cost respawn", "ammo ...", "heavyammo ...",
    "weapon <class> x y z yaw cost respawn" (the class without TnWeapon: a weapon
    station's factory, 2 m over its base; its price is the weapon's own in the
    game, TnPickupFactoryPurchasable.GetCost, so the cost is the factory's unused
    one) and "equipment <class> x y z yaw cost respawn" (without
    TnEquipmentPickup)."""
    conv = lambda v: [round(v[0] * 0.01, 2), round(v[2] * 0.01, 2), round(v[1] * 0.01, 2)]
    row = lambda v: "%.2f %.2f %.2f %.1f %d %g" % (*conv(v), -v[3] * 360.0 / 65536.0 + 0.0, v[4], v[5])
    lines = []
    for kind, key in (("health", "HealthStation"), ("ammo", "AmmoStation"), ("heavyammo", "HeavyAmmoStation")):
        lines += [f"{kind} {row(v)}\n" for v in pts[key]]
    for kind, key, prefix in (("weapon", "WeaponStation", "TnWeapon"),
                              ("equipment", "EquipmentStation", "TnEquipmentPickup")):
        lines += [f"{kind} {cls[len(prefix):] if cls.startswith(prefix) else cls} {row(v)}\n"
                  for cls, v in pts[key]]
    return lines


def points_only(key, cooked, out):
    """Writes levels/<map>_points.txt alone, from the BASE package (no map build)."""
    spec = MAPS[key]
    base_path = os.path.join(cooked, spec["base"] + ".xxx")
    base = Package(base_path)
    base.path = base_path
    write_points(spec, key, base, points(base), out)


if __name__ == "__main__":
    if len(sys.argv) == 5 and sys.argv[1] == "--points" and sys.argv[2] in MAPS:
        points_only(*sys.argv[2:5])
        sys.exit(0)
    if len(sys.argv) == 5 and sys.argv[1] == "--volumes" and sys.argv[2] in MAPS:
        volumes_only(*sys.argv[2:5])
        sys.exit(0)
    if len(sys.argv) == 5 and sys.argv[1] == "--hulls" and sys.argv[2] in MAPS:
        hulls_only(*sys.argv[2:5])
        sys.exit(0)
    if len(sys.argv) != 5 or sys.argv[1] not in MAPS:
        print(__doc__)
        sys.exit(1)
    build(*sys.argv[1:5])
