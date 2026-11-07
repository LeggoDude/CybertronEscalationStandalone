"""Builds the effect meshes in assets/models/fx from umodel exports.

The Ion Blaster's muzzle-flash core and tracer trail are mesh particles in the
game, drawn with FX_OS_Common_Meshes_p.Static.Ion_Cannon_MuzzleFlash_STAT: four
crossed fins, 2.24 m long and 1.12 m across, each carrying one of the four comet
streaks of Ion_Cannon_MuzzleFlash_Core_CLR (the comets' heads at the mesh's pivot).

Export it first:

    umodel -game=trans -export -path=<CookedPC> -out=<dir> L03b_CutAndRun_BASE_m.xxx Ion_Cannon_MuzzleFlash_STAT

then:

    python build_fx_meshes.py <dir>/L03b_CutAndRun_BASE_m/StaticMesh3/Ion_Cannon_MuzzleFlash_STAT.pskx <assets>/models/fx/ion_core.glb

Jazz's car jets (the pulses of CarJump_01_FX and CarHover_A_01_FX, and the
boost flames) are mesh particles too, drawn with FX_Navigation_p.Boostermesh_03:
a flared cone 0.5 m long, running along Unreal +X from its pivot. Export it the
same way (object Boostermesh_03) and pass --along x:

    python build_fx_meshes.py <dir>/L03b_CutAndRun_BASE_m/StaticMesh3/Boostermesh_03.pskx <assets>/models/fx/booster_cone.glb --along x

The glb holds one primitive, scaled to a unit size so the demo can size it like
its sprites: it runs from the pivot (y = 0) to y = 1 and is 1 across (x, z in
-0.5..0.5). The game's own size is printed.
"""
import os
import struct
import sys

import numpy as np

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from gltf_io import Writer  # noqa: E402


def read_psk(path):
    """Points, wedges (point, u, v) and triangles (wedge indices) of a .psk/.pskx."""
    d = open(path, "rb").read()
    o = 0
    chunks = {}
    while o + 32 <= len(d):
        name = d[o:o + 20].split(b"\0")[0].decode()
        _, size, count = struct.unpack_from("<iii", d, o + 20)
        o += 32
        chunks[name] = (size, count, d[o:o + size * count])
        o += size * count
    size, count, b = chunks["PNTS0000"]
    points = np.frombuffer(b, "<f4").reshape(count, 3).astype(np.float64)
    size, count, b = chunks["VTXW0000"]
    wedges = []
    for i in range(count):
        p, u, v = struct.unpack_from("<Iff", b, i * size)
        if size != 16:
            p &= 0xFFFF
        wedges.append((p, u, v))
    name = "FACE3200" if "FACE3200" in chunks else "FACE0000"
    size, count, b = chunks[name]
    fmt = "<3I" if name == "FACE3200" else "<3H"
    faces = [struct.unpack_from(fmt, b, i * size) for i in range(count)]
    return points, wedges, faces


def main(src, dst, along="y"):
    points, wedges, faces = read_psk(src)
    if along == "x":
        # Runs along Unreal +X from its pivot.
        length = points[:, 0].max()
        across = 2.0 * np.abs(points[:, [1, 2]]).max()
        pos = np.array([[points[p][1] / across, points[p][0] / length, points[p][2] / across]
                        for p, _, _ in wedges], np.float32)
    else:
        # umodel writes Unreal's Y negated; the mesh runs along Unreal +Y from its pivot.
        length = -points[:, 1].min()
        across = 2.0 * np.abs(points[:, [0, 2]]).max()
        pos = np.array([[points[p][0] / across, -points[p][1] / length, points[p][2] / across]
                        for p, _, _ in wedges], np.float32)
    uv = np.array([[u, v] for _, u, v in wedges], np.float32)
    idx = np.array(faces, np.uint16).reshape(-1)
    # Face normals summed per corner (flat fins keep their fin's normal).
    normal = np.zeros_like(pos)
    for a, b, c in faces:
        n = np.cross(pos[b] - pos[a], pos[c] - pos[a])
        normal[[a, b, c]] += n
    normal /= np.maximum(np.linalg.norm(normal, axis=1, keepdims=True), 1e-9)

    w = Writer()
    prim = {
        "attributes": {
            "POSITION": w.accessor(pos, 34962, minmax=True),
            "NORMAL": w.accessor(normal.astype(np.float32), 34962),
            "TEXCOORD_0": w.accessor(uv, 34962),
        },
        "indices": w.accessor(idx, 34963),
    }
    name = os.path.splitext(os.path.basename(src))[0]
    w.g["meshes"].append({"name": name, "primitives": [prim]})
    w.node({"name": name, "mesh": 0}, root=True)
    os.makedirs(os.path.dirname(os.path.abspath(dst)), exist_ok=True)
    w.save(dst)
    print(f"{dst}: {len(wedges)} vertices, {len(faces)} triangles; "
          f"game size {length / 100:.3f} m long, {across / 100:.3f} m across")


if __name__ == "__main__":
    args = [a for a in sys.argv[1:] if not a.startswith("--")]
    along = "x" if "--along" in sys.argv and sys.argv[sys.argv.index("--along") + 1] == "x" else "y"
    args = [a for a in args if a not in ("x", "y")]
    if len(args) != 2:
        sys.exit(__doc__)
    main(args[0], args[1], along)
