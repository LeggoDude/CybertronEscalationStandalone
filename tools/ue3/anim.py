"""Reads an animation clip straight from a FoC package (an AnimSet's AnimSequence).

FoC keeps its clips in its own compressed layout (umodel calls it "Trans3"): after
the sequence's properties, an int (1) and a byte array holding the counts of static
rotations, static translations, animated rotations, animated (packed) translations and
animated uncompressed translations, then the static keys, the animated keys' ranges and
the animated keys frame by frame. The sequence's TrackOffsets give each bone of the
AnimSet's TrackBoneNames its rotation key and translation key. This follows umodel's
UAnimSequence::DecodeTrans3Anims, which umodel itself skips for clips without a
CompressedByteStream (the Optimus doors' among them).

    clip(pkg, animset, sequence) -> ({bone: (positions Fx3, rotations Fx4)}, frame time)

in the same space as psa_io.Psa.clip (umodel's glTF export: metres, x z y, root bone
conjugated).
"""
import os, struct, sys

import numpy as np

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from layout import props_any  # noqa: E402

QUAT_SIZE = {"ACF_None": 16, "ACF_Float96NoW": 12, "ACF_Fixed48NoW": 6, "ACF_IntervalFixed48NoW": 6,
             "ACF_IntervalFixed32NoW": 4, "ACF_Fixed32NoW": 4, "ACF_Float32NoW": 4}


def _ints(v):
    d = v[1]
    n = struct.unpack_from("<i", d)[0]
    return list(struct.unpack_from(f"<{n}i", d, 4))


def _names(pkg, v):
    d = v[1]
    n = struct.unpack_from("<i", d)[0]
    out = []
    for k in range(n):
        idx, num = struct.unpack_from("<ii", d, 4 + 8 * k)
        name = pkg.names[idx]
        out.append(name if num == 0 else f"{name}_{num - 1}")
    return out


def _restore_w(q):
    w2 = 1.0 - (q[..., :3] ** 2).sum(-1)
    q[..., 3] = np.sqrt(np.maximum(w2, 0.0))
    return q


def _mul(a, b):
    """umodel's CQuat::Mul: a * b (x, y, z, w)."""
    ax, ay, az, aw = a[..., 0], a[..., 1], a[..., 2], a[..., 3]
    bx, by, bz, bw = b[0], b[1], b[2], b[3]
    return np.stack([aw * bx + ax * bw + ay * bz - az * by,
                     aw * by - ax * bz + ay * bw + az * bx,
                     aw * bz + ax * by - ay * bx + az * bw,
                     aw * bw - ax * bx - ay * by - az * bz], -1)


def _decode_quats(fmt, raw, mins, ranges):
    """Animated rotation keys (n x QuatSize bytes) -> (n, 4) x y z w."""
    n = len(raw) // QUAT_SIZE[fmt]
    q = np.zeros((n, 4))
    if fmt == "ACF_IntervalFixed32NoW":
        v = np.frombuffer(raw, "<u4").astype(np.int64)
        q[:, 0] = ((v >> 21) & 0x7FF) / 1023.0 - 1.0
        q[:, 1] = ((v >> 10) & 0x7FF) / 1023.0 - 1.0
        q[:, 2] = (v & 0x3FF) / 511.0 - 1.0
        q[:, :3] = q[:, :3] * ranges + mins
        return _restore_w(q)
    if fmt == "ACF_Float96NoW":
        q[:, :3] = np.frombuffer(raw, "<f4").reshape(n, 3)
        return _restore_w(q)
    if fmt == "ACF_None":
        return np.frombuffer(raw, "<f4").reshape(n, 4).astype(np.float64)
    if fmt == "ACF_Fixed48NoW":
        q[:, :3] = (np.frombuffer(raw, "<u2").reshape(n, 3).astype(np.float64) - 32767.0) / 32767.0
        return _restore_w(q)
    raise ValueError(f"rotation format {fmt} not supported")


def _sequence(pkg, animset, name):
    aset = next((i for i, e in enumerate(pkg.exports, 1)
                 if e["name"] == animset and pkg.class_name(e) == "AnimSet"), None)
    if aset is None:
        raise KeyError(animset)
    spr, _ = props_any(pkg, pkg.exports[aset - 1], (4, 0, 8))
    bones = _names(pkg, spr["TrackBoneNames"])
    for s in _ints(spr["Sequences"]):
        if s <= 0:
            continue
        pr, r = props_any(pkg, pkg.exports[s - 1], (4, 0, 8))
        if pr.get("SequenceName") == name:
            return bones, pr, r
    raise KeyError(f"{animset}.{name}")


def clip(pkg, animset, name):
    """A clip's keys, every frame of it: ({bone: (positions, rotations)}, frame time),
    as psa_io.Psa.clip gives them."""
    bones, pr, r = _sequence(pkg, animset, name)
    frames = pr.get("NumFrames", 1)
    length = pr.get("SequenceLength", 0.0)
    fmt = pr.get("RotationCompressionFormat", "ACF_Float96NoW")
    if r.i32() == 0:
        raise ValueError(f"{name}: not in FoC's own layout")
    n = r.i32()
    data = r.bytes(n)
    nsr, nst, nar, nat, naut = struct.unpack_from("<5i", data)
    body = memoryview(data)[20:]
    qsize = QUAT_SIZE[fmt]
    # Where each kind of key starts (key indices, then byte offsets).
    s_st, s_ar, s_at, s_aut = nsr, nsr + nst, nsr + nst + nar, nsr + nst + nar + nat
    rot_int = 40 * nar if fmt in ("ACF_IntervalFixed32NoW", "ACF_IntervalFixed48NoW") else 0
    o_sr = 0
    o_st = o_sr + 16 * nsr
    o_ri = o_st + 12 * nst
    o_ti = o_ri + rot_int
    o_ar = o_ti + 24 * nat
    o_at = o_ar + qsize * nar
    o_aut = o_at + 4 * nat
    stride = qsize * nar + 4 * nat + 12 * naut
    if o_ar + stride * frames != len(body):
        raise ValueError(f"{name}: unexpected key data size")
    keys = np.frombuffer(body, np.uint8, stride * frames, o_ar).reshape(frames, stride) if stride else None
    offsets = _ints(pr["TrackOffsets"])
    out = {}
    for b, bone in enumerate(bones):
        rk, tk = offsets[2 * b], offsets[2 * b + 1]
        # Translation (Unreal units).
        if tk < s_st:
            pos = np.zeros((frames, 3))
        elif tk < s_at:
            pos = np.tile(np.frombuffer(body, "<f4", 3, o_st + 12 * (tk - s_st)), (frames, 1)).astype(np.float64)
        elif tk < s_aut:
            k = tk - s_at
            mins = np.frombuffer(body, "<f4", 3, o_ti + 24 * k).astype(np.float64)
            ranges = np.frombuffer(body, "<f4", 3, o_ti + 24 * k + 12).astype(np.float64)
            a = o_at - o_ar + 4 * k
            v = np.ascontiguousarray(keys[:, a:a + 4]).view("<u4").reshape(-1).astype(np.int64)
            pos = np.stack([((v >> 21) & 0x7FF) / 2047.0, ((v >> 11) & 0x3FF) / 1023.0, (v & 0x7FF) / 2047.0], 1)
            pos = pos * ranges + mins
        else:
            a = o_aut - o_ar + 12 * (tk - s_aut)
            pos = np.ascontiguousarray(keys[:, a:a + 12]).view("<f4").reshape(frames, 3).astype(np.float64)
        # Rotation.
        if rk < s_ar:
            q = np.frombuffer(body, "<f4", 4, o_sr + 16 * rk).astype(np.float64).copy()
            q[3] *= -1
            rot = np.tile(q, (frames, 1))
        else:
            k = rk - s_ar
            a = qsize * k
            raw = np.ascontiguousarray(keys[:, a:a + qsize]).tobytes()
            if rot_int:
                mins = np.frombuffer(body, "<f4", 3, o_ri + 40 * k).astype(np.float64)
                ranges = np.frombuffer(body, "<f4", 3, o_ri + 40 * k + 12).astype(np.float64)
                w, x, y, z = np.frombuffer(body, "<f4", 4, o_ri + 40 * k + 24).astype(np.float64)
                rot = _mul(_decode_quats(fmt, raw, mins, ranges), (x, y, z, w))
            else:
                rot = _decode_quats(fmt, raw, 0.0, 1.0)
            rot[:, 3] *= -1
        # Unreal -> umodel's glTF space, as psa_io does.
        gpos = np.stack([pos[:, 0], pos[:, 2], pos[:, 1]], 1) * 0.01
        grot = np.stack([rot[:, 0], rot[:, 2], rot[:, 1], rot[:, 3]], 1)
        if b == 0:
            grot[:, :3] *= -1
        out[bone] = (gpos.astype(np.float32), grot.astype(np.float32))
    return out, (length / frames if frames > 0 else 1.0 / 30)
