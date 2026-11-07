"""Extract BSP geometry (UModel) from a FoC level package.

Layout (FoC, UE3 v846/181), after a 4-byte NetIndex + "None" property terminator:
  FBoxSphereBounds(28) | Vectors bulk(12) | Points bulk(12) | Nodes bulk(64)
  | Surfs TTransArray(owner, count, 60 bytes each) | Verts bulk(16) | ...
"""
import struct, sys, json
import numpy as np
from ue3pkg import Package


def bulk(d, o, esize):
    es, n = struct.unpack_from("<ii", d, o)
    assert es == esize, (o, es, esize)
    return o + 8, n, o + 8 + es * n


def extract(path, name=None, with_shadow_uv=False, pkg=None, index=None):
    """BSP data of the level's Model (the largest of its PersistentLevel's, or the one
    called `name`). With `with_shadow_uv` the result also holds each
    vertex's ShadowTexCoord: its position in the light map of the node's ModelComponent
    element (see lighting.py). A Model can also be picked by export `index` (1-based) in
    an already open `pkg` (volumes' brushes share names)."""
    pkg = pkg or Package(path)
    if index is not None:
        e = pkg.exports[index - 1]
    elif name is None:
        e = max((e for e in pkg.exports if pkg.class_name(e) == "Model" and pkg.obj_name(e["outer"]) == "PersistentLevel"),
                key=lambda e: e["size"])
    else:
        e = next(e for e in pkg.exports if e["name"] == name and pkg.class_name(e) == "Model")
    d = pkg.data[e["offset"]:e["offset"] + e["size"]]
    o = 8 + 28 + 4  # netindex, None, bounds(28) -> vectors start at 40
    vo, nv, o = bulk(d, o, 12)
    vectors = np.frombuffer(d, "<f4", nv * 3, vo).reshape(-1, 3)
    po, npnt, o = bulk(d, o, 12)
    points = np.frombuffer(d, "<f4", npnt * 3, po).reshape(-1, 3)
    no, nn, o = bulk(d, o, 64)
    nodes = []
    for i in range(nn):
        b = no + i * 64
        plane = struct.unpack_from("<4f", d, b)
        vpool, isurf = struct.unpack_from("<ii", d, b + 16)
        nverts = d[b + 54]
        nodes.append((vpool, isurf, nverts, plane))
    o += 4  # owner
    ns = struct.unpack_from("<i", d, o)[0]; o += 4
    surfs = []
    for i in range(ns):
        mat, flags, pbase, vn, vu, vv, ibp, actor = struct.unpack_from("<iIiiiiii", d, o)
        surfs.append({"material": pkg.full_name(mat) if mat else None, "flags": flags,
                      "base": pbase, "normal": vn, "u": vu, "v": vv, "actor": actor})
        o += 60
    vo2, nvt, o = bulk(d, o, 16)
    # FVert: pVertex, iSide, ShadowTexCoord (2 floats)
    verts = np.frombuffer(d, "<i4", nvt * 4, vo2).reshape(-1, 4)[:, 0]
    if with_shadow_uv:
        shadow_uv = np.frombuffer(d, "<f4", nvt * 4, vo2).reshape(-1, 4)[:, 2:4]
        return pkg, vectors, points, nodes, surfs, verts, shadow_uv
    return pkg, vectors, points, nodes, surfs, verts


def triangles(vectors, points, nodes, surfs, verts, skip_flags=0x1):
    """Yields per-material lists of (positions, uvs, normal) triangles in UE space."""
    out = {}
    for vpool, isurf, nv, plane in nodes:
        if nv < 3 or isurf < 0:
            continue
        s = surfs[isurf]
        if s["flags"] & skip_flags:
            continue
        idx = verts[vpool:vpool + nv]
        if len(idx) < 3 or idx.max() >= len(points):
            continue
        P = points[idx].astype(np.float64)
        base = points[s["base"]]; tu = vectors[s["u"]]; tv = vectors[s["v"]]
        uv = np.stack([(P - base) @ tu, (P - base) @ tv], 1) / 128.0
        n = np.array(plane[:3])
        g = out.setdefault(s["material"], {"pos": [], "uv": [], "nrm": []})
        for k in range(1, nv - 1):
            for j in (0, k, k + 1):
                g["pos"].append(P[j]); g["uv"].append(uv[j]); g["nrm"].append(n)
    return out


if __name__ == "__main__":
    pkg, vectors, points, nodes, surfs, verts = extract(sys.argv[1])
    print(len(vectors), "vectors", len(points), "points", len(nodes), "nodes", len(surfs), "surfs", len(verts), "verts")
    flags = {}
    for s in surfs:
        flags[hex(s["flags"])] = flags.get(hex(s["flags"]), 0) + 1
    print("flags", flags)
    tris = triangles(vectors, points, nodes, surfs, verts)
    for m, g in sorted(tris.items(), key=lambda kv: -len(kv[1]["pos"])):
        p = np.array(g["pos"])
        print(len(p) // 3, m, p.min(0).round(), p.max(0).round())
