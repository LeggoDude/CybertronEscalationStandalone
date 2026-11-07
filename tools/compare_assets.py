"""Compares two assets folders (e.g. one tools/extract_game.py built into an empty
folder with the demo's own assets\\), file by file, telling the files that differ in
what they hold from those that only differ in how they were written.

usage: python compare_assets.py <new assets folder> <old assets folder> [<report.txt>]

Each file in either folder is one of:

  same        the same bytes
  equivalent  different bytes, the same content (the report says why), as far as
              the demo can tell: text with other line endings, numbers a rounding
              apart or the same lines in another order; pictures and textures within
              a step or two of each other in each colour (or within the noise of
              block compression); sounds that decode to the same audio within the
              noise of encoding (30 dB or better: needs ffmpeg); models (.glb) with
              the same nodes, meshes, materials, clips and skins, their numbers a
              rounding apart, whatever their order in the file, a bitangent sign
              aside (UE Viewer builds differ in the one they give some vertices);
              collision with the same triangles, effect meshes with the same
              numbers; movies the same length
  differs     different content (the report says how: for a picture or texture its
              size, for text the lines only in either...)
  new         only in the new folder
  old only    only in the old folder (often a file an earlier version of the demo
              used and nothing writes any more, or one of the demo's own that no
              script makes: its shaders)

It prints a table, folder by folder, and writes every file that isn't the same to
the report (compare_report.txt in the new folder by default), the differing ones
first. It needs numpy (as the build scripts do); Pillow for PNG pictures; ffmpeg
and ffprobe on the PATH for sounds and movies (without them those are only
compared byte for byte).
"""
import hashlib
import json
import os
import re
import struct
import subprocess
import sys
from collections import Counter, defaultdict

import numpy as np

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)

# How near two numbers (in text, models, effect meshes) count as the same: within
# this share of the larger (or this much near zero).
REL = 1e-4
ABS = 1e-5
# Sounds: the decoded audio's signal to the difference between them (dB).
SNR_DB = 30.0


def md5(path):
    h = hashlib.md5()
    with open(path, "rb") as f:
        for chunk in iter(lambda: f.read(1 << 20), b""):
            h.update(chunk)
    return h.hexdigest()


def close(a, b):
    return abs(a - b) <= max(ABS, REL * max(abs(a), abs(b)))


# ---------------------------------------------------------------------------
# Text
# ---------------------------------------------------------------------------

NUMBER = re.compile(r"[-+]?(?:\d+\.\d*|\.\d+|\d+)(?:[eE][-+]?\d+)?")


def text_lines(path):
    with open(path, "rb") as f:
        data = f.read()
    try:
        s = data.decode("utf-8")
    except UnicodeDecodeError:
        return None
    return [l.rstrip() for l in s.replace("\r\n", "\n").replace("\r", "\n").split("\n")]


def lines_close(a, b):
    """The same line but for numbers a rounding apart (and "-0" for "0")."""
    if a == b:
        return True
    if NUMBER.sub("#", a) != NUMBER.sub("#", b):
        return False
    na, nb = NUMBER.findall(a), NUMBER.findall(b)
    return len(na) == len(nb) and all(close(float(x), float(y)) for x, y in zip(na, nb))


def text(a, b):
    la, lb = text_lines(a), text_lines(b)
    if la is None or lb is None:
        return False, "not text"
    if la == lb:
        return True, "line endings"
    if len(la) == len(lb) and all(lines_close(x, y) for x, y in zip(la, lb)):
        return True, "numbers a rounding apart"
    # The same lines in another order (numbers rounded off for the match, those near
    # nothing taken as nothing).
    def rounded(m):
        x = float(m.group())
        return "0" if abs(x) < 1e-6 else f"{x:.4g}"

    key = lambda l: NUMBER.sub(rounded, l)
    ca, cb = Counter(map(key, la)), Counter(map(key, lb))
    if ca == cb:
        return True, "the same lines in another order"
    only_new = sum((ca - cb).values())
    only_old = sum((cb - ca).values())
    first = next(iter((ca - cb).elements()), None) or next(iter((cb - ca).elements()), "")
    return False, f"{only_new} lines only in the new, {only_old} only in the old (e.g. {first[:80]!r})"


# ---------------------------------------------------------------------------
# Pictures and textures
# ---------------------------------------------------------------------------

def pixels_compare(x, y):
    if x.shape != y.shape:
        return False, f"size {x.shape[1]}x{x.shape[0]} (new) vs {y.shape[1]}x{y.shape[0]} (old)"
    d = np.abs(x.astype(np.int16) - y.astype(np.int16))
    if d.size == 0 or d.max() <= 2:
        return True, "pixels within 2"
    off = np.count_nonzero(d.max(-1) > 8) / (d.shape[0] * d.shape[1])
    if d.mean() < 1.0 and off < 0.005:
        return True, f"block-compression noise (mean {d.mean():.2f})"
    return False, f"pixels differ (mean {d.mean():.1f}, {off * 100:.1f}% by more than 8)"


def png(a, b):
    try:
        from PIL import Image
        x = np.asarray(Image.open(a).convert("RGBA"))
        y = np.asarray(Image.open(b).convert("RGBA"))
    except Exception as ex:
        return False, f"unreadable ({type(ex).__name__})"
    return pixels_compare(x, y)


def ktx2(a, b):
    try:
        import ktx2_io
        x = ktx2_io.decode(open(a, "rb").read())
        y = ktx2_io.decode(open(b, "rb").read())
    except Exception as ex:
        return False, f"unreadable ({type(ex).__name__})"
    return pixels_compare(x, y)


# ---------------------------------------------------------------------------
# Sounds and movies
# ---------------------------------------------------------------------------

def run(cmd):
    try:
        return subprocess.run(cmd, capture_output=True)
    except OSError:
        return None


def sound(a, b):
    def pcm(p):
        r = run(["ffmpeg", "-v", "error", "-i", p, "-f", "s16le", "-ac", "1", "-ar", "48000", "-"])
        return None if r is None or r.returncode else np.frombuffer(r.stdout, "<i2").astype(np.float64)
    x, y = pcm(a), pcm(b)
    if x is None or y is None:
        return False, "not decoded (ffmpeg?)"
    if abs(len(x) - len(y)) > 1024:
        return False, f"length {len(x) / 48000:.2f} s vs {len(y) / 48000:.2f} s"
    n = min(len(x), len(y))
    x, y = x[:n], y[:n]
    if not np.any(x - y):
        return True, "the same audio"
    snr = lambda d: 20 * np.log10(max(np.sqrt(np.mean(y ** 2)), 1e-9) / max(np.sqrt(np.mean(d ** 2)), 1e-9))
    if snr(x - y) >= SNR_DB:
        return True, f"the same audio, encoded again ({snr(x - y):.0f} dB)"
    gain = np.dot(x, y) / max(np.dot(y, y), 1e-9)
    mean, worst = band_levels(x, y)
    if mean < 0.5 and worst < 2.0:
        # Hissy sounds encode with more noise, but it's the same sound: the same
        # loudness in each band of frequencies, moment by moment.
        return True, f"the same audio, encoded again ({snr(x - y):.0f} dB, every band within {worst:.1f} dB)"
    if gain > 0 and snr(x - gain * y) >= SNR_DB:
        db = 20 * np.log10(gain)
        return False, f"the same audio, {abs(db):.1f} dB {'louder' if db > 0 else 'quieter'} in the new"
    return False, f"audio differs ({snr(x - y):.0f} dB)"


def band_levels(x, y):
    """How far apart two sounds' levels (dB) are, in 24 bands of frequencies from 60 Hz
    to 20 kHz, every 1024 samples (overlapping by half), where either is within
    50 dB of the loudest: the mean and the 95th percentile."""
    edges = np.geomspace(60, 20000, 25)
    n = 1024
    freqs = np.fft.rfftfreq(n, 1 / 48000)
    band = np.digitize(freqs, edges)

    def levels(z):
        z = np.pad(z, (0, max(0, n - len(z))))
        frames = np.lib.stride_tricks.sliding_window_view(z, n)[::n // 2] * np.hanning(n)
        power = np.abs(np.fft.rfft(frames, axis=1)) ** 2
        return 10 * np.log10(np.stack([power[:, band == k].sum(1) for k in range(1, len(edges))], 1) + 1e-3)

    lx, ly = levels(x), levels(y)
    top = max(lx.max(), ly.max())
    d = np.abs(lx - ly)[(lx > top - 50) | (ly > top - 50)]
    return (float(d.mean()), float(np.percentile(d, 95))) if d.size else (0.0, 0.0)


def movie(a, b):
    def info(p):
        r = run(["ffprobe", "-v", "error", "-select_streams", "v:0", "-count_packets", "-show_entries",
                 "stream=width,height,nb_read_packets", "-of", "csv=p=0", p])
        return None if r is None or r.returncode else r.stdout.decode().strip()
    x, y = info(a), info(b)
    if x is None or y is None:
        return False, "not read (ffprobe?)"
    if x == y:
        return True, "the same size and frames, encoded again"
    return False, f"{x} vs {y} (width, height, frames)"


# ---------------------------------------------------------------------------
# Models (.glb)
# ---------------------------------------------------------------------------

COMPONENT = {5120: np.int8, 5121: np.uint8, 5122: np.int16, 5123: np.uint16, 5125: np.uint32, 5126: np.float32}
WIDTH = {"SCALAR": 1, "VEC2": 2, "VEC3": 3, "VEC4": 4, "MAT2": 4, "MAT3": 9, "MAT4": 16}


def load_glb(path):
    data = open(path, "rb").read()
    n, = struct.unpack_from("<I", data, 12)
    doc = json.loads(data[20:20 + n])
    at, chunk = 20 + n, b""
    if at + 8 <= len(data):
        m, = struct.unpack_from("<I", data, at)
        chunk = data[at + 8:at + 8 + m]
    buffers = []
    for buf in doc.get("buffers", []):
        if "uri" in buf:
            with open(os.path.join(os.path.dirname(path), buf["uri"]), "rb") as f:
                buffers.append(f.read())
        else:
            buffers.append(chunk)
    return doc, buffers


def accessor(doc, buffers, i):
    a = doc["accessors"][i]
    width = WIDTH[a["type"]]
    if "bufferView" not in a or a["count"] == 0:
        return np.zeros((a["count"], width))
    view = doc["bufferViews"][a["bufferView"]]
    kind = np.dtype(COMPONENT[a["componentType"]])
    offset = view.get("byteOffset", 0) + a.get("byteOffset", 0)
    stride = view.get("byteStride", 0) or kind.itemsize * width
    raw = np.frombuffer(buffers[view.get("buffer", 0)], np.uint8, stride * (a["count"] - 1) + kind.itemsize * width,
                        offset)
    rows = np.lib.stride_tricks.as_strided(raw, (a["count"], kind.itemsize * width), (stride, 1))
    out = rows.copy().view(kind).reshape(a["count"], width).astype(np.float64)
    if a.get("normalized") and kind != np.float32:
        out /= np.iinfo(kind).max
    return out


def canonical(doc, buffers):
    """A model's content by name, whatever the order of things in the file: nodes,
    meshes (their primitives' data and materials), materials (their textures by
    file), the images used, clips (by node and path), skins and scenes."""
    nodes = doc.get("nodes", [])
    name = lambda i: nodes[i].get("name", f"#{i}")
    c = {}
    for i, n in enumerate(nodes):
        c[("node", name(i))] = {
            "t": n.get("translation"), "r": n.get("rotation"), "s": n.get("scale"), "m": n.get("matrix"),
            "mesh": doc["meshes"][n["mesh"]].get("name") if "mesh" in n else None,
            "children": sorted(name(k) for k in n.get("children", [])), "extras": n.get("extras")}
    images = doc.get("images", [])
    textures = doc.get("textures", [])

    def texture(t):
        if isinstance(t, dict) and "index" in t:
            src = textures[t["index"]].get("source")
            t = dict(t, index=images[src].get("uri", images[src].get("name")) if src is not None else None)
        return t

    for i, m in enumerate(doc.get("materials", [])):
        mm = {}
        for k, v in m.items():
            if k == "pbrMetallicRoughness":
                v = {kk: texture(vv) for kk, vv in v.items()}
            elif isinstance(v, dict):
                v = texture(v)
            mm[k] = v
        c[("material", m.get("name", f"#{i}"))] = mm
    materials = doc.get("materials", [])
    for i, m in enumerate(doc.get("meshes", [])):
        prims = []
        for p in m["primitives"]:
            data = {k: (lambda k=k, v=v: accessor(doc, buffers, v)) for k, v in p["attributes"].items()}
            if "indices" in p:
                data["indices"] = lambda p=p: accessor(doc, buffers, p["indices"])
            prims.append((materials[p["material"]].get("name") if "material" in p else None, data))
        c[("mesh", m.get("name", f"#{i}"))] = prims
    c[("images",)] = sorted(im.get("uri", im.get("name", "")) for im in images)
    for i, a in enumerate(doc.get("animations", [])):
        chans = {}
        for ch in a["channels"]:
            s = a["samplers"][ch["sampler"]]
            chans[(name(ch["target"]["node"]), ch["target"]["path"])] = (
                lambda s=s: accessor(doc, buffers, s["input"]), lambda s=s: accessor(doc, buffers, s["output"]))
        c[("clip", a.get("name", f"#{i}"))] = chans
    for i, s in enumerate(doc.get("skins", [])):
        c[("skin", i)] = {"joints": [name(k) for k in s["joints"]], "skeleton": s.get("skeleton") is not None,
                          "ibm": (lambda s=s: accessor(doc, buffers, s["inverseBindMatrices"]))
                          if "inverseBindMatrices" in s else None}
    c[("scenes",)] = [sorted(name(k) for k in sc.get("nodes", [])) for sc in doc.get("scenes", [])]
    c[("extras",)] = doc.get("extras")
    return c


class Notes:
    def __init__(self):
        self.diffs = []
        self.flipped = 0
        self.vertices = 0

    def add(self, what):
        self.diffs.append(what)


def values_close(x, y, where, notes):
    if callable(x):
        x = x()
    if callable(y):
        y = y()
    if isinstance(x, np.ndarray) or isinstance(y, np.ndarray):
        if not isinstance(x, np.ndarray) or not isinstance(y, np.ndarray) or x.shape != y.shape:
            notes.add(f"{where}: {getattr(x, 'shape', x)} vs {getattr(y, 'shape', y)}")
            return
        if x.size == 0:
            return
        if where.endswith(".TANGENT") and x.shape[1] == 4:
            notes.flipped += int(np.count_nonzero(np.sign(x[:, 3]) != np.sign(y[:, 3])))
            notes.vertices += len(x)
            x, y = x[:, :3], y[:, :3]
        d = np.abs(x - y)
        if d.max() > max(ABS, REL * max(1.0, float(np.abs(y).max()))) * 10:
            notes.add(f"{where}: by up to {d.max():.4g}")
        return
    if isinstance(x, dict) and isinstance(y, dict):
        for k in sorted(set(x) | set(y), key=str):
            values_close(x.get(k), y.get(k), f"{where}.{k}", notes)
        return
    if isinstance(x, (list, tuple)) and isinstance(y, (list, tuple)):
        if len(x) != len(y):
            notes.add(f"{where}: {len(x)} vs {len(y)} items")
            return
        for i, (p, q) in enumerate(zip(x, y)):
            values_close(p, q, f"{where}[{i}]", notes)
        return
    if isinstance(x, (int, float)) and isinstance(y, (int, float)) and not isinstance(x, bool):
        if not close(float(x), float(y)):
            notes.add(f"{where}: {x} vs {y}")
        return
    if x != y:
        notes.add(f"{where}: {str(x)[:60]} vs {str(y)[:60]}")


def glb(a, b):
    try:
        ca, cb = canonical(*load_glb(a)), canonical(*load_glb(b))
    except Exception as ex:
        return False, f"unreadable ({type(ex).__name__}: {ex})"
    notes = Notes()
    for k in sorted(set(ca) | set(cb), key=str):
        label = " ".join(str(p) for p in k)
        if k not in cb:
            notes.add(f"{label}: only in the new")
            continue
        if k not in ca:
            notes.add(f"{label}: only in the old")
            continue
        if k[0] == "mesh":
            pa, pb = ca[k], cb[k]
            if len(pa) != len(pb):
                notes.add(f"{label}: {len(pa)} vs {len(pb)} primitives")
                continue
            for i, ((ma, da), (mb, db)) in enumerate(zip(pa, pb)):
                if ma != mb:
                    notes.add(f"{label}[{i}]: material {ma} vs {mb}")
                for att in sorted(set(da) | set(db)):
                    values_close(da.get(att), db.get(att), f"{k[1]}[{i}].{att}", notes)
        elif k[0] == "clip":
            for ch in sorted(set(ca[k]) | set(cb[k])):
                where = f"clip {k[1]} {ch[0]} {ch[1]}"
                if ch not in ca[k] or ch not in cb[k]:
                    notes.add(f"{where}: only in the {'old' if ch not in ca[k] else 'new'}")
                    continue
                for part, (x, y) in zip(("times", "keys"), zip(ca[k][ch], cb[k][ch])):
                    values_close(x, y, f"{where} {part}", notes)
        else:
            values_close(ca[k], cb[k], label, notes)
    share = notes.flipped * 100 / max(notes.vertices, 1)
    flip = ("" if not notes.flipped else
            f", bitangent signs differ on {share:.1f}% of vertices" if share >= 0.1 else
            f", bitangent signs differ on {notes.flipped} vertices")
    if notes.diffs:
        more = f" (and {len(notes.diffs) - 3} more)" if len(notes.diffs) > 3 else ""
        return False, "; ".join(notes.diffs[:3]) + more + flip
    return True, "the same content" + flip


# ---------------------------------------------------------------------------
# Collision and effect meshes
# ---------------------------------------------------------------------------

def triangle_sets(path):
    """A collision file's (levels/<map>_col.bin) sets of triangles, or a doors file's
    (levels/<map>_doors.bin) groups, as {name: (kind, corners in metres)}."""
    data = open(path, "rb").read()
    origin = np.array(struct.unpack_from("<3f", data, 8), np.float64)
    step, = struct.unpack_from("<f", data, 20)
    at, out = 24, {}

    def tris(count):
        nonlocal at
        t = np.frombuffer(data, "<i2", 9 * count, at).reshape(count, 3, 3) * step + origin
        at += 18 * count
        return t

    if data[:4] == b"FOCC":
        version, = struct.unpack_from("<I", data, 4)
        for k, label in enumerate(("solid", "movement only", "players only")[:3 if version >= 3 else 2]):
            n, = struct.unpack_from("<I", data, at)
            at += 4
            out[label] = (0, tris(n))
    elif data[:4] == b"FOCD":
        n, = struct.unpack_from("<I", data, at)
        at += 4
        for _ in range(n):
            m, = struct.unpack_from("<H", data, at)
            name = data[at + 2:at + 2 + m].decode()
            kind = data[at + 2 + m]
            at += 3 + m
            c, = struct.unpack_from("<I", data, at)
            at += 4
            out[name] = (kind, tris(c))
    else:
        return None
    return out, step


def unmatched(x, y, tol):
    """How many of x's triangles (by their middles) have none of y's within `tol` or so."""
    if len(x) == 0 or len(y) == 0:
        return len(x)

    def key(c):
        c = c + (1 << 20)
        return (c[:, 0] << 42) | (c[:, 1] << 21) | c[:, 2]

    have = np.unique(key(np.floor(y.mean(1) / tol).astype(np.int64)))
    cx = np.floor(x.mean(1) / tol).astype(np.int64)
    found = np.zeros(len(x), bool)
    for i in (-1, 0, 1):
        for j in (-1, 0, 1):
            for k in (-1, 0, 1):
                left = np.flatnonzero(~found)
                want = key(cx[left] + (i, j, k))
                at = np.minimum(np.searchsorted(have, want), len(have) - 1)
                found[left] = have[at] == want
    return int(np.count_nonzero(~found))


def collision(a, b):
    sa, sb = triangle_sets(a), triangle_sets(b)
    if sa is None or sb is None:
        return False, "not collision"
    (ga, step), (gb, _) = sa, sb
    tol = max(step, 0.01) * 2
    diffs, doubled = [], []
    for name in list(dict.fromkeys(list(ga) + list(gb))):
        if name not in gb or name not in ga:
            diffs.append(f"{name} only in the {'new' if name in ga else 'old'}")
            continue
        (ka, ta), (kb, tb) = ga[name], gb[name]
        if ka != kb:
            diffs.append(f"{name}: kind {ka} vs {kb}")
        na, nb = unmatched(ta, tb, tol), unmatched(tb, ta, tol)
        if na or nb:
            diffs.append(f"{name}: {na} of {len(ta)} triangles only in the new, {nb} of {len(tb)} only in the old")
        elif len(ta) != len(tb):
            doubled.append(f"{name} {len(ta)} vs {len(tb)}")
    if diffs:
        more = f" (and {len(diffs) - 3} more)" if len(diffs) > 3 else ""
        return False, "; ".join(diffs[:3]) + more
    extra = f"; some twice in one: {', '.join(doubled[:3])}" if doubled else ""
    return True, f"the same triangles (to {tol * 100:.0f} cm){extra}"


def floats(a, b):
    """Effect meshes (models/fx/*.fxmesh) and other little binaries of numbers: the
    same size and, read as 4-byte floats, the same numbers but for roundings (or a
    sign: a bitangent's)."""
    x, y = open(a, "rb").read(), open(b, "rb").read()
    if len(x) != len(y):
        return False, f"{len(x)} vs {len(y)} bytes"
    if len(x) % 4:
        return False, "not floats"
    fx = np.frombuffer(x, "<f4").astype(np.float64)
    fy = np.frombuffer(y, "<f4").astype(np.float64)
    ix, iy = np.frombuffer(x, "<u4"), np.frombuffer(y, "<u4")
    diff = ix != iy
    ok = np.isclose(fx[diff], fy[diff], rtol=REL, atol=ABS) | np.isclose(fx[diff], -fy[diff], rtol=REL, atol=ABS)
    if ok.all():
        return True, f"numbers a rounding apart ({int(diff.sum())} of {len(fx)})"
    return False, f"{int((~ok).sum())} numbers differ"


# ---------------------------------------------------------------------------
# The folders
# ---------------------------------------------------------------------------

def font(a, b):
    """A font's tables but for the times in its head table (when it was made)."""
    try:
        from fontTools.ttLib import TTFont
    except ImportError:
        return False, "not compared (fontTools?)"

    def tables(p):
        f = TTFont(p)
        head = f["head"]
        stamp = {k: v for k, v in vars(head).items() if k not in ("created", "modified", "checkSumAdjustment")}
        return {t: f.getTableData(t) for t in f.keys() if t not in ("GlyphOrder", "head")}, stamp

    try:
        if tables(a) == tables(b):
            return True, "the same font but for when it was made"
        return False, "tables differ"
    except Exception as ex:
        return False, f"not compared ({type(ex).__name__})"


BY_EXT = {
    ".png": png, ".ktx2": ktx2, ".ogg": sound, ".h264": movie, ".glb": glb, ".ttf": font,
    ".txt": text, ".wgsl": text, ".json": text, ".ron": text, ".csv": text, ".md": text, ".cfg": text,
    ".fxmesh": floats,
}


def compare(a, b, rel):
    if os.path.getsize(a) == os.path.getsize(b) and md5(a) == md5(b):
        return "same", ""
    ext = os.path.splitext(rel)[1].lower()
    how = BY_EXT.get(ext)
    if ext == ".bin" and open(a, "rb").read(4) in (b"FOCC", b"FOCD"):
        how = collision
    if how is None:
        return "differs", f"{os.path.getsize(a)} vs {os.path.getsize(b)} bytes"
    for attempt in (1, 2):
        try:
            ok, why = how(a, b)
            break
        except Exception as ex:
            # (Once more: a file read while something else was busy with the disk.)
            ok, why = False, f"not compared ({type(ex).__name__}: {ex})"
    return ("equivalent" if ok else "differs"), why


def files(root):
    out = {}
    for d, _dirs, names in os.walk(root):
        for n in names:
            full = os.path.join(d, n)
            out[os.path.relpath(full, root).replace(os.sep, "/")] = full
    return out


def group(rel):
    parts = rel.split("/")
    return "/".join(parts[:2]) if parts[0] in ("models", "textures", "levels") and len(parts) > 2 else parts[0]


KINDS = ("same", "equivalent", "differs", "new", "old only")


def main(new, old, report=None):
    report = report or os.path.join(new, "compare_report.txt")
    a, b = files(new), files(old)
    a.pop(os.path.relpath(report, new).replace(os.sep, "/"), None)
    b.pop(os.path.relpath(report, old).replace(os.sep, "/"), None)
    stats = defaultdict(lambda: defaultdict(int))
    lines = defaultdict(list)
    names = sorted(set(a) | set(b))
    for n, rel in enumerate(names):
        if rel not in b:
            kind, why = "new", ""
        elif rel not in a:
            kind, why = "old only", ""
        else:
            kind, why = compare(a[rel], b[rel], rel)
        stats[group(rel)][kind] += 1
        if kind != "same":
            lines[kind].append(f"{kind:10} {rel}" + (f"  ({why})" if why else ""))
        if sys.stdout.isatty() and n % 50 == 0:
            print(f"\r{n} of {len(names)}...", end="", flush=True)
    if sys.stdout.isatty():
        print("\r" + " " * 30 + "\r", end="")
    total = defaultdict(int)
    print(f"{'':32}" + "".join(f"{k:>11}" for k in KINDS))
    for g in sorted(stats):
        print(f"{g:32}" + "".join(f"{stats[g][k]:11}" for k in KINDS))
        for k in KINDS:
            total[k] += stats[g][k]
    print(f"{'all':32}" + "".join(f"{total[k]:11}" for k in KINDS))
    with open(report, "w", encoding="utf-8", newline="\n") as fh:
        for kind in ("differs", "new", "old only", "equivalent"):
            for l in lines[kind]:
                fh.write(l + "\n")
    print(f"\nThe files that aren't the same: {report}")


if __name__ == "__main__":
    if len(sys.argv) not in (3, 4):
        sys.exit(__doc__)
    main(*sys.argv[1:])
