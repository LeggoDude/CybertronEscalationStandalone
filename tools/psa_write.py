"""Writes a FoC package's animation sets (AnimSet) as ActorX .psa files, as a
umodel that reads all of FoC's clips writes them: the same chunks, clips, frames
and keys, byte for byte.

Why: FoC saves most clips in its own layout ("Trans3"), and the official UE Viewer
builds drop every one of those (UAnimSequence::DecodeTrans3Anims returns early when
the clip has no CompressedByteStream, which those clips never have), so their sets
come out short of clips, or, when all their clips are FoC's own (the trap
sentries', the Optimus doors'), empty: then umodel writes no file at all. The
clips in UE3's usual layout umodel reads fine. So tools/extract_game.py has umodel
export a set first, then rewrites its .psa with this: the clips in FoC's layout
worked out here (as umodel's DecodeTrans3Anims and its .psa export work them out,
in single precision), the others taken from umodel's .psa as they are.

usage: python psa_write.py <package .xxx> <out folder> [<AnimSet> ...]

rewrites <out folder>/<package name>/AnimSet/<set>.psa (umodel's layout; umodel's
own .psa there, if any, gives the clips in UE3's layout) for the sets named (none:
every AnimSet in the package); a set without a clip with keys gets no file (umodel:
"has no sequences"). Exits 1 if a set named isn't in the package.
"""
import os
import struct
import sys

import numpy as np

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.path.join(HERE, "ue3"))
from ue3pkg import Package  # noqa: E402
from layout import props_any  # noqa: E402
from anim import _ints, _names  # noqa: E402

F = np.float32
PSA_VERSION = 20100422
QUAT_SIZE = {"ACF_None": 16, "ACF_Float96NoW": 12, "ACF_Fixed48NoW": 6, "ACF_IntervalFixed48NoW": 6,
             "ACF_IntervalFixed32NoW": 4, "ACF_Fixed32NoW": 4, "ACF_Float32NoW": 4}


def _restore_w(x, y, z):
    """umodel's RESTORE_QUAT_W, in single precision."""
    w2 = F(1) - (x * x + y * y + z * z)
    with np.errstate(invalid="ignore"):
        return np.where(w2 > 0, np.sqrt(np.maximum(w2, F(0))), F(0)).astype(F)


def _rotations(fmt, raw, n, interval):
    """Animated rotation keys -> (x, y, z, w) arrays, as umodel decodes them."""
    if fmt == "ACF_None":
        q = np.frombuffer(raw, "<f4").reshape(n, 4)
        return q[:, 0], q[:, 1], q[:, 2], q[:, 3]
    if fmt == "ACF_Float96NoW":
        q = np.frombuffer(raw, "<f4").reshape(n, 3)
        x, y, z = q[:, 0], q[:, 1], q[:, 2]
    elif fmt == "ACF_Fixed48NoW":
        q = np.frombuffer(raw, "<u2").reshape(n, 3).astype(np.int32)
        x, y, z = [((q[:, i] - 32767).astype(F) / F(32767)) for i in range(3)]
    elif fmt == "ACF_Fixed32NoW":
        v = np.frombuffer(raw, "<u4")
        x = ((v >> 21) & 0x7FF).astype(F) / F(1023) - F(1)
        y = ((v >> 10) & 0x7FF).astype(F) / F(1023) - F(1)
        z = (v & 0x3FF).astype(F) / F(511) - F(1)
    elif fmt == "ACF_IntervalFixed48NoW":   # FoC's own (umodel: FQuatIntervalFixed48NoW_Trans)
        q = np.frombuffer(raw, "<u2").reshape(n, 3).astype(np.int32)
        mins, ranges = interval
        x, y, z = [((q[:, i] - 32767).astype(F) / F(32767) * ranges[i] + mins[i]) for i in range(3)]
    elif fmt == "ACF_IntervalFixed32NoW":
        v = np.frombuffer(raw, "<u4")
        mins, ranges = interval
        x = (((v >> 21) & 0x7FF).astype(F) / F(1023) - F(1)) * ranges[0] + mins[0]
        y = (((v >> 10) & 0x7FF).astype(F) / F(1023) - F(1)) * ranges[1] + mins[1]
        z = ((v & 0x3FF).astype(F) / F(511) - F(1)) * ranges[2] + mins[2]
    else:
        raise ValueError(f"rotation format {fmt} not supported")
    return x, y, z, _restore_w(x, y, z)


def _decode(pkg, pr, r, nbones):
    """A clip in FoC's own layout, its tracks as umodel's DecodeTrans3Anims makes
    them: per bone, its position keys (k x 3) and rotation keys (k x 4), k 1 or the
    clip's frames. None when the clip has no keys; "ue3" when it is in UE3's usual
    layout (umodel reads those)."""
    if r.i32() == 0:
        return "ue3"
    data = r.bytes(r.i32())
    if len(data) < 20:
        return None  # no keys (some clips have none)
    frames = pr.get("NumFrames", 1)
    fmt = pr.get("RotationCompressionFormat", "ACF_Float96NoW")
    nsr, nst, nar, nat, naut = struct.unpack_from("<5i", data)
    body = data[20:]
    qsize = QUAT_SIZE[fmt]
    s_st, s_ar = nsr, nsr + nst
    s_at = s_ar + nar
    s_aut = s_at + nat
    rot_int = 40 * nar if fmt in ("ACF_IntervalFixed32NoW", "ACF_IntervalFixed48NoW") else 0
    o_st = 16 * nsr
    o_ri = o_st + 12 * nst
    o_ti = o_ri + rot_int
    o_ar = o_ti + 24 * nat
    stride = qsize * nar + 4 * nat + 12 * naut
    if o_ar + stride * frames != len(body):
        raise ValueError(f"{pr.get('SequenceName')}: unexpected key data size")
    keys = np.frombuffer(body, np.uint8, stride * frames, o_ar).reshape(frames, stride)
    o_at = qsize * nar          # within a frame's keys
    o_aut = o_at + 4 * nat
    offsets = _ints(pr["TrackOffsets"])
    if len(offsets) != 2 * nbones:
        raise ValueError(f"{pr.get('SequenceName')}: {len(offsets) // 2} tracks for {nbones} bones")
    tracks = []
    for b in range(nbones):
        rk, tk = offsets[2 * b], offsets[2 * b + 1]
        if tk < s_st:
            pos = np.zeros((1, 3), F)
        elif tk < s_at:
            pos = np.frombuffer(body, "<f4", 3, o_st + 12 * (tk - s_st)).reshape(1, 3).copy()
        elif tk < s_aut:
            k = tk - s_at
            mins = np.frombuffer(body, "<f4", 3, o_ti + 24 * k)
            ranges = np.frombuffer(body, "<f4", 3, o_ti + 24 * k + 12)
            v = np.ascontiguousarray(keys[:, o_at + 4 * k:o_at + 4 * k + 4]).view("<u4").reshape(-1)
            pos = np.stack([((v >> 21) & 0x7FF).astype(F) / F(2047) * ranges[0] + mins[0],
                            ((v >> 11) & 0x3FF).astype(F) / F(1023) * ranges[1] + mins[1],
                            (v & 0x7FF).astype(F) / F(2047) * ranges[2] + mins[2]], 1)
        else:
            a = o_aut + 12 * (tk - s_aut)
            pos = np.ascontiguousarray(keys[:, a:a + 12]).view("<f4").reshape(frames, 3).copy()
        if rk < s_ar:
            q = np.frombuffer(body, "<f4", 4, 16 * rk).reshape(1, 4).copy()
            q[:, 3] *= F(-1)
            rot = q
        else:
            k = rk - s_ar
            raw = np.ascontiguousarray(keys[:, qsize * k:qsize * (k + 1)]).tobytes()
            interval = None
            if rot_int:
                interval = (np.frombuffer(body, "<f4", 3, o_ri + 40 * k), np.frombuffer(body, "<f4", 3, o_ri + 40 * k + 12))
            x, y, z, w = _rotations(fmt, raw, frames, interval)
            if rot_int:
                bw, bx, by, bz = np.frombuffer(body, "<f4", 4, o_ri + 40 * k + 24)
                # CQuat::Mul(TransQuatBase), then W negated
                rot = np.stack([w * bx + x * bw + y * bz - z * by,
                                w * by - x * bz + y * bw + z * bx,
                                w * bz + x * by - y * bx + z * bw,
                                -(w * bw - x * bx - y * by - z * bz)], 1).astype(F)
            else:
                rot = np.stack([x, y, z, w], 1).astype(F)
        tracks.append((pos.astype(F), rot.astype(F)))
    return tracks


def _libm():
    """The C library's single-precision sinf and atan2f, which umodel's Slerp calls
    (with them, the keys come out exactly as a umodel built against that library
    writes them), or None."""
    import ctypes
    import ctypes.util
    for name in ("ucrtbase", "msvcrt") if os.name == "nt" else (ctypes.util.find_library("m"), "libm.so.6"):
        try:
            lib = ctypes.CDLL(name)
            sinf, atan2f = lib.sinf, lib.atan2f
        except (OSError, AttributeError, TypeError):
            continue
        sinf.restype, sinf.argtypes = ctypes.c_float, [ctypes.c_float]
        atan2f.restype, atan2f.argtypes = ctypes.c_float, [ctypes.c_float, ctypes.c_float]
        if abs(sinf(1.0) - 0.84147098) < 1e-6 and abs(atan2f(1.0, 1.0) - 0.78539816) < 1e-6:
            return sinf, atan2f
    return None


_LIBM = _libm()


def _sin(x):
    return F(_LIBM[0](float(x))) if _LIBM else F(np.sin(np.float64(x)))


def _atan2(y, x):
    return F(_LIBM[1](float(y), float(x))) if _LIBM else F(np.arctan2(np.float64(y), np.float64(x)))


def _sample(keys, frames, is_quat):
    """umodel's CAnimTrack::GetBonePosition at frames 0..frames-1 (no loop): the
    key at frame / frames * keys, lerped (slerped) when that isn't a whole key."""
    n = len(keys)
    out = np.empty((frames, keys.shape[1]), F)
    out[0] = keys[0]
    if frames == 1:
        return out
    if n <= 1:
        out[1:] = keys[0]
        return out
    t = np.arange(1, frames).astype(F)
    pos = t / F(frames) * F(n)
    x = np.floor(pos).astype(np.int64)
    frac = (pos - x.astype(F)).astype(F)
    y = x + 1
    last = y >= n
    y[last] = n - 1
    frac[last] = 0
    a, b = keys[x], keys[y]
    res = a.copy()
    for i in np.nonzero(frac > 0)[0]:
        al = frac[i]
        if not is_quat:
            res[i] = a[i] + al * (b[i] - a[i])
            continue
        A, B = a[i], b[i]
        if al >= F(1):
            res[i] = B
            continue
        cosom = A[0] * B[0] + A[1] * B[1] + A[2] * B[2] + A[3] * B[3]
        sign = F(1)
        if cosom < 0:
            cosom, sign = -cosom, F(-1)
        if F(1) - cosom > F(1e-6):
            f = F(1) - cosom * cosom
            inv = F(1) / np.sqrt(f)
            omega = _atan2(f * inv, abs(cosom))
            sa = _sin((F(1) - al) * omega) * inv
            sb = _sin(al * omega) * inv
        else:
            sa, sb = F(1) - al, al
        sb = sb * sign
        res[i] = sa * A + sb * B
    out[1:] = res
    return out


def _bytes_name(s, n=64):
    b = s.encode("latin-1")
    if len(b) >= n:
        mid = (n - 4) // 2
        b = b[:mid] + b"____" + b[len(b) - mid + 1:]
    return b.ljust(n, b"\0")[:n]


def _chunk(cid, flag, size, count):
    return cid.encode().ljust(20, b"\0") + struct.pack("<Iii", flag, size, count)


def read_psa(path):
    """umodel's .psa: (bone names, {clip name: (ANIMINFO record, its keys' bytes)})."""
    data = open(path, "rb").read()
    o, bones, infos, keys = 0, [], [], b""
    while o + 32 <= len(data):
        cid = data[o:o + 20].split(b"\0")[0].decode("latin-1")
        size, count = struct.unpack_from("<ii", data, o + 24)
        body = data[o + 32:o + 32 + size * count]
        if cid == "BONENAMES":
            bones = [body[i * size:i * size + 64].split(b"\0")[0].decode("latin-1") for i in range(count)]
        elif cid == "ANIMINFO":
            infos = [body[i * size:(i + 1) * size] for i in range(count)]
        elif cid == "ANIMKEYS":
            keys = body
        o += 32 + size * count
    nb = len(bones)
    clips = {}
    for info in infos:
        first, frames = struct.unpack_from("<ii", info, 160)
        clips[info[:64].split(b"\0")[0].decode("latin-1")] = (info, keys[first * nb * 32:(first + frames) * nb * 32])
    return bones, clips


def animset_psa(pkg, idx, own=None):
    """The .psa bytes of the AnimSet at export index idx (1-based), or None when
    none of its clips has keys, and notes: its clips in FoC's layout worked out here,
    those in UE3's layout taken from `own` (read_psa of umodel's .psa, or None)."""
    e = pkg.exports[idx - 1]
    spr, _ = props_any(pkg, e, (4, 0, 8))
    bones = _names(pkg, spr["TrackBoneNames"]) if "TrackBoneNames" in spr else []
    nb = len(bones)
    if own and own[0] and [_bytes_name(b) for b in own[0]] != [_bytes_name(b) for b in bones]:
        raise ValueError(f"{e['name']}: umodel's .psa has other bones")
    clips, notes = [], []
    for s in _ints(spr["Sequences"]) if "Sequences" in spr else []:
        if s == 0:
            continue
        if s < 0:
            raise ValueError(f"{e['name']}: a clip from another package ({pkg.full_name(s)})")
        pr, r = props_any(pkg, pkg.exports[s - 1], (4, 0, 8))
        name = pr.get("SequenceName", "None")
        tracks = _decode(pkg, pr, r, nb)
        if tracks is None:
            continue
        if tracks == "ue3":
            if own and name in own[1]:
                clips.append(("ue3", name) + own[1][name])
            else:
                notes.append(f"{e['name']}.{name} (UE3 layout) is not in umodel's .psa")
            continue
        frames = pr.get("NumFrames", 1)
        rate = F(frames) / F(pr.get("SequenceLength", 0.0)) * F(pr.get("RateScale", 1.0))
        clips.append(("foc", name, frames, rate, tracks))
    if not clips:
        return None, notes
    out = [_chunk("ANIMHEAD", PSA_VERSION, 0, 0), _chunk("BONENAMES", 0, 120, nb)]
    for i, name in enumerate(bones):
        out.append(_bytes_name(name) + struct.pack("<iii", 0, 0, -1 if i == 0 else 0)
                   + struct.pack("<4f3f4f", 0, 0, 0, 0, 0, 0, 0, 1.0, 0, 0, 0))
    out.append(_chunk("ANIMINFO", 0, 168, len(clips)))
    first = 0
    for c in clips:
        if c[0] == "ue3":
            info = c[2]
            frames = struct.unpack_from("<i", info, 164)[0]
            out.append(info[:160] + struct.pack("<i", first) + info[164:])
        else:
            _, name, frames, rate, _ = c
            out.append(_bytes_name(name) + _bytes_name("None")
                       + struct.pack("<iiiifffiii", nb, 0, 0, frames * nb, 0.0, float(frames), float(rate), 0, first, frames))
        first += frames
    out.append(_chunk("ANIMKEYS", 0, 32, first * nb))
    for c in clips:
        if c[0] == "ue3":
            out.append(c[3])
            continue
        _, name, frames, rate, tracks = c
        k = np.empty((frames, nb, 8), F)
        for b, (pos, rot) in enumerate(tracks):
            k[:, b, 0:3] = _sample(pos, frames, False)
            k[:, b, 3:7] = _sample(rot, frames, True)
        k[:, :, 1] *= F(-1)   # MIRROR_MESH: position Y, rotation Y and W
        k[:, :, 4] *= F(-1)
        k[:, :, 6] *= F(-1)
        k[:, :, 7] = F(1)     # key time
        out.append(k.astype("<f4").tobytes())
    out.append(_chunk("SCALEKEYS", 0, 16, 0))
    return b"".join(out), notes


def write(package, out_dir, names=None, pkg=None, log=print):
    """(Re)writes the package's animation sets named (None: all) under out_dir, in
    umodel's layout, umodel's own .psa there giving the clips in UE3's layout;
    returns the paths written. Raises KeyError for a set not in the package."""
    pkg = pkg or Package(package)
    sets = {e["name"]: i for i, e in enumerate(pkg.exports, 1) if pkg.class_name(e) == "AnimSet"}
    if names is None:
        names = list(sets)
    missing = [n for n in names if n not in sets]
    if missing:
        raise KeyError(f"no AnimSet {', '.join(missing)} in {os.path.basename(package)}")
    folder = os.path.join(out_dir, os.path.splitext(os.path.basename(package))[0], "AnimSet")
    written = []
    for n in names:
        path = os.path.join(folder, n + ".psa")
        own = read_psa(path) if os.path.exists(path) else None
        data, notes = animset_psa(pkg, sets[n], own)
        for note in notes:
            log("  ! " + note)
        if data is None:
            log(f"  {n}: no clip with keys - no .psa (as umodel)")
            continue
        os.makedirs(folder, exist_ok=True)
        tmp = path + ".part"
        with open(tmp, "wb") as f:
            f.write(data)
        os.replace(tmp, path)
        written.append(path)
    return written


if __name__ == "__main__":
    if len(sys.argv) < 3:
        sys.exit(__doc__)
    try:
        paths = write(sys.argv[1], sys.argv[2], sys.argv[3:] or None)
    except KeyError as ex:
        sys.exit(str(ex.args[0]))
    print(f"wrote {len(paths)} animation sets")
