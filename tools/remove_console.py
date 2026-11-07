"""Takes Escalation's weapon upgrade console out of built maps.

The demo's weapons take their upgrades from the Customize build, as multiplayer's do
(src/upgrades.rs), not from Escalation's console (TnInteractSurvivalWeaponUpgradeConsole,
AP_EscalationConsoleWeapon_SKEL), which tools/build_map.py now leaves out
(SKIPPED_ACTORS). This takes it out of maps built before that, in place:

  * models/<map>.glb - its node comes out of the scene (its mesh stays in the file,
    unused; the glTF's binary is kept as it is)
  * levels/<map>_col.bin - its collision triangles come out (the ones
    build_map.py made of it: build_level.collision_mesh of its mesh where it stood,
    found again in the file's own quantization)

usage: python remove_console.py <foc_esc\\assets> <map> [<map> ...]
"""
import json
import os
import struct
import sys

import numpy as np

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
sys.path.insert(0, os.path.join(HERE, "ue3"))
import build_level as bl  # noqa: E402
from gltf_io import Doc  # noqa: E402

CONSOLE = "AP_EscalationConsoleWeapon_SKEL"


def read_glb(path):
    data = open(path, "rb").read()
    magic, version, _length = struct.unpack_from("<4sII", data, 0)
    assert magic == b"glTF" and version == 2
    jlen, jtype = struct.unpack_from("<I4s", data, 12)
    assert jtype == b"JSON"
    g = json.loads(data[20:20 + jlen])
    rest = data[20 + jlen:]
    return g, rest


def write_glb(path, g, rest):
    j = json.dumps(g, separators=(",", ":")).encode()
    j += b" " * (-len(j) % 4)
    out = struct.pack("<4sII", b"glTF", 2, 12 + 8 + len(j) + len(rest))
    out += struct.pack("<I4s", len(j), b"JSON") + j + rest
    with open(path, "wb") as fh:
        fh.write(out)


class MeshDoc:
    """One mesh of a Doc, as build_level.collision_mesh reads a umodel export."""

    def __init__(self, doc, mesh):
        self.g = {"meshes": [doc.g["meshes"][mesh]], "accessors": doc.g["accessors"]}
        self.acc = doc.acc


def read_col(path):
    """A collision file (build_map.py's write_collision): its origin and step, its
    solid triangles, and its volumes' (movement only, players only: version 2's
    files have none of the latter), quantized as the file has them."""
    data = open(path, "rb").read()
    assert data[:4] == b"FOCC"
    version, ox, oy, oz, step = struct.unpack_from("<I3ff", data, 4)
    assert version in (2, 3)
    off = 4 + 20
    sets = []
    for _ in range(3 if version == 3 else 2):
        (n,) = struct.unpack_from("<I", data, off)
        off += 4
        sets.append(np.frombuffer(data, "<i2", n * 9, off).reshape(-1, 9).copy())
        off += n * 18
    if len(sets) == 2:
        sets.append(np.zeros((0, 9), np.int16))
    return np.array([ox, oy, oz], np.float32), step, sets[0], (sets[1], sets[2])


def write_col(path, origin, step, tris, vol):
    """Writes a collision file (version 3) of read_col's parts: `vol` the volumes'
    (movement only, players only)."""
    movement, players = vol
    with open(path, "wb") as fh:
        fh.write(b"FOCC")
        fh.write(struct.pack("<I3ff", 3, *origin.tolist(), step))
        for t in (tris, movement, players):
            fh.write(struct.pack("<I", len(t)))
            fh.write(np.asarray(t).astype("<i2").tobytes())


def corner_sets(q):
    """Each quantized triangle's corners as a key that ignores their order."""
    c = q.reshape(-1, 3, 3)
    order = np.lexsort((c[:, :, 2], c[:, :, 1], c[:, :, 0]))
    return [tuple(c[i][order[i]].reshape(-1).tolist()) for i in range(len(c))]


def remove(assets, key):
    glb = os.path.join(assets, "models", f"{key}.glb")
    col = os.path.join(assets, "levels", f"{key}_col.bin")
    doc = Doc(glb)
    g, rest = read_glb(glb)
    found = [i for i, n in enumerate(g["nodes"]) if n.get("name", "").startswith(CONSOLE)]
    if not found:
        print(f"{key}: no {CONSOLE}")
        return
    origin, step, tris, vol = read_col(col)
    keys = corner_sets(tris)
    drop = set()
    for i in found:
        n = g["nodes"][i]
        T = np.array(n["matrix"], np.float64).reshape(4, 4).T
        A, t = T[:3, :3], T[:3, 3]
        cm = bl.collision_mesh(n["name"], MeshDoc(doc, n["mesh"]), np.linalg.norm(A, axis=0).max())
        if cm is not None:
            v = cm[0] @ A.T + t
            mine = v[cm[1].reshape(-1)].astype(np.float32).reshape(-1, 9)
            q = np.round((mine.reshape(-1, 3) - origin) / step).astype(np.int64).reshape(-1, 9)
            want = set(corner_sets(q))
            # (Rounded the other way by a hair: a corner a step off still matches.)
            hits = {j for j, k in enumerate(keys) if k in want}
            if len(hits) < len(want):
                near = []
                for j, k in enumerate(keys):
                    if j in hits:
                        continue
                    kk = np.array(k).reshape(3, 3)
                    for w in want:
                        if np.abs(kk - np.array(w).reshape(3, 3)).max() <= 1:
                            near.append(j)
                            break
                hits |= set(near)
            drop |= hits
            print(f"{key}: node {i} at {np.round(t, 2).tolist()}: {len(hits)} of its {len(want)} collision triangles found")
        for parent in g["nodes"]:
            if i in parent.get("children", []):
                parent["children"].remove(i)
        for scene in g["scenes"]:
            if i in scene["nodes"]:
                scene["nodes"].remove(i)
    keep = np.array([j not in drop for j in range(len(tris))], bool)
    write_col(col, origin, step, tris[keep], vol)
    write_glb(glb, g, rest)
    print(f"{key}: {len(found)} console(s) out of the scene, {len(drop)} collision triangles out "
          f"({keep.sum()} left)")


if __name__ == "__main__":
    if len(sys.argv) < 3:
        sys.exit(__doc__)
    for m in sys.argv[2:]:
        remove(sys.argv[1], m)
