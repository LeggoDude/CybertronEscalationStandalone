"""Build the sentries of the maps' traps (src/traps.rs), as the BASE packages of the
maps that use them have them (tools/build_doors.py writes where they stand and
what wakes them: levels/<map>_doors.txt's sentry lines):

  * models/traps/sentry_rocket.glb: Downfall's Defensive Sentries
    (WEP_Interact_Sentry_p.RocketInteract_Sentry_TURRETDEF, its destructible's
    SentryRocketESC_DES: WEP_SentryBlaster01_p.SentryRocket.WEP_SentryRocket_SKEL,
    drawn at 1.5 times its size), with WEP_SentryRocket_ANIM's clips
    (SentryRocket_Activate, unfolding, and SentryRocket_DeActivate, folding up).
  * models/traps/sentry_mg.glb: the GunShip Support's guns
    (WEP_Interact_Sentry_p.MGInteract_TURRETDEF, SentryBlaster01_DES:
    WEP_SentryBlaster01_p.SentryBlaster.WEP_SentryBlaster_SKEL, at its own size),
    with WEP_SentryBlaster_ANIM's (SentryBlaster_Activate, SentryBlaster_DeActivate).

Both in umodel's glTF axes as they are (placed by their actors' turns), in
WEP_Sentry_New_MATINST (WEP_Sentry_New_TEXSET), its colours the material's own
(Cust_Color_A, a dark red, and Cust_COLOR_B, a grey). Their heads turn on
C_Robo01_XT, their anim tree's YawPitchControl (HmSkelControl_TurretConstrained,
added to the clips' pose).

  * models/traps/textures/: the texture set (build_class.py's Textures) at its full
    size (1024): a cooked map keeps only its small mips (umodel's export is 128
    pixels), the rest staying in the package it came from
    (WEP_SentryBlaster01_p.xxx), which CookedPC has (ue3/texture.py reads them
    through the map's export; without that package, the map's own 128).
  * models/traps/traps.txt (tab-separated; umodel's glTF axes, metres):
      clip   <model> <role> <index> <seconds> <loop 0/1>
      socket <model> <name> <bone> <tx ty tz> <qx qy qz qw> <sx sy sz>
      paint  <model> <r g b> <r g b>     (its customization colours, linear)
      aim    <model> <bone>              (the bone its head turns on)
    (the clips' roles: activate and deactivate; the sockets the meshes' own: the
    rocket sentry's MuzzleFlash_T, _B and _M, the gun's MuzzleFlash_L and _R, both's
    LaserSocket and LaserSocket_2, where their sights' effects play...).

usage: python build_traps.py <umodel export dir> <CookedPC> <foc_esc/assets>

The export folder holds a folder per package as umodel writes it:
  umodel -game=trans -export -gltf -path=<CookedPC> -out=<dir> ESC_Iacon_BASE_m.xxx WEP_SentryRocket_SKEL
  umodel -game=trans -export -path=<CookedPC> -out=<dir> ESC_Iacon_BASE_m.xxx WEP_SentryRocket_ANIM
  umodel -game=trans -export -gltf -path=<CookedPC> -out=<dir> ESC_Gladiator_BASE_m.xxx WEP_SentryBlaster_SKEL
  umodel -game=trans -export -path=<CookedPC> -out=<dir> ESC_Gladiator_BASE_m.xxx WEP_SentryBlaster_ANIM
  umodel -game=trans -export -png -path=<CookedPC> -out=<dir> ESC_Iacon_BASE_m.xxx
      WEP_Sentry_New_TEXSET_Color_NormX WEP_Sentry_New_TEXSET_Masks_NormY
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

OUT_DIR = os.path.join("models", "traps")
# (model file, package, skeletal mesh, its full name, anim set, clips: (role, clip,
# looping), the material whose colours it wears)
MODELS = [
    ("sentry_rocket", "ESC_Iacon_BASE_m.xxx", "WEP_SentryRocket_SKEL",
     "WEP_SentryBlaster01_p.SentryRocket.WEP_SentryRocket_SKEL", "WEP_SentryRocket_ANIM",
     [("activate", "SentryRocket_Activate", False), ("deactivate", "SentryRocket_DeActivate", False)]),
    ("sentry_mg", "ESC_Gladiator_BASE_m.xxx", "WEP_SentryBlaster_SKEL",
     "WEP_SentryBlaster01_p.SentryBlaster.WEP_SentryBlaster_SKEL", "WEP_SentryBlaster_ANIM",
     [("activate", "SentryBlaster_Activate", False), ("deactivate", "SentryBlaster_DeActivate", False)]),
]
MATERIAL = "WEP_Sentry_New_MATINST"
# The maps whose Texture2D exports lead to the texture set's full-size mips.
TEXTURE_MAPS = ("ESC_Iacon_BASE_m.xxx", "ESC_Gladiator_BASE_m.xxx")
# The bone the anim tree's YawPitchControl turns (WEP_SentryBlaster_ANIMTREE's
# second SkelControlList: both sentries' tree).
AIM_BONE = "C_Robo01_XT"


class FullTextures(bc.Textures):
    """The texture sets at their full size: each picture's largest mip read through
    the maps' Texture2D exports (TEXTURE_MAPS: their big mips from the package the
    textures came from), umodel's small export when no map has them."""

    def _pictures(self, base, cooked):
        import texture as ue3_texture
        for package in TEXTURE_MAPS:
            path = os.path.join(self.cooked, package)
            if not os.path.exists(path):
                continue
            p = ue3_texture._package(path)
            index = {e["name"]: i + 1 for i, e in enumerate(p.exports) if p.class_name(e) == "Texture2D"}
            names = (base + "_Color_NormX", base + "_Masks_NormY")
            if all(n in index for n in names):
                c, m = (np.asarray(ue3_texture.read_texture(p, index[n], self.cooked)[0]) for n in names)
                print("  %s: %dx%d (from %s's export)" % (base, c.shape[1], c.shape[0], package))
                return c, m
        return super()._pictures(base, cooked)


def build(model, ex, textures, cooked, out):
    name, package, mesh, full, anim_set, clips = model
    path = ex.find("SkeletalMesh3", mesh + ".gltf")
    if not path:
        print("  ! not exported:", mesh)
        return []
    mp = cd.Mp(cooked, package)
    ref = Doc(path)
    w = Writer(share=True, image_dir=os.path.join(out, "textures"), image_uri="textures")
    mats = bc.Materials(w, textures, PackageMaterials(mp))
    r = w.node({"name": "Root", "children": []}, root=True)
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
    vec = mp.material(MATERIAL)["vectors"]
    a = vec.get("Cust_Color_A", (1.0, 1.0, 1.0, 1.0))[:3]
    b = vec.get("Cust_COLOR_B", (1.0, 1.0, 1.0, 1.0))[:3]
    lines.append(["paint", name, bc.fmt(a), bc.fmt(b)])
    if AIM_BONE in names:
        lines.append(["aim", name, AIM_BONE])
    else:
        print("  ! no", AIM_BONE, "on", mesh)
    return lines


def main(export_dir, cooked, assets):
    out = os.path.join(assets, OUT_DIR)
    os.makedirs(out, exist_ok=True)
    ex = bc.Exports(export_dir)
    textures = FullTextures(ex, os.path.join(out, "textures"), os.path.join(assets, "models", "vehicle.glb"), cooked)
    lines = ["# The traps' sentries (tools/build_traps.py): see its header for the fields"]
    for model in MODELS:
        print("==", model[0])
        lines += ["\t".join(f) for f in build(model, ex, textures, cooked, out)]
    open(os.path.join(out, "traps.txt"), "w").write("\n".join(lines) + "\n")
    print("wrote", os.path.join(OUT_DIR, "traps.txt"))


if __name__ == "__main__":
    if len(sys.argv) != 4:
        sys.exit(__doc__)
    main(sys.argv[1], sys.argv[2], sys.argv[3])
