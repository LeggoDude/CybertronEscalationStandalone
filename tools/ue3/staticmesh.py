"""What a Fall of Cybertron StaticMesh's first level of detail holds, as far as the
demo needs to know: whether it has vertex colours of its own.

UE Viewer (umodel) gives a static mesh without them colours anyway, out of memory
it never fills in (FoC's UV items hold no colour, so FStaticMeshUVItem3's Color is
left as it was allocated: UnMesh3.cpp's "Lod->VertexColors[i] = SUV.Color"), which
can come out black, or random, depending on the build of umodel and the machine:
an effect mesh that multiplies by its vertex colour then draws nothing, or noise.
The game draws a mesh without colours white. So the builds read the package to
know (levelfx.LevelFx.mesh_has_colours).

A LOD's streams, as umodel reads them for FoC (UnMesh3.cpp, FStaticMeshLODModel3
under GAME_Transformers), little-endian:
  positions  u32 item size (12), u32 vertex count n, then a bulk array: u32 item
             size (12), u32 count (n), n x 3 f32
  UVs        u32 texcoord count, u32 item size, u32 n, u32 full precision flag,
             then a bulk array: u32 item size, u32 count (n), the items
  colours    u32 item size, u32 count (0 or n), then (if not 0) a bulk array:
             u32 item size (4), u32 count, the colours (BGRA bytes)
"""
import struct

TWELVE = struct.pack("<I", 12)


def lod_colours(data):
    """The vertex count and the colour count (0: none) of the first level of detail
    in a StaticMesh export's bytes, found by its streams' headers; None if they
    can't be found."""
    at = 0
    while True:
        o = data.find(TWELVE, at)
        if o < 0 or o + 16 > len(data):
            return None
        at = o + 1
        _, n, size, m = struct.unpack_from("<4I", data, o)
        if size != 12 or n != m or not 0 < n < 4_000_000:
            continue
        p = o + 16 + 12 * n
        if p + 32 > len(data):
            continue
        _tc, item, nv, _full, item2, count = struct.unpack_from("<6I", data, p)
        if nv != n or count != n or item2 != item or not 0 < item <= 64:
            continue
        q = p + 24 + item * n
        if q + 8 > len(data):
            continue
        _csize, colours = struct.unpack_from("<2I", data, q)
        if colours not in (0, n):
            continue
        return n, colours


def has_colours(pkg, idx):
    """Whether StaticMesh export `idx` of package `pkg` (ue3pkg.Package) has vertex
    colours of its own; None if its streams can't be found."""
    e = pkg.exports[idx - 1]
    found = lod_colours(pkg.data[e["offset"]:e["offset"] + e["size"]])
    return None if found is None else found[1] > 0
