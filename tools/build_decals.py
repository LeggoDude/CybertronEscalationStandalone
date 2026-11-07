"""Build the characters' decal sets (the Customize screen's Decals) from the game.

The game's eight decal sets (TransCustomization.ini's DecalSet01 to DecalSet08, 400
shards each, beside the free DecalSetNone) are TR_CharacterCustomDecals_p's
DecalSet01_PART to DecalSet08_PART (TnMeshPartDecals), cooked into UI_Lobby_m.xxx:
eight decals each, every one a small quad (DecalNN_STAT, 32 in all) and the start of
the names of the sockets it hangs on (AttachmentSocket: DecalBack01, DecalBack02,
DecalFront01, DecalFront02, DecalSide01, DecalSide02, DecalTop01, DecalTop02). The
game hangs a decal's quad on every socket whose name starts with that
(MeshPart.TrySocketStaticMeshAttachmentOnComponent: DecalBack01_Left01,
DecalBack01_Right01...), on the robot's meshes (its chests have them) and on its
vehicle's (the part is a vehicle part, IsVehiclePart, and a robot one, TnMeshPart's
IsRobotPart), and draws it with TR_CharacterCustomDecals_p's TR_Decals1_Mat_INST, an
instance of TR_AllShader_p's TR_FactionLogo_Mat (the factions' logos' material):
unlit and see-through, its picture (Decals_FullSheet: the 32 decals on one 1024 x
1024 grey sheet) lit in the character's EnergonColor (TnCharacterApplier.
ApplyColorsToMesh: its faction's, as the static switch UseFactionEnergonColor is
off) times LogoIntensity, pulsing as the energon does (fractalMaps panned over it at
EnergonPanUVTile, with the time through two sines, times energonIntensity and to the
power of energonContrast), and as opaque as 15 times the picture.

This script writes, from umodel's exports and UI_Lobby_m.xxx:

  assets/models/decals/decals.glb         the 32 quads, a root node each named after
                                          its mesh (umodel's glTF axes and metres, as
                                          the sockets in <class>.txt have them)
  assets/models/decals/decals_sheet.ktx2  the sheet (BC1, sRGB as the game's texture
                                          is, with its mips)
  assets/models/decals/decals.txt         the sets and their material's values:
      decal <set id> <socket name start> <mesh>   (a set's decals, in its order)
      param <name> <value>                        (TR_Decals1_Mat_INST's scalars)

The sockets are the characters' own: the robots' chests' are among <class>.txt's
socket lines, the vehicles' its vdecal lines (build_class.py --decals). src/decals.rs
draws them (assets/shaders/decal.wgsl).

usage: python build_decals.py <umodel export dir> <CookedPC> <foc_esc/assets>
       python build_decals.py --umodel <CookedPC> <umodel export dir>
           prints the umodel commands that export the quads and the sheet
"""
import os
import sys

import numpy as np
from PIL import Image

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
sys.path.insert(0, os.path.join(HERE, "ue3"))
import class_data  # noqa: E402
import ktx2_io  # noqa: E402
from gltf_io import Doc, Writer  # noqa: E402
from ue3pkg import Package  # noqa: E402

# Where the decal sets are cooked (with the Customize screen's lobby).
PACKAGE = "UI_Lobby_m.xxx"
# The sets' parts (TransCustomization.ini's PartPaths), in the ini's order.
SETS = [f"DecalSet{n:02d}" for n in range(1, 9)]
PART = "TR_CharacterCustomDecals_p.{}_PART"
MATERIAL = "TR_CharacterCustomDecals_p.Material.TR_Decals1_Mat_INST"
SHEET = "Decals_FullSheet"
# The material's values the demo's shader takes (its other scalar, Faction, picks
# the Autobot or Decepticon logo: both are the sheet here).
PARAMS = ("LogoIntensity", "EnergonPanUVTile", "energonIntensity", "energonContrast")


def read_sets(pkg):
    """{set id: [(socket name start, mesh name)]} from the TnMeshPartDecals parts."""
    by_name = {pkg.full_name(i): i for i in range(1, len(pkg.exports) + 1)}
    out = {}
    for set_id in SETS:
        idx = by_name.get(PART.format(set_id))
        if not idx:
            raise SystemExit(f"{PART.format(set_id)} isn't in {PACKAGE}")
        decals = []
        for el in class_data._structs(pkg, class_data._props(pkg, idx).get("MeshPartDecals")):
            mesh = class_data._obj(el.get("DecalMesh"))
            socket = class_data._str(el.get("AttachmentSocket"))
            if mesh and socket:
                decals.append((socket, pkg.obj_name(mesh)))
        out[set_id] = decals
    return out


def read_params(pkg):
    """TR_Decals1_Mat_INST's scalar parameters ({name: value})."""
    idx = next((i for i in range(1, len(pkg.exports) + 1) if pkg.full_name(i) == MATERIAL), None)
    if not idx:
        raise SystemExit(f"{MATERIAL} isn't in {PACKAGE}")
    pr = class_data._props(pkg, idx)
    out = {}
    for el in class_data._structs(pkg, pr.get("ScalarParameterValues")):
        name, value = el.get("ParameterName"), el.get("ParameterValue")
        if isinstance(value, float):
            out[str(name)] = value
    return out


def find(export_dir, kind, name):
    for root, _, files in os.walk(export_dir):
        if os.path.basename(root) == kind and name in files:
            return os.path.join(root, name)
    return None


def build_model(export_dir, meshes, path):
    """decals.glb: each quad as a root node named after its mesh (positions, normals
    and texture coordinates, as umodel exported them)."""
    w = Writer(share=True)
    for mesh in meshes:
        src = find(export_dir, "StaticMesh3", mesh + ".gltf")
        if not src:
            raise SystemExit(f"{mesh} not exported (see --umodel)")
        d = Doc(src)
        prims = []
        for p in d.g["meshes"][d.g["nodes"][0]["mesh"]]["primitives"]:
            a = p["attributes"]
            pos = d.acc(a["POSITION"]).astype(np.float32)
            prim = {"attributes": {
                "POSITION": w.accessor(pos, target=34962, minmax=True),
                "NORMAL": w.accessor(d.acc(a["NORMAL"]).astype(np.float32), target=34962),
                "TEXCOORD_0": w.accessor(d.acc(a["TEXCOORD_0"]).astype(np.float32), target=34962),
            }, "indices": w.accessor(d.acc(p["indices"]).astype(np.uint32).ravel(), target=34963)}
            prims.append(prim)
        w.g["meshes"].append({"name": mesh, "primitives": prims})
        w.node({"name": mesh, "mesh": len(w.g["meshes"]) - 1}, root=True)
    return w.save(path)


def build_sheet(export_dir, path):
    src = find(export_dir, "Texture2D", SHEET + ".png")
    if not src:
        raise SystemExit(f"{SHEET} not exported (see --umodel)")
    grey = np.asarray(Image.open(src).convert("L"))
    data = ktx2_io.encode(np.repeat(grey[..., None], 3, -1), "color")
    with open(path, "wb") as fh:
        fh.write(data)
    return grey.shape


def main(export_dir, cooked, assets):
    pkg = Package(os.path.join(cooked, PACKAGE))
    sets = read_sets(pkg)
    params = read_params(pkg)
    out = os.path.join(assets, "models", "decals")
    os.makedirs(out, exist_ok=True)
    meshes = sorted({m for decals in sets.values() for _, m in decals})
    size = build_model(export_dir, meshes, os.path.join(out, "decals.glb"))
    print(f"wrote decals.glb ({len(meshes)} quads, {size} bytes)")
    h, w = build_sheet(export_dir, os.path.join(out, "decals_sheet.ktx2"))
    print(f"wrote decals_sheet.ktx2 ({w} x {h})")
    lines = ["# The decal sets (tools/build_decals.py): each set's decals, the start of the",
             "# names of the sockets each hangs on and its quad (decals.glb), and the",
             "# material's values (TR_Decals1_Mat_INST's)."]
    for set_id in SETS:
        for socket, mesh in sets[set_id]:
            lines.append("\t".join(["decal", set_id, socket, mesh]))
    for name in PARAMS:
        if name not in params:
            raise SystemExit(f"{MATERIAL} has no {name}")
        lines.append("\t".join(["param", name, f"{params[name]:.6g}"]))
    with open(os.path.join(out, "decals.txt"), "w", encoding="utf-8") as fh:
        fh.write("\n".join(lines) + "\n")
    print(f"wrote decals.txt ({len(SETS)} sets)")


def umodel_commands(cooked, export_dir):
    pkg = Package(os.path.join(cooked, PACKAGE))
    meshes = sorted({m for decals in read_sets(pkg).values() for _, m in decals})
    objs = " ".join(f"-obj={m}" for m in meshes)
    base = f'umodel -game=trans -export -path="{cooked}" -out="{export_dir}" {PACKAGE}'
    return [f"{base.replace('-export', '-export -gltf')} {objs}", f"{base.replace('-export', '-export -png')} -obj={SHEET}"]


if __name__ == "__main__":
    args = sys.argv[1:]
    if len(args) == 3 and args[0] == "--umodel":
        print("\n".join(umodel_commands(*args[1:])))
    elif len(args) == 3:
        main(*args)
    else:
        raise SystemExit(__doc__)
