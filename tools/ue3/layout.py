"""Extract static mesh placements from FoC level packages.

Output JSON: list of {"mesh": "<package path of StaticMesh>", "matrix": 16 floats (UE row-major,
UE units), "source": actor name, "materials": [material paths or null], "component": export index of
the StaticMeshComponent in "package" (its baked lighting is read from there by lighting.py)}
"""
import json, math, os, struct, sys
import numpy as np
from ue3pkg import Package, Reader
from props import read_props


def vec(data):
    return struct.unpack("<fff", data[:12])


def rotator_matrix(pitch, yaw, roll):
    """UE3 FRotationMatrix (row-major, row vectors)."""
    k = math.pi * 2 / 65536.0
    p, y, r = pitch * k, yaw * k, roll * k
    sp, cp, sy, cy, sr, cr = math.sin(p), math.cos(p), math.sin(y), math.cos(y), math.sin(r), math.cos(r)
    m = np.eye(4)
    m[0, :3] = [cp * cy, cp * sy, sp]
    m[1, :3] = [sr * sp * cy - cr * sy, sr * sp * sy + cr * cy, -sr * cp]
    m[2, :3] = [-(cr * sp * cy + sr * sy), cy * sr - cr * sp * sy, cr * cp]
    return m


def props_any(pkg, e, starts):
    """Objects may have a state frame before their properties; try the known layouts."""
    last = None
    for st in starts:
        r = Reader(pkg.data, e["offset"] + st)
        try:
            pr = read_props(pkg, r, e["offset"] + e["size"])
            if r.o <= e["offset"] + e["size"]:
                return pr, r
        except Exception as ex:
            last = ex
    raise ValueError(f"cannot parse {e['name']}: {last}")


def component_info(pkg, idx):
    e = pkg.exports[idx - 1]
    # TemplateOwnerClass + NetIndex (+ TemplateName when the component is a template)
    try:
        pr, r = props_any(pkg, e, (8, 16, 4, 12))
    except ValueError as ex:
        print("  skip component", ex)
        return None, [], {}
    mesh = pr.get("StaticMesh")
    mesh = pkg.full_name(mesh[1]) if mesh and mesh[1] else None
    mats = []
    if "Materials" in pr:
        data = pr["Materials"][1]
        rr = Reader(data)
        n = rr.i32()
        for _ in range(n):
            o = rr.i32()
            mats.append(pkg.full_name(o) if o else None)
    return mesh, mats, pr


def component_matrix(cpr):
    """A primitive component's own transform (Scale, Scale3D, Rotation, Translation)."""
    s = cpr.get("Scale", 1.0)
    s3 = vec(cpr["Scale3D"][2]) if "Scale3D" in cpr else (1, 1, 1)
    rot = struct.unpack("<iii", cpr["Rotation"][2][:12]) if "Rotation" in cpr else (0, 0, 0)
    m = np.diag([s * s3[0], s * s3[1], s * s3[2], 1.0]) @ rotator_matrix(*rot)
    if "Translation" in cpr:
        m[3, :3] = vec(cpr["Translation"][2])
    return m


def mesh_section_materials(pkg, idx):
    """The materials of a StaticMesh export's sections (its first LOD), in section
    order, as object indices (negative: imports; 0: none). umodel names a section
    whose material it can't load (one the art package takes from the gameplay
    package, say) "dummy_material_<n>"; this reads what the mesh names."""
    e = pkg.exports[idx - 1]
    _, r = props_any(pkg, e, (4, 0, 8))
    d = pkg.data
    for _ in range(2):  # FoC: the kDOP tree's nodes and triangles (element size, count, data)
        size, n = struct.unpack_from("<ii", d, r.o)
        r.o += 8 + size * n
    if r.i32() <= 0:  # LODs
        return []
    # The LOD's raw triangles: a bulk data header (flags, count, size, offset and FoC's
    # key), then the data when it is inline.
    flags, _count, on_disk, offset, _key = struct.unpack_from("<iiiii", d, r.o)
    r.o += 20
    if not flags & 0x20 and offset == r.o:
        r.o += on_disk
    out = []
    for _ in range(r.i32()):
        out.append(r.i32())  # material
        r.o += 4 * 8  # flags, shadow casting, first index, triangles, min/max vertex, index
        r.o += 8 * r.i32()  # fragments
    return out


def extract(path):
    pkg = Package(path)
    out = []
    for i, e in enumerate(pkg.exports):
        cls = pkg.class_name(e)
        if cls == "StaticMeshCollectionActor":
            r = Reader(pkg.data, e["offset"] + 4)  # NetIndex
            pr = read_props(pkg, r, e["offset"] + e["size"])
            comps = []
            if "StaticMeshComponents" in pr:
                rr = Reader(pr["StaticMeshComponents"][1])
                comps = [rr.i32() for _ in range(rr.i32())]
            for c in comps:
                # Each component's CachedParentToWorld: the actor's rotation and
                # location. The actor's draw scale moved into the component's
                # Scale3D when the cooker collected it, and the component's own
                # transform (FScaleRotationTranslationMatrix) applies first.
                m = np.array(struct.unpack_from("<16f", pkg.data, r.o), np.float64).reshape(4, 4); r.o += 64
                if c <= 0:
                    continue
                mesh, mats, cpr = component_info(pkg, c)
                if mesh:
                    m = component_matrix(cpr) @ m
                    out.append({"mesh": mesh, "matrix": m.ravel().tolist(), "source": e["name"], "materials": mats, "component": c})
        elif cls in ("StaticMeshActor", "StaticInterpActor", "InterpActor", "HmStaticMeshActor", "KActor"):
            try:
                pr, r = props_any(pkg, e, (4, 30))
            except ValueError as ex:
                print("  skip", ex)
                continue
            loc = vec(pr["Location"][2]) if "Location" in pr else (0, 0, 0)
            rot = struct.unpack("<iii", pr["Rotation"][2][:12]) if "Rotation" in pr else (0, 0, 0)
            ds = pr.get("DrawScale", 1.0)
            ds3 = vec(pr["DrawScale3D"][2]) if "DrawScale3D" in pr else (1, 1, 1)
            comp = pr.get("StaticMeshComponent")
            if not comp or comp[1] <= 0:
                continue
            mesh, mats, cpr = component_info(pkg, comp[1])
            if not mesh:
                continue
            cs = cpr.get("Scale", 1.0)
            cs3 = vec(cpr["Scale3D"][2]) if "Scale3D" in cpr else (1, 1, 1)
            s = np.diag([ds * ds3[0] * cs * cs3[0], ds * ds3[1] * cs * cs3[1], ds * ds3[2] * cs * cs3[2], 1.0])
            m = s @ rotator_matrix(*rot)
            m[3, :3] = loc
            out.append({"mesh": mesh, "matrix": m.ravel().tolist(), "source": e["name"], "materials": mats, "class": cls,
                        "component": comp[1]})
    return pkg, out


if __name__ == "__main__":
    allp = []
    for path in sys.argv[2:]:
        pkg, items = extract(path)
        for it in items:
            it["package"] = os.path.basename(path)
        print(path, len(items), "placements")
        allp += items
    json.dump(allp, open(sys.argv[1], "w"))


def read_struct_array(pkg, data):
    """Array of structs: count, then each element is a tagged property list."""
    r = Reader(data)
    n = r.i32()
    out = []
    for _ in range(n):
        out.append(read_props(pkg, r, len(data)))
    return out


def material_texsets(pkg):
    """Map material object name -> TextureSet object name (following Parent links)."""
    direct, parent = {}, {}
    for i, e in enumerate(pkg.exports):
        if pkg.class_name(e) not in ("MaterialInstanceConstant",):
            continue
        try:
            pr, _ = props_any(pkg, e, (4, 0, 8))
        except ValueError:
            continue
        name = e["name"]
        if "Parent" in pr and pr["Parent"][1]:
            parent[name] = pkg.obj_name(pr["Parent"][1])
        if "TextureSetParameterValues" in pr:
            # The set named "TextureSet" is the material's own; the others are
            # layers over it (the Ash shader's TextureSet_RustAsh, say).
            try:
                sets = [(el.get("ParameterName"), el["ParameterValue"][1])
                        for el in read_struct_array(pkg, pr["TextureSetParameterValues"][1])
                        if el.get("ParameterValue") and el["ParameterValue"][1]]
                sets.sort(key=lambda s: s[0] != "TextureSet")
                if sets:
                    direct[name] = pkg.obj_name(sets[0][1])
            except Exception:
                pass
    result = {}
    for name in set(direct) | set(parent):
        cur, seen = name, set()
        while cur and cur not in direct and cur not in seen:
            seen.add(cur)
            cur = parent.get(cur)
        if cur in direct:
            result[name] = direct[cur]
    return result
