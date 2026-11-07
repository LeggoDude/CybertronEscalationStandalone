"""Build Escalation's Armory Recreator pods (TransContent.TnInteractLootBox): the pod
model with its opening, and where each map has its pods (src/recreator.rs).

Writes into <assets>:

  * models/storage_pod.glb: TPRO_NEU_StoragePod_p's AP_StoragePod_SKEL (umodel's glTF
    export) with its bones and its one clip, AP_StoragePodOpen (the pod's AnimSet
    NEU_StoragePod_ANIM: 3 s, read from the map's package by ue3/anim.py); its
    materials as the maps draw the pods (build_map.py's AP_StoragePod_MATINST,
    AP_StoragePodDoor_MATINST and AP_StoragePod_Glass_MAT_INST, their textures taken
    from Downfall's build into models/storage_pod/); and an outline round its body
    (StoragePodOutline: AP_StoragePod_MATINST's EnableOutlineHighlight is on, cyan;
    the door's is off), its shell drawn as the pod stands closed, its material
    (StoragePodOutline_MAT) lit by recreator.rs; and a node on its bones for each
    puff of steam its opening lets out (STEAM).
  * levels/<map>_pods.txt: each map's pods (its BASE package's TnInteractLootBox
    actors), tab-separated, positions in Bevy metres ((ue x, ue z, ue y) x 0.01),
    rotations as (x, y, z, w) quaternions in Bevy axes (P R P, P swapping y and z):
      pod <name> <x y z> <rotation> <scale x y z>
          <weapon socket x y z> <its rotation> <misc socket x y z> <its rotation>
          <interact spot x y z>
    and a line `collide` when the map's build draws none of them (Oblivion's leaves
    them out), so has no collision for them either: recreator.rs gives them some.
    The sockets are the pod mesh's (on C_Body_XB, the mesh's origin, unrotated):
    WeaponLootSocket (-65, 0, 200), pitch -90, yaw 90, roll -90, where a weapon
    the pod makes is dropped; MiscLootSocket (-113.39, 0, 250), yaw -90, where
    equipment is. The interact spot is 500 along the pod's +X (its front): where
    the buyer stands (UseConditions' distance is from there) and where what it made
    is picked up from (TnInteractActionLootBoxSpawn moves its pickup cylinder there).

usage: python build_pods.py <CookedPC> <umodel export dir> <foc_esc/assets>

The export folder needs AP_StoragePod_SKEL as glTF, from any of the Escalation BASE
packages (umodel -game=trans -export -gltf -path=<CookedPC> -out=<folder>
ESC_Iacon_BASE_m.xxx -obj=AP_StoragePod_SKEL): it is looked for in
<folder>/*/SkeletalMesh3/. Downfall must have been built first (build_map.py
downfall), for the pod's textures. One map's package is read at a time (about 1 GB).
"""
import json
import math
import os
import shutil
import struct
import sys

import numpy as np

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.path.join(HERE, "ue3"))
sys.path.insert(0, HERE)

import anim  # noqa: E402
import build_class as bc  # noqa: E402
import build_map as bm  # noqa: E402
import layout as lay  # noqa: E402
from gltf_io import Doc, Writer, node_globals  # noqa: E402
from ue3pkg import Package  # noqa: E402

MESH = "AP_StoragePod_SKEL"
# The maps' own pods (build_map.py's bind-pose placements), which recreator.rs takes
# out of the map for these.
BAKED = MESH + "@bind"
ANIM_SET = "NEU_StoragePod_ANIM"
CLIP = "AP_StoragePodOpen"
# The map whose build the pod's textures come from, and its package (for the clip).
TEXTURE_MAP = "downfall"
# The materials as build_map.py writes them for the pods (their names in the glTF
# export), with the textures each takes from the map's folder.
MATERIALS = {
    "AP_StoragePod_MATINST": ("AP_StoragePod_TXTSET", "AP_StoragePod_MATINST_e", 3.0),
    "AP_StoragePodDoor_MATINST": ("AP_StoragePodDoor_TXTSET", "AP_StoragePodDoor_MATINST_e", 2.9058823585510254),
}
GLASS = "AP_StoragePod_Glass_MAT_INST"
# The outline (ENV_OutlineHighlight_MF): round the body only; OutlineColor (0, 1, 1),
# the default; its strength is recreator.rs's (OutlineIntensity 2, pulsing).
OUTLINED = "AP_StoragePod_MATINST"
OUTLINE_NODE = "StoragePodOutline"
OUTLINE_MATERIAL = "StoragePodOutline_MAT"
OUTLINE_WIDTH = 0.04
# The sockets (UE units and rotators, on the mesh's origin) and the interact spot.
WEAPON_SOCKET = ((-65.0, 0.0, 200.0), (-16384, 16384, -16384))
MISC_SOCKET = ((-113.39, 0.0, 250.0), (0, -16384, 0))
INTERACT_SPOT = (500.0, 0.0, 0.0)
# The steam its opening lets out (AP_StoragePodOpen's HmAnimNotify_PlayEffects, each
# ReloadSteam_Titan_FX on one of the mesh's FX sockets, at the notify's
# RelativeLocation and RelativeRotation in the socket's frame): the socket, its
# bone, its location and rotator, and the notify's (UE units, 65536ths of a turn).
# Each is a node under its bone, STEAM_NODE + the socket's name, its frame the
# socket's turned by the notify's: recreator.rs lets the steam out there at the
# notify's time.
STEAM = [
    ("TopLeftFXSocket", "L_Robo7_XT", (-124.22554, -51.96987, 79.0), (0, 16384, 0), (25.0, 25.0, -85.0), (-13653, 0, 0)),
    ("TopRightFXSocket", "L_Robo7_XT", (107.68434, -51.96987, 79.0), (0, 16384, 0), (-25.0, 25.0, -85.0), (-13653, 0, 0)),
    ("TopCenterFXSocket", "L_Robo8_XT", (0.0, 0.0, 0.0), (0, 0, 0), (0.0, 10.0, -20.0), (-8192, 16384, 0)),
    ("SideLeftFXSocket", "L_HandleRobo_XT", (0.0, 0.0, 0.0), (32768, 16384, 0), (0.0, 0.0, 0.0), (0, 0, 0)),
    ("SideRightFXSocket", "R_HandleRobo_XT", (0.0, 0.0, 0.0), (32768, 16384, 0), (0.0, 0.0, 0.0), (0, 0, 0)),
]
STEAM_NODE = "Steam_"
P = np.array([[1, 0, 0], [0, 0, 1], [0, 1, 0]], float)


def bevy_pos(v):
    return [v[0] * 0.01, v[2] * 0.01, v[1] * 0.01]


def quat_from_matrix(m):
    """(x, y, z, w) of a column-vector rotation matrix."""
    t = m[0, 0] + m[1, 1] + m[2, 2]
    if t > 0:
        s = math.sqrt(t + 1.0) * 2
        w, x, y, z = 0.25 * s, (m[2, 1] - m[1, 2]) / s, (m[0, 2] - m[2, 0]) / s, (m[1, 0] - m[0, 1]) / s
    elif m[0, 0] > m[1, 1] and m[0, 0] > m[2, 2]:
        s = math.sqrt(1.0 + m[0, 0] - m[1, 1] - m[2, 2]) * 2
        w, x, y, z = (m[2, 1] - m[1, 2]) / s, 0.25 * s, (m[0, 1] + m[1, 0]) / s, (m[0, 2] + m[2, 0]) / s
    elif m[1, 1] > m[2, 2]:
        s = math.sqrt(1.0 + m[1, 1] - m[0, 0] - m[2, 2]) * 2
        w, x, y, z = (m[0, 2] - m[2, 0]) / s, (m[0, 1] + m[1, 0]) / s, 0.25 * s, (m[1, 2] + m[2, 1]) / s
    else:
        s = math.sqrt(1.0 + m[2, 2] - m[0, 0] - m[1, 1]) * 2
        w, x, y, z = (m[1, 0] - m[0, 1]) / s, (m[0, 2] + m[2, 0]) / s, (m[1, 2] + m[2, 1]) / s, 0.25 * s
    return [x, y, z, w]


def ue_rot(r):
    return lay.rotator_matrix(*r)[:3, :3]


def bevy_quat(R):
    """A row-vector UE rotation matrix as a Bevy quaternion."""
    return quat_from_matrix((P @ R @ P).T)


# ---------------------------------------------------------------------------
# The model
# ---------------------------------------------------------------------------

def find_export(export_dir):
    for pkg in sorted(os.listdir(export_dir)):
        path = os.path.join(export_dir, pkg, "SkeletalMesh3", MESH + ".gltf")
        if os.path.exists(path):
            return path
    return None


def outline_shell(w, doc, prim, material):
    """The body again as a shell, as it stands closed (skinned by its bones at
    rest, as build_map.py's Posed poses the maps' pods): each corner pushed out by
    OUTLINE_WIDTH along the average normal at its place (no cracks at hard edges),
    wound the other way round so that only its inside, behind the body, is drawn."""
    at = prim["attributes"]
    G = node_globals(doc.g)
    skin = doc.g["skins"][0]
    ibm = doc.acc(skin["inverseBindMatrices"]).reshape(-1, 4, 4).transpose(0, 2, 1)
    J = np.stack([G[j] @ ibm[k] for k, j in enumerate(skin["joints"])])
    M = np.einsum("vk,vkij->vij", doc.acc(at["WEIGHTS_0"]).astype(np.float64),
                  J[doc.acc(at["JOINTS_0"]).astype(int)])
    raw = doc.acc(at["POSITION"]).astype(np.float64)
    pos = np.einsum("vij,vj->vi", M, np.concatenate([raw, np.ones((len(raw), 1))], 1))[:, :3]
    nrm = np.einsum("vij,vj->vi", M[:, :3, :3], doc.acc(at["NORMAL"]).astype(np.float64))
    nrm /= np.maximum(np.linalg.norm(nrm, axis=1, keepdims=True), 1e-9)
    idx = doc.acc(prim["indices"]).reshape(-1, 3).astype(np.int64)
    key = np.round(pos / 1e-4).astype(np.int64)
    _, group = np.unique(key, axis=0, return_inverse=True)
    group = group.reshape(-1)
    smooth = np.zeros((group.max() + 1, 3))
    np.add.at(smooth, group, nrm)
    smooth /= np.maximum(np.linalg.norm(smooth, axis=1, keepdims=True), 1e-9)
    n = smooth[group]
    shell = pos + n * OUTLINE_WIDTH
    return {"attributes": {"POSITION": w.accessor(shell.astype(np.float32), 34962, minmax=True),
                           "NORMAL": w.accessor((-n).astype(np.float32), 34962)},
            "indices": w.accessor(idx[:, [0, 2, 1]].reshape(-1).astype(np.uint32), 34963),
            "material": material}


def build_model(cooked, export_dir, assets):
    gltf = find_export(export_dir)
    if gltf is None:
        sys.exit(f"no {MESH}.gltf under {export_dir}/*/SkeletalMesh3")
    doc = Doc(gltf)
    models = os.path.join(assets, "models")
    tex_src = os.path.join(models, TEXTURE_MAP)
    image_dir = os.path.join(models, "storage_pod")
    if os.path.isdir(image_dir):
        shutil.rmtree(image_dir)
    w = Writer(image_dir=image_dir, image_uri="storage_pod")
    w.g["extensionsUsed"] = ["KHR_materials_emissive_strength", "KHR_materials_unlit"]

    # The materials.
    def tex(name):
        path = os.path.join(tex_src, name + ".ktx2")
        if not os.path.exists(path):
            sys.exit(f"{path} missing: build {TEXTURE_MAP} first (build_map.py {TEXTURE_MAP})")
        return w.texture(name, open(path, "rb").read())

    mats = {}
    for name, (texset, glow, strength) in MATERIALS.items():
        mats[name] = w.material({
            "name": name,
            "pbrMetallicRoughness": {"baseColorTexture": {"index": tex(texset + "_c")},
                                     "metallicRoughnessTexture": {"index": tex(texset + "_mr")},
                                     "metallicFactor": 0.0, "roughnessFactor": 1.0},
            "normalTexture": {"index": tex(texset + "_n")},
            "emissiveTexture": {"index": tex(glow)},
            "emissiveFactor": [1, 1, 1],
            "extensions": {"KHR_materials_emissive_strength": {"emissiveStrength": strength}}})
    mats[GLASS] = w.material({
        "name": GLASS,
        "pbrMetallicRoughness": {"baseColorFactor": [0.036, 0.097, 0.062, 1.0], "metallicFactor": 0.0,
                                 "roughnessFactor": 1.0},
        "extensions": {"KHR_materials_unlit": {}}, "extras": {"foc": "nolight"}})
    outline_mat = w.material({
        "name": OUTLINE_MATERIAL,
        "pbrMetallicRoughness": {"baseColorFactor": [0, 0, 0, 1], "metallicFactor": 0.0, "roughnessFactor": 1.0},
        "emissiveFactor": [0, 1, 1],
        "extensions": {"KHR_materials_emissive_strength": {"emissiveStrength": 2.0}}})

    # The skeleton, under a root, then the mesh's node.
    root = w.node({"name": "AP_StoragePod", "children": []}, root=True)
    _, bone_root, joints = bc.copy_skeleton(w, doc, root)
    names = {w.g["nodes"][j]["name"]: j for j in joints}
    names.update({n["name"]: i for i, n in enumerate(w.g["nodes"]) if "name" in n})
    # The steam's sockets, on their bones (the bones' frames are Unreal's, P turned).
    for socket, bone, loc, rot, rel_loc, rel_rot in STEAM:
        R_socket = ue_rot(rot)
        R = ue_rot(rel_rot) @ R_socket
        at = np.array(loc) + np.array(rel_loc) @ R_socket
        node = w.node({"name": STEAM_NODE + socket, "translation": bevy_pos(at), "rotation": bevy_quat(R)})
        w.g["nodes"][names[bone]].setdefault("children", []).append(node)
    prims, outline = [], None
    for p in doc.g["meshes"][0]["primitives"]:
        mname = doc.g["materials"][p["material"]]["name"]
        at = p["attributes"]
        attrs = {"POSITION": w.accessor(doc.acc(at["POSITION"]).astype(np.float32), 34962, minmax=True),
                 "NORMAL": w.accessor(doc.acc(at["NORMAL"]).astype(np.float32), 34962),
                 "TEXCOORD_0": w.accessor(doc.acc(at["TEXCOORD_0"]).astype(np.float32), 34962),
                 "JOINTS_0": w.accessor(doc.acc(at["JOINTS_0"]).astype(np.uint16), 34962)}
        if "TANGENT" in at:
            attrs["TANGENT"] = w.accessor(doc.acc(at["TANGENT"]).astype(np.float32), 34962)
        wt = doc.acc(at["WEIGHTS_0"]).astype(np.float32)
        attrs["WEIGHTS_0"] = w.accessor(wt / np.maximum(wt.sum(1, keepdims=True), 1e-8), 34962)
        prims.append({"attributes": attrs,
                      "indices": w.accessor(doc.acc(p["indices"]).reshape(-1).astype(np.uint32), 34963),
                      "material": mats[mname]})
        if mname == OUTLINED:
            outline = outline_shell(w, doc, p, outline_mat)
    skin = doc.g["skins"][0]
    ibm = doc.acc(skin["inverseBindMatrices"]).astype(np.float32).reshape(-1, 16)
    w.g["skins"].append({"joints": joints, "inverseBindMatrices": w.accessor(ibm), "skeleton": bone_root})
    w.g["meshes"].append({"name": MESH, "primitives": prims})
    mesh_node = w.node({"name": MESH, "mesh": len(w.g["meshes"]) - 1, "skin": 0})
    w.g["nodes"][root]["children"].append(mesh_node)
    if outline is not None:
        w.g["meshes"].append({"name": OUTLINE_NODE, "primitives": [outline]})
        w.g["nodes"][root]["children"].append(w.node({"name": OUTLINE_NODE, "mesh": len(w.g["meshes"]) - 1}))

    # The opening, read from the map's package.
    pkg = Package(os.path.join(cooked, bm.MAPS[TEXTURE_MAP]["base"] + ".xxx"))
    tracks, dt = anim.clip(pkg, ANIM_SET, CLIP)
    del pkg
    rest = {w.g["nodes"][j]["name"]: (w.g["nodes"][j].get("translation", [0, 0, 0]),
                                     w.g["nodes"][j].get("rotation", [0, 0, 0, 1])) for j in joints}
    length = bc.base_clip(w, names, CLIP, tracks, dt, rest=rest, skip=())
    path = os.path.join(models, "storage_pod.glb")
    size = w.save(path)
    print(f"wrote {path} ({size / 1e6:.2f} MB): {len(prims)} parts, {len(joints)} bones, "
          f"{CLIP} {length:.2f} s ({len(tracks)} tracks)")


# ---------------------------------------------------------------------------
# Where the maps have them
# ---------------------------------------------------------------------------

def fmt(v):
    return " ".join(f"{x:.5g}" for x in v)


def baked_pods(glb):
    """How many pods the map's build draws itself (in their bind pose)."""
    if not os.path.exists(glb):
        return 0
    with open(glb, "rb") as f:
        f.read(12)
        length, _ = struct.unpack("<I4s", f.read(8))
        g = json.loads(f.read(length))
    return sum(n.get("name") == BAKED for n in g["nodes"])


def build_places(cooked, assets):
    for key, spec in bm.MAPS.items():
        pkg = Package(os.path.join(cooked, spec["base"] + ".xxx"))
        lines = [f"# {spec['title']}'s Armory Recreators (tools/build_pods.py): see its header"]
        # (The maps whose build has no pods have no collision for them either.)
        if baked_pods(os.path.join(assets, "models", key + ".glb")) == 0:
            lines.append("collide")
        for i, e in enumerate(pkg.exports, 1):
            if pkg.class_name(e) != "TnInteractLootBox" or not pkg.full_name(i).startswith("TheWorld."):
                continue
            pr = bm.actor_props(pkg, e)
            loc = np.array(bm.vec3(pr, "Location"), float)
            R = ue_rot(bm.rot3(pr))
            ds = pr.get("DrawScale", 1.0) or 1.0
            ds3 = bm.vec3(pr, "DrawScale3D", (1.0, 1.0, 1.0))
            scale = [ds * ds3[0], ds * ds3[2], ds * ds3[1]]
            frames = []
            for (sloc, srot) in (WEAPON_SOCKET, MISC_SOCKET):
                at = loc + (np.array(sloc) * ds) @ R
                frames.append(fmt(bevy_pos(at)) + "\t" + fmt(bevy_quat(ue_rot(srot) @ R)))
            spot = loc + np.array(INTERACT_SPOT) @ R
            lines.append("\t".join(["pod", e["name"], fmt(bevy_pos(loc)), fmt(bevy_quat(R)), fmt(scale)]
                                   + frames + [fmt(bevy_pos(spot))]))
        del pkg
        path = os.path.join(assets, "levels", f"{key}_pods.txt")
        open(path, "w").write("\n".join(lines) + "\n")
        print(f"wrote {path} ({sum(line.startswith('pod') for line in lines)} pods)")


if __name__ == "__main__":
    if len(sys.argv) != 4:
        sys.exit(__doc__)
    build_model(sys.argv[1], sys.argv[2], sys.argv[3])
    build_places(sys.argv[1], sys.argv[3])
