"""Turns the collision triangles of a map's mirrored placements the right way round.

A mesh placed with a mirroring scale (a negative determinant: 134 of Ancients' 1100
rocks and props) is drawn as the game draws it (glTF turns its triangles round), but
build_map.py (before this was fixed there) wrote its collision triangles with their
corners in the mesh's own order, which a mirror turns inside out: their normals,
by the order of their corners, point into the rock. The demo's movement tells a
rock's underside from its top by them (collision.rs: a surface facing down is never
stood on from below, and stops the body under it), so they must face out.

This puts that right in a map's levels/<map>_col.bin from its models/<map>.glb, as
remove_console.py finds a console's triangles: each mirrored placement's collision is
made again from its mesh in the model (build_level.collision_mesh, as build_map.py
makes it), found among the file's solid triangles corner for corner (to a step of the
file's either way: the model keeps its placements' matrices as 32-bit floats), and
turned round (two corners swapped). A file already put right (or built by a
build_map.py that does it) is left as it is: its triangles are found turned round
already. A triangle the file has both ways round (a two-sided surface) is left so.

usage: python col_winding.py <foc_esc\\assets> <map> [<map>...]
       (the maps: downfall, oblivion, ancients, ignition; a file is only rewritten if
       something in it was turned round)
"""
import os
import sys

import numpy as np

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
import build_level as bl  # noqa: E402
import build_map as bm  # noqa: E402
from gltf_io import Doc, node_globals  # noqa: E402
from remove_console import MeshDoc, read_col, write_col  # noqa: E402


def mirrored_placements(doc):
    """The model's placed meshes with a mirroring scale that have collision (as
    build_map.py gives it: not SKIP_COLLIDE's), as (name, mesh, matrix)."""
    G = node_globals(doc.g)
    out = []
    for i, M in G.items():
        n = doc.g["nodes"][i]
        name = n.get("name", "")
        if "mesh" not in n or name.startswith(("bsp_", "Terrain")):
            continue
        if any(s in name for s in bm.SKIP_COLLIDE):
            continue
        if np.linalg.det(M[:3, :3]) < 0:
            out.append((name.split("@")[0], n["mesh"], M))
    return out


CELL = 4  # the triangles' middles bucketed by this many of the file's steps
# A triangle's corners the other way round, from each of them.
BACK = ([0, 2, 1], [2, 1, 0], [1, 0, 2])


class Rows:
    """A file's triangles, found by their corners to a step either way."""

    def __init__(self, tris):
        self.tris = tris.astype(np.int32)
        cells = np.floor_divide(self.tris.reshape(-1, 3, 3).sum(1), 3 * CELL)
        self.cells = {}
        for j, c in enumerate(map(tuple, cells.tolist())):
            self.cells.setdefault(c, []).append(j)

    def find(self, r):
        """The triangles with corners (in that order) within a step of row `r`'s."""
        c = np.floor_divide(r.reshape(3, 3).sum(0), 3 * CELL)
        out = []
        for dx in (-1, 0, 1):
            for dy in (-1, 0, 1):
                for dz in (-1, 0, 1):
                    for j in self.cells.get((c[0] + dx, c[1] + dy, c[2] + dz), ()):
                        if np.abs(self.tris[j] - r).max() <= 1:
                            out.append(j)
        return out


def fix(assets, key):
    """Turns round levels/<key>_col.bin's triangles of the mirrored placements; the
    counts (the file's triangles turned round, the placements' found the right way
    round already, theirs not in the file: left out of it by build_map.py's area)."""
    glb = os.path.join(assets, "models", f"{key}.glb")
    col = os.path.join(assets, "levels", f"{key}_col.bin")
    doc = Doc(glb)
    origin, step, tris, vol = read_col(col)
    rows = Rows(tris)
    votes = {}
    already = missing = 0
    for name, mesh, M in mirrored_placements(doc):
        A, t = M[:3, :3], M[:3, 3]
        cm = bl.collision_mesh(name, MeshDoc(doc, mesh), np.linalg.norm(A, axis=0).max())
        if cm is None:
            continue
        v = cm[0] @ A.T + t
        mine = v[cm[1].reshape(-1)].astype(np.float32).reshape(-1, 9)
        q = np.round((mine.reshape(-1, 3) - origin) / step).astype(np.int32).reshape(-1, 9)
        for r in q:
            ahead = rows.find(r)
            back = [j for o in BACK for j in rows.find(r.reshape(3, 3)[o].reshape(-1))]
            # (A triangle two placements share, one of them mirrored the other way,
            # goes as most of them have it.)
            for j in ahead:
                votes[j] = votes.get(j, 0) + 1
            for j in back:
                votes[j] = votes.get(j, 0) - 1
            if back and not ahead:
                already += 1
            elif not ahead:
                missing += 1
    flip = [j for j, v in votes.items() if v > 0]
    if flip:
        idx = np.array(sorted(flip))
        t3 = tris.reshape(-1, 3, 3)
        t3[idx] = t3[idx][:, [0, 2, 1]]
        write_col(col, origin, step, t3.reshape(-1, 9), vol)
    return len(flip), already, missing


def main(argv):
    if len(argv) < 3:
        print(__doc__)
        return 1
    assets = argv[1]
    for key in argv[2:]:
        turned, already, missing = fix(assets, key)
        print(f"{key}: {turned} triangles of mirrored placements turned round, "
              f"{already} already the right way round, {missing} not in the file")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
