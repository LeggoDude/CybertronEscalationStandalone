"""Reads a Texture2D (or a subclass: TextureFlipBook, LightMapTexture2D) straight from a
FoC package: its largest mip, decoded to RGBA. umodel exports most textures; this is
for the classes it skips (flip books).

A cooked map keeps only a texture's small mips; the large ones stay in the package the
texture came from (the outermost package of its path, e.g. ENV_ASH_Maproom_p.xxx), in
the CookedPC folder.

    read_texture(pkg, index, cooked_dir) -> (RGBA uint8 array, mip count)
"""
import os, struct, sys, zlib

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from ue3pkg import Package  # noqa: E402
from layout import props_any  # noqa: E402

sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), ".."))
import ktx2_io  # noqa: E402

FORMATS = {"PF_DXT1": "bc1", "PF_DXT5": "bc3", "PF_ATI2": "bc5", "PF_BC5": "bc5"}
SEPARATE_FILE, ZLIB, UNUSED = 0x01, 0x02, 0x20
_packages = {}


def _package(path):
    if path not in _packages:
        _packages[path] = Package(path)
    return _packages[path]


def _zlib_bulk(data, off):
    """UE3's compressed bulk data: a tag, chunk size, totals, the chunk table, then
    the zlib chunks."""
    tag, _chunk, csize, usize = struct.unpack_from("<IIII", data, off)
    assert tag == 0x9E2A83C1, "not compressed bulk data"
    p = off + 16
    sizes = []
    left = usize
    while left > 0:
        c, u = struct.unpack_from("<II", data, p)
        p += 8
        sizes.append(c)
        left -= u
    out = bytearray()
    for c in sizes:
        out += zlib.decompress(data[p:p + c])
        p += c
    return bytes(out)


def _outermost(pkg, idx):
    while pkg.exports[idx - 1]["outer"] > 0:
        idx = pkg.exports[idx - 1]["outer"]
    return pkg.exports[idx - 1]["name"]


# Uncompressed formats: bytes per pixel.
PLAIN = {"PF_A8R8G8B8": 4, "PF_G8": 1}


def _plain(fmt, w, h, raw):
    """An uncompressed mip as RGBA (A8R8G8B8 is stored B, G, R, A)."""
    import numpy as np
    if fmt == "PF_G8":
        g = np.frombuffer(raw, np.uint8)[: w * h].reshape(h, w)
        return np.stack([g, g, g, np.full_like(g, 255)], -1)
    bgra = np.frombuffer(raw, np.uint8)[: w * h * 4].reshape(h, w, 4)
    return bgra[..., [2, 1, 0, 3]].copy()


def read_cube(pkg, idx):
    """A TextureCube kept in its package (a level's captured cube map, its six faces
    inline): (pixel format, edge size, [[face mip bytes, largest first] for +X, -X,
    +Y, -Y, +Z, -Z])."""
    e = pkg.exports[idx - 1]
    pr, r = props_any(pkg, e, (4, 0, 8))
    d = pkg.data
    faces = []
    for _ in range(6):
        mips = []
        for _ in range(r.i32()):
            flags, count, on_disk, offset, _key = struct.unpack_from("<iiiii", d, r.o)
            r.o += 20
            if flags & (SEPARATE_FILE | UNUSED) or offset != r.o:
                raise ValueError(f"{e['name']}: a face's mip isn't kept with it")
            raw = d[r.o:r.o + on_disk]
            r.o += on_disk + 8
            if flags & ZLIB:
                raw = _zlib_bulk(raw, 0)
            mips.append(raw)
        faces.append(mips)
    return pr.get("Format"), pr.get("EdgeSize", 0), faces


def read_texture(pkg, idx, cooked_dir):
    """The texture's largest mip with data (from the package the texture came from
    when that is in `cooked_dir`, else the largest the map keeps itself)."""
    e = pkg.exports[idx - 1]
    pr, r = props_any(pkg, e, (4, 0, 8))
    plain = pr.get("Format") in PLAIN
    fmt = FORMATS.get(pr.get("Format"))
    if fmt is None and not plain:
        raise ValueError(f"{e['name']}: unsupported format {pr.get('Format')}")
    d = pkg.data
    mips = []
    for _ in range(r.i32()):
        flags, count, on_disk, offset, _key = struct.unpack_from("<iiiii", d, r.o)
        r.o += 20
        inline = None
        if not flags & (SEPARATE_FILE | UNUSED) and offset == r.o:
            inline = d[r.o:r.o + on_disk]
            r.o += on_disk
        w, h = struct.unpack_from("<ii", d, r.o)
        r.o += 8
        mips.append((flags, count, on_disk, offset, inline, w, h))
    for flags, count, on_disk, offset, inline, w, h in mips:
        if flags & UNUSED or count == 0:
            continue
        if inline is not None:
            raw = inline
        else:
            path = os.path.join(cooked_dir, _outermost(pkg, idx) + ".xxx")
            if not os.path.exists(path):
                continue
            src = _package(path)
            raw = src.data[offset:offset + on_disk]
        if flags & ZLIB:
            raw = _zlib_bulk(raw, 0)
        if plain:
            return _plain(pr.get("Format"), w, h, raw), len(mips)
        return ktx2_io.decode_level(fmt, w, h, raw), len(mips)
    raise ValueError(f"{e['name']}: no mip with data")
