"""Build Oblivion's exploding barrel (src/barrels.rs) as the game's map has it.

ESC_Ark_ART_m.xxx places one TnStaticDestructibleActor near the start
(TnStaticDestructibleActor_4264, at Unreal (-16, -4720, -16)), its destructible
ENV_IAC_ExplodingBarrel_p's DES_AutobotExplodingBarrel_DSYS: one piece, its states
drawing its meshes (HmStaticMeshDestructionEffectFactory, each component at Scale
0.7) and setting off its effects:

  * standing (AutobotExplodingBarrel_State1, InitialHealth 25): AutobotExplodingBarrel_STAT
    in AutobotExplodingBarrel_MATINST (its lights glowing red: Emissive_Color
    0.41 0 0 at EmissiveIntensity 5.48; its Emissive_Pulse_Rate is unused, its
    shaders compiled without the pulse) with the outline round it (OutlineColor
    1 0 0 at OutlineIntensity 1, as the pickups' cyan one), targetable, its hum
    played as it comes in (EMIT_AMB_MACH_OSC_07);
  * blown up (AutobotExplodingBarrel_State2, which every kind of damage brings it to
    once its health is gone: its links' triggers; the Sys Corruptor's would bring
    it to a corrupted state the demo hasn't the gun for): LPdamage2_STAT, the wreck,
    in AutobotExplodingBarrel_noGlow_MATINST, the blast (Exp_GenericLvl_MED_FX,
    DEST_ENERGON_TANK_01_EXPLO, HurtRadius: 150 of TnDamageTypeBruticusFire within
    15 m, all of it to all, momentum 125000), its debris
    (HmDebrisDestructionEffectFactory: Debris1_STAT twice, Debris2_STAT twice,
    Debris3_STAT three times, Debris4_STAT trailing Wounded_Soldier_FX, six of
    DES_ASH_DestructCoverBlocks_p's CoverBLockCLarge_debris_02_STAT trailing
    Fiery_Smokey_GibsTrail_FX) and its fire (Fire_ExpBarrel_FX, ended with the
    state: TerminateEffectOnExitState) for the state's 10 s
    (AutomaticTransitionTime), then the wreck alone, for good;
  * damage to it as its blueprint has it: an AI's at half (AIDamageModifier), a
    blast's at a quarter when its middle is more than 1 m off
    (RadiusDamageModifier, MinRadiusDamageDistanceForModifier), and its
    DamageModifiers' (Corrosion's 5 times, the Sys Corruptor's 25).

This writes models/props/exploding_barrel.glb (its textures in
models/props/exploding_barrel/, KTX2 as the maps' are): a node for each state's
mesh by its role (barrel, barrel_outline, wreck), in umodel's glTF axes and metres,
at its own size (barrels.rs draws them at the game's 0.7), with the map's materials
(build_map.py's MapMats: the texture sets at their full size, the barrel's glow as
its material makes it) and the outline's own glowing material. (The debris are
tools/build_weapon_fx.py's mesh systems, Debris1_STAT_FX..., that src/gibs.rs flies.)

It also writes where the map puts them, what stops movement and shots there, and
what the blueprint does:

  levels/oblivion_barrels.txt  barrel <actor> <tx ty tz> <qx qy qz qw> <scale>
                               (Bevy's axes, metres: the actor's placement, the
                               mesh components' Scale in it)
                               health <InitialHealth>
                               aidamage <AIDamageModifier>
                               radiusdamage <RadiusDamageModifier> <its distance, uu>
                               modifier <damage type> <value>
                               standing <state>  (its first state's name)
                               blown <state> <seconds>  (the state damage brings it
                               to, and how long that lasts)
                               held <system> <x y z>  (a particle system the blown
                               state ends with it: uu, the actor's frame)
                               dsys <state> ... (each state's other effects, as
                               tools/build_death_sets.py writes a destructible's:
                               particle, sound, damage, gibs)
  levels/oblivion_barrels.bin  collision groups, as levels/<map>_doors.bin packs
                               them (build_doors.py): "barrel:<n>" (the standing
                               barrel's) and "wreck:<n>" (the wreck's), both solid,
                               each its mesh's collision (build_level.py's) where
                               it stands

usage: python build_barrels.py <umodel export dir> <CookedPC> <foc_esc/assets>

The export folder holds umodel's exports of ESC_Ark_ART_m.xxx (a folder per
package as umodel writes it):
  umodel -game=trans -export -gltf -path=<CookedPC> -out=<dir> ESC_Ark_ART_m.xxx AutobotExplodingBarrel_STAT StaticMesh
  (likewise LPdamage2_STAT; and Debris1_STAT ... Debris4_STAT and
  CoverBLockCLarge_debris_02_STAT for tools/build_weapon_fx.py)
  umodel -game=trans -export -png -path=<CookedPC> -out=<dir> ESC_Ark_ART_m.xxx AutobotExplodingBarrel_texset_Color_NormX Texture2D
  (likewise its _Masks_NormY)
"""
import os
import sys

import numpy as np

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.path.join(HERE, "ue3"))
sys.path.insert(0, HERE)

import build_death_sets as bds  # noqa: E402
import build_level as bl  # noqa: E402
import build_map as bm  # noqa: E402
import build_doors as bd  # noqa: E402
import class_data as cd  # noqa: E402
from gltf_io import Doc, Writer  # noqa: E402
from ue3pkg import Package  # noqa: E402

PACKAGE = "ESC_Ark_ART_m"
MAP = "oblivion"
BLUEPRINT = "DES_AutobotExplodingBarrel_DSYS"
# The barrel's mesh components' Scale (each HmStaticMeshDestructionEffect's).
MESH_SCALE = 0.7
# (role, static mesh)
MESHES = [
    ("barrel", "AutobotExplodingBarrel_STAT"),
    ("wreck", "LPdamage2_STAT"),
]
# The barrel's outline (its material's OutlineColor at OutlineIntensity 1: half the
# weapon pickups', OutlineColor 0 1 1 at 2, which build_assets.py's OUTLINE_GLOW
# draws at 6) and how far out it stands (build_assets.py's OUTLINE_WIDTH, 2.5 cm at
# the guns' size, here in the barrel's units, drawn at 0.7).
OUTLINE_COLOR = [1.0, 0.0, 0.0]
OUTLINE_STRENGTH = 3.0
OUTLINE_WIDTH = 0.025 / 0.7


def outline(w, d, prim):
    """The outline shell: the mesh again, each corner pushed out along the average
    normal of the corners there (no cracks at hard edges), wound the other way so
    its outside is culled, black with a red glow (build_assets.py's outline_shell)."""
    at = prim["attributes"]
    pos = d.acc(at["POSITION"]).astype(np.float64)
    nrm = d.acc(at["NORMAL"]).astype(np.float64)
    idx = d.acc(prim["indices"]).reshape(-1, 3).astype(np.int64)
    key = np.round(pos / 1e-4).astype(np.int64)
    _, group = np.unique(key, axis=0, return_inverse=True)
    group = group.reshape(-1)
    smooth = np.zeros((group.max() + 1, 3))
    np.add.at(smooth, group, nrm)
    smooth /= np.maximum(np.linalg.norm(smooth, axis=1, keepdims=True), 1e-9)
    n = smooth[group]
    shell = pos + n * OUTLINE_WIDTH
    mat = w.material({"name": "ExplodingBarrelOutline",
                      "pbrMetallicRoughness": {"baseColorFactor": [0, 0, 0, 1], "metallicFactor": 0.0,
                                               "roughnessFactor": 1.0},
                      "emissiveFactor": OUTLINE_COLOR,
                      "extensions": {"KHR_materials_emissive_strength": {"emissiveStrength": OUTLINE_STRENGTH}}})
    used = w.g.setdefault("extensionsUsed", [])
    if "KHR_materials_emissive_strength" not in used:
        used.append("KHR_materials_emissive_strength")
    return {"attributes": {"POSITION": w.accessor(shell.astype(np.float32), 34962, minmax=True),
                           "NORMAL": w.accessor((-n).astype(np.float32), 34962)},
            "indices": w.accessor(idx[:, [0, 2, 1]].reshape(-1).astype(np.uint32), 34963),
            "material": mat}


def primitives(w, mats, d):
    out = []
    for p in d.g["meshes"][d.g["nodes"][0]["mesh"]]["primitives"]:
        at = p["attributes"]
        attrs = {"POSITION": w.accessor(d.acc(at["POSITION"]).astype(np.float32), 34962, minmax=True),
                 "NORMAL": w.accessor(d.acc(at["NORMAL"]).astype(np.float32), 34962),
                 "TEXCOORD_0": w.accessor(d.acc(at["TEXCOORD_0"]).astype(np.float32), 34962)}
        if "TANGENT" in at:
            tan = d.acc(at["TANGENT"]).astype(np.float32)
            t3 = tan[:, :3] / np.maximum(np.linalg.norm(tan[:, :3], axis=1, keepdims=True), 1e-9)
            attrs["TANGENT"] = w.accessor(np.concatenate([t3, tan[:, 3:4]], 1).astype(np.float32), 34962)
        name = d.g["materials"][p["material"]]["name"]
        out.append({"attributes": attrs,
                    "indices": w.accessor(d.acc(p["indices"]).reshape(-1).astype(np.uint32), 34963),
                    "material": mats.get(name)})
    return out


def main(export_root, cooked, assets):
    src = os.path.join(export_root, PACKAGE)
    out_dir = os.path.join(assets, "models", "props")
    os.makedirs(out_dir, exist_ok=True)
    bl.configure(src, None, assets)
    bl.EXTRA_TX = [d for d in bm.export_dirs(export_root, "Texture2D") if d != bl.TX]
    pkg = Package(os.path.join(cooked, PACKAGE + ".xxx"))
    texsets = bm.file_texsets(pkg)
    # (Its textures as KTX2 files beside it, as the maps' are.)
    w = Writer(image_dir=os.path.join(out_dir, "exploding_barrel"), image_uri="exploding_barrel")
    mats = bm.MapMats(w, texsets, {}, (pkg,), cooked)
    root = w.node({"name": "Root", "children": []}, root=True)
    for role, mesh in MESHES:
        path = os.path.join(src, "StaticMesh3", mesh + ".gltf")
        if not os.path.exists(path):
            raise SystemExit(f"{mesh} not exported ({path})")
        d = Doc(path)
        prims = primitives(w, mats, d)
        w.g["meshes"].append({"name": mesh, "primitives": prims})
        w.g["nodes"][root]["children"].append(w.node({"name": role, "mesh": len(w.g["meshes"]) - 1}))
        if role == "barrel":
            shell = [outline(w, d, p) for p in d.g["meshes"][d.g["nodes"][0]["mesh"]]["primitives"]]
            w.g["meshes"].append({"name": mesh + "_Outline", "primitives": shell})
            w.g["nodes"][root]["children"].append(w.node({"name": "barrel_outline",
                                                          "mesh": len(w.g["meshes"]) - 1}))
        print(f"  {role}: {mesh}, {[w.g['materials'][p['material']]['name'] for p in prims]}")
    if mats.missing:
        print("  ! materials without texture sets:", sorted(mats.missing))
    size = w.save(os.path.join(out_dir, "exploding_barrel.glb"))
    print(f"wrote models/props/exploding_barrel.glb ({size / 1e6:.2f} MB)")
    placements(pkg, src, assets, cooked)


class _Lines:
    """What build_death_sets.Deaths.factory writes a destructible's effects into."""

    def __init__(self):
        self.lines = []


def blueprint_lines(cooked):
    """The barrel's blueprint (BLUEPRINT): its damage rules, its standing state's
    sound and its blown state's effects (see the module docs)."""
    mp = cd.Mp(cooked, PACKAGE + ".xxx")
    bp = next((i for i in range(1, len(mp.pkg.exports) + 1)
               if mp.pkg.exports[i - 1]["name"] == BLUEPRINT
               and mp.pkg.class_name(mp.pkg.exports[i - 1]) == "HmDestructibleBlueprint"), 0)
    if not bp:
        raise SystemExit(f"no {BLUEPRINT} in {PACKAGE}")
    pr = bds.parsed(mp, mp.props(bp))
    lines = ["aidamage\t%s" % bds.fmt(bds.num(pr.get("AIDamageModifier"), 1.0)),
             "radiusdamage\t%s\t%s" % (bds.fmt(bds.num(pr.get("RadiusDamageModifier"), 1.0)),
                                       bds.fmt(bds.num(pr.get("MinRadiusDamageDistanceForModifier"), 50.0)))]
    for m in cd._structs(mp.pkg, pr.get("DamageModifiers")):
        kind = bds.last(bds.ref(mp, m.get("ModifierDamageType")))
        if kind:
            lines.append("modifier\t%s\t%s" % (kind, bds.fmt(bds.num(m.get("ModifierValue"), 1.0))))
    pieces = [i for i in cd._objs(pr.get("PieceBlueprints")) if i > 0]
    if len(pieces) != 1:
        raise SystemExit(f"{BLUEPRINT} has {len(pieces)} pieces (one expected)")
    states = [i for i in cd._objs(mp.props(pieces[0]).get("StateBlueprints"))]
    standing = states[0]
    spr = mp.props(standing)
    lines.append("health\t%s" % bds.fmt(bds.num(spr.get("InitialHealth"), 1.0)))
    # (The state damage brings it to: its link that the plain damage's trigger,
    # HmGenericDamageDestructionTrigger, takes; the others, but the Sys
    # Corruptor's, go there too.)
    blown = None
    for link in cd._objs(spr.get("LinkBlueprints")):
        lpr = mp.props(link)
        trigger = mp.full(cd._obj(lpr.get("TransitionTrigger"))) or ""
        if trigger.endswith("HmGenericDamageDestructionTrigger"):
            blown = states[int(bds.num(lpr.get("NextStateHandle"), 0))]
    if not blown:
        raise SystemExit(f"{BLUEPRINT}'s standing state has no damage link")
    holder = _Lines()
    ptm = bds.transform(mp, mp.props(pieces[0]).get("Transform"))
    # (What the demo draws or plays of a state's effects: its wreck is the glb's,
    # its targetable the barrels' own, the AI's hearing and Kismet's notice not the
    # demo's.)
    skip = ("HmStaticMeshDestructionEffectFactory", "TnTargetableDestructionEffectFactory",
            "HmNoiseDestructionEffectFactory", "HmStateNotificationDestructionEffectFactory")

    def effects(state, name, held):
        for f in cd._objs(mp.props(state).get("EffectFactories")):
            if f <= 0:
                continue
            cls = mp.pkg.class_name(mp.pkg.exports[f - 1])
            fpr = bds.parsed(mp, mp.props(f))
            if cls in skip:
                continue
            if held and cls == "HmParticleDestructionEffectFactory" and fpr.get("TerminateEffectOnExitState"):
                at, _ = bds.transform(mp, fpr.get("Transform"))
                at = tuple(a + b for a, b in zip(at, ptm[0]))
                lines.append("held\t%s\t%s" % (bds.last(bds.ref(mp, fpr.get("Template"))),
                                                " ".join(bds.fmt(x) for x in at)))
            else:
                bds.Deaths.factory(holder, name, mp, f, ptm)

    name = str(spr.get("StateName", "Standing"))
    lines.append("standing\t%s" % name)
    effects(standing, name, False)
    bpr = mp.props(blown)
    name = str(bpr.get("StateName", "Blown"))
    lines.append("blown\t%s\t%s" % (name, bds.fmt(bds.num(bpr.get("AutomaticTransitionTime"), 0.0))))
    effects(blown, name, True)
    return lines + holder.lines


def placements(pkg, src, assets, cooked):
    """The map's barrels: each TnStaticDestructibleActor whose destructible is the
    barrel's, its placement, and its standing and wrecked meshes' collision; and
    the blueprint's rules and effects."""
    by_index = {i + 1: e for i, e in enumerate(pkg.exports)}
    lines = ["# Oblivion's exploding barrels and what they do (tools/build_barrels.py)"]
    groups = bd.Groups(np.zeros(3), 0.01)
    shapes = {role: Doc(os.path.join(src, "StaticMesh3", mesh + ".gltf")) for role, mesh in MESHES}
    n = 0
    for i, e in by_index.items():
        if pkg.class_name(e) != "TnStaticDestructibleActor":
            continue
        pr = bm.actor_props(pkg, e)
        comp = bm.obj_index(pr.get("DestructibleComponent"))
        blueprint = bm.obj_name(pkg, bm.chain_props(pkg, comp).get("Blueprint")) if comp else None
        if not blueprint or not blueprint.endswith(BLUEPRINT):
            continue
        m = bm.actor_matrix(pr)
        T = np.eye(4)
        T[:3, :3] = bd.P @ m[:3, :3].T @ bd.P
        T[:3, 3] = bd.P @ m[3, :3] * 0.01
        # (Its meshes at their components' Scale.)
        S = T @ np.diag([MESH_SCALE, MESH_SCALE, MESH_SCALE, 1.0])
        t, q, sc = bd.decompose(S)
        lines.append("\t".join(["barrel", e["name"], bd.fv(t), bd.fv(q), bd.f(sc[0])]))
        for role, mesh in MESHES:
            tris = bd.mesh_triangles(mesh, shapes[role], S)
            groups.add(f"{role}:{n}", tris, solid=True)
            print(f"  {e['name']}: {role} collision {len(tris)} triangles")
        n += 1
    if not n:
        raise SystemExit(f"no {BLUEPRINT} placed in {PACKAGE}")
    lines += blueprint_lines(cooked)
    levels = os.path.join(assets, "levels")
    os.makedirs(levels, exist_ok=True)
    with open(os.path.join(levels, f"{MAP}_barrels.txt"), "w", encoding="utf-8") as fh:
        fh.write("\n".join(lines) + "\n")
    groups.write(os.path.join(levels, f"{MAP}_barrels.bin"))
    print(f"wrote levels/{MAP}_barrels.txt and levels/{MAP}_barrels.bin ({n} barrels)")


if __name__ == "__main__":
    if len(sys.argv) != 4:
        sys.exit(__doc__)
    main(*sys.argv[1:])
