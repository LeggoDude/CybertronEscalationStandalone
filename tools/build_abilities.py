"""Build the class abilities' models (src/abilities.rs): the Soldier's Flak Shield and
the Scientist's Battle Sentry, as mp_shared.xxx has them.

Writes into <assets>/models/abilities/:

  * flak_shield.glb: WEP_Shield_p.MP.WEP_ShieldLeader_SKEL, the shield the Soldier's
    block controller hangs on his ShieldSocket_L (TR_CustomSoldier_ROBODEF's
    TnRobotBlockControllerData_871: ShieldMeshTemplate ShieldLeader_MP_WEPMESH), with
    its clips (WEP_ShieldLeader_ANM: ShieldGrimlock_Equip, unfolding, and
    ShieldGrimlock_Drop, folding away). It is in umodel's glTF axes as it is, not
    turned (hung under the socket, it sits as the game hangs it). Its frame is drawn
    on WEP_MPShield_TEXSET (painted like the guns); its energon pane
    (WEP_MPShield_Energon_MAT_INST, on the game's translucent Canister_CasingTrans_MAT)
    as the material "ShieldEnergon": see-through, its pattern's brightness as its
    emissive texture, which abilities.rs tints in the player's energon colour.
  * sentry.glb and sentry_slow.glb: TPRO_AttackSentry_p's AP_AttackSentry_SKEL (the
    Battle Sentry: WEP_SentryAbility_p.Default_TURRETDEF's Sentry_DSYS draws it) and
    AP_AttackSentrySlow_SKEL (Rust Sentry's, Slow_TURRETDEF), turned as the demo's
    models are (build_class.py's YAW), each with AP_AttackSentry_ANIM's clips:
    Nav_Shooting_Idle (its anim tree's one node, Sentry_ANIMTREE, looping),
    AP_AttackSentry_Activate and AP_AttackSentry_Deactivate (poses). Their texture
    sets are painted in the owner's colours (TnSentryPawnAbility.ApplyColor).
  * textures/: the texture sets (build_class.py's Textures).
  * abilities.txt (tab-separated; umodel's glTF axes, metres):
      clip   <model> <role> <index> <seconds> <loop 0/1>
      socket <model> <name> <bone> <tx ty tz> <qx qy qz qw> <sx sy sz>
    (the clips' roles: equip and drop for the shield; idle, activate and
    deactivate for the sentries; the sockets are the meshes' own: the sentry's
    MuzzleFlash, Booster, TargetingSocket..., the shield's MuzzleFlash).

usage: python build_abilities.py <umodel export dir> <CookedPC> <foc_esc/assets>

The export folder needs, from mp_shared.xxx:
  umodel -game=trans -export -gltf -path=<CookedPC> -out=<dir> mp_shared.xxx
      -obj=AP_AttackSentry_SKEL -obj=AP_AttackSentrySlow_SKEL -obj=WEP_ShieldLeader_SKEL
  umodel -game=trans -export -path=<CookedPC> -out=<dir> mp_shared.xxx
      -obj=AP_AttackSentry_ANIM -obj=WEP_ShieldLeader_ANM
  umodel -game=trans -export -png -path=<CookedPC> -out=<dir> mp_shared.xxx
      -obj=<set>_Color_NormX -obj=<set>_Masks_NormY for each of AP_AttackSentry_body_TEXSET,
      AP_AttackSentry_parts_TEXSET, WEP_RustCannon_TEXSET, WEP_MPShield_TEXSET and
      WEP_MPShield_Energon_TEXSET
"""
import os
import sys

import numpy as np

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.path.join(HERE, "ue3"))
sys.path.insert(0, HERE)

import build_class as bc  # noqa: E402
import class_data as cd  # noqa: E402
from build_enemies import PackageMaterials  # noqa: E402
from gltf_io import Doc, Writer  # noqa: E402

PACKAGE = "mp_shared.xxx"
OUT_DIR = os.path.join("models", "abilities")
# (model file, skeletal mesh, its full name in the package, turned, anim set, clips:
# (role, clip, looping))
MODELS = [
    ("flak_shield", "WEP_ShieldLeader_SKEL", "WEP_Shield_p.MP.WEP_ShieldLeader_SKEL", False,
     "WEP_ShieldLeader_ANM", [("equip", "ShieldGrimlock_Equip", False), ("drop", "ShieldGrimlock_Drop", False)]),
    ("sentry", "AP_AttackSentry_SKEL", "TPRO_AttackSentry_p.SkelMesh.AP_AttackSentry_SKEL", True,
     "AP_AttackSentry_ANIM", [("idle", "Nav_Shooting_Idle", True), ("activate", "AP_AttackSentry_Activate", False),
                              ("deactivate", "AP_AttackSentry_Deactivate", False)]),
    ("sentry_slow", "AP_AttackSentrySlow_SKEL", "TPRO_AttackSentry_p.SkelMesh.AP_AttackSentrySlow_SKEL", True,
     "AP_AttackSentry_ANIM", [("idle", "Nav_Shooting_Idle", True), ("activate", "AP_AttackSentry_Activate", False),
                              ("deactivate", "AP_AttackSentry_Deactivate", False)]),
]
# The shield's energon pane: its texture set, and the material written for it.
ENERGON_TEXSET = "WEP_MPShield_Energon_TEXSET"
ENERGON_MATERIAL = "ShieldEnergon"
# How see-through the pane is (Canister_CasingTrans_MAT's Opacity Mask Intensity
# 0.75 over its fresnel: about this much where it faces the view).
ENERGON_ALPHA = 0.3


def energon_material(w, textures):
    """The shield's energon pane: see-through, glowing with its pattern (the colour
    picture's brightness, stretched to full), which abilities.rs tints."""
    pictures = textures._pictures(ENERGON_TEXSET, False)
    m = {"name": ENERGON_MATERIAL, "alphaMode": "BLEND", "doubleSided": True,
         "pbrMetallicRoughness": {"baseColorFactor": [0.0, 0.0, 0.0, ENERGON_ALPHA], "metallicFactor": 0.0,
                                  "roughnessFactor": 0.15},
         "emissiveFactor": [1.0, 1.0, 1.0]}
    if pictures is not None:
        c = pictures[0][..., :3].astype(np.float32)
        lum = c.max(-1)
        lum = (lum / max(float(np.percentile(lum, 99.5)), 1.0)).clip(0, 1)
        grey = (np.repeat(lum[..., None], 3, -1) * 255).astype(np.uint8)
        m["emissiveTexture"] = {"index": w.texture(ENERGON_MATERIAL + "_e", textures._encode(grey, "color"))}
    return w.material(m)


def build(model, ex, textures, game, mp, out):
    name, mesh, full, turned, anim_set, clips = model
    path = ex.find("SkeletalMesh3", mesh + ".gltf")
    if not path:
        print("  ! not exported:", mesh)
        return []
    ref = Doc(path)
    w = Writer(share=True, image_dir=os.path.join(out, "textures"), image_uri="textures")
    mats = bc.Materials(w, textures, game)
    if name == "flak_shield":
        mats.cache[ENERGON_TEXSET] = energon_material(w, textures)
    r = bc.root_node(w) if turned else w.node({"name": "Root", "children": []}, root=True)
    _, _, joints = bc.copy_skeleton(w, ref, r)
    bone_names = [ref.g["nodes"][j]["name"] for j in ref.g["skins"][0]["joints"]]
    names = {n: joints[k] for k, n in enumerate(bone_names)}
    ibm = ref.acc(ref.g["skins"][0]["inverseBindMatrices"]).astype(np.float32)
    w.g["skins"].append({"joints": joints, "inverseBindMatrices": w.accessor(ibm), "skeleton": joints[0]})
    prims = bc.primitives(w, mats, ref, ref.g["nodes"][0]["mesh"], mesh)
    w.g["meshes"].append({"name": mesh, "primitives": prims})
    w.g["nodes"][r]["children"].append(w.node({"name": mesh, "mesh": len(w.g["meshes"]) - 1, "skin": 0}))
    lines = []
    rest = bc.robot_rest(ref)
    for role, clip, loop in clips:
        bones, dt = bc.clip_tracks(ex, [anim_set], clip)
        if bones is None:
            print("  ! clip not exported:", anim_set, clip)
            continue
        length = bc.base_clip(w, names, role, bones, dt, loop=loop, rest=rest, skip=())
        lines.append(["clip", name, role, str(len(w.g["animations"]) - 1), "%.4g" % length, str(int(loop))])
    w.save(os.path.join(out, name + ".glb"))
    print("wrote", os.path.join(OUT_DIR, name + ".glb"), "(%d clips)" % len(w.g["animations"]))
    idx = next((i for i in mp.by_name.get(mesh, []) if mp.pkg.full_name(i) == full), None)
    for s in mp.sockets(idx) if idx else []:
        if s["bone"] not in names:
            print("  ! socket bone not on the skeleton:", name, s["name"], s["bone"])
            continue
        t, q, sc = bc.gltf_socket(s)
        lines.append(["socket", name, s["name"], s["bone"], bc.fmt(t), bc.fmt(q), bc.fmt(sc)])
    return lines


def main(export_dir, cooked, assets):
    out = os.path.join(assets, OUT_DIR)
    os.makedirs(out, exist_ok=True)
    ex = bc.Exports(export_dir)
    textures = bc.Textures(ex, os.path.join(out, "textures"), os.path.join(assets, "models", "vehicle.glb"), cooked)
    mp = cd.Mp(cooked, PACKAGE)
    game = PackageMaterials(mp)
    lines = ["# The class abilities' models (tools/build_abilities.py): see its header for the fields"]
    for model in MODELS:
        print("==", model[0])
        lines += ["\t".join(f) for f in build(model, ex, textures, game, mp, out)]
    open(os.path.join(out, "abilities.txt"), "w").write("\n".join(lines) + "\n")
    print("wrote", os.path.join(OUT_DIR, "abilities.txt"))


if __name__ == "__main__":
    if len(sys.argv) != 4:
        sys.exit(__doc__)
    main(sys.argv[1], sys.argv[2], sys.argv[3])
