"""Reader for ActorX .psa animation files written by umodel (AnimSet export).

Gives clips in the same space as umodel's glTF export: positions (x, z, y) in
metres, quaternions (x, z, y, w), root bone conjugated. umodel samples every frame
when writing a PSA, so no key-timing fix-up is needed."""
import struct
import numpy as np


def _chunks(data):
    o = 0
    while o + 32 <= len(data):
        cid = data[o:o + 20].split(b"\0")[0].decode("latin-1")
        _flags, size, count = struct.unpack_from("<iii", data, o + 20)
        yield cid, data[o + 32:o + 32 + size * count], size, count
        o += 32 + size * count


class Psa:
    def __init__(self, path):
        data = open(path, "rb").read()
        self.bones, self.clips = [], []
        keys = None
        for cid, body, size, count in _chunks(data):
            if cid == "BONENAMES":
                for i in range(count):
                    self.bones.append(body[i * size:i * size + 64].split(b"\0")[0].decode("latin-1").strip())
            elif cid == "ANIMINFO":
                for i in range(count):
                    b = body[i * size:(i + 1) * size]
                    name = b[:64].split(b"\0")[0].decode("latin-1")
                    total_bones, _ri, _kcs, _kq, _kr, track_time, rate, _sb, first, frames = struct.unpack_from("<iiiiffffii", b, 128)
                    self.clips.append({"name": name, "rate": rate, "first": first, "frames": frames})
            elif cid == "ANIMKEYS":
                keys = np.frombuffer(body, "<f4").reshape(count, 8)
        self.keys = keys

    def clip(self, name):
        """{bone name: (positions Fx3, rotations Fx4)} in glTF space, plus frame time."""
        c = next((c for c in self.clips if c["name"] == name), None)
        if c is None:
            return None, None
        nb = len(self.bones)
        k = self.keys[c["first"] * nb:(c["first"] + c["frames"]) * nb].reshape(c["frames"], nb, 8)
        out = {}
        for b, bone in enumerate(self.bones):
            pos = k[:, b, 0:3].astype(np.float64).copy()
            rot = k[:, b, 3:7].astype(np.float64).copy()
            # undo umodel's ActorX mirroring (Y flipped, W negated)
            pos[:, 1] *= -1
            rot[:, 1] *= -1
            rot[:, 3] *= -1
            # Unreal -> glTF, like umodel's glTF exporter
            gpos = np.stack([pos[:, 0], pos[:, 2], pos[:, 1]], 1) * 0.01
            grot = np.stack([rot[:, 0], rot[:, 2], rot[:, 1], rot[:, 3]], 1)
            if b == 0:
                grot[:, :3] *= -1  # conjugate
            out[bone] = (gpos.astype(np.float32), grot.astype(np.float32))
        return out, 1.0 / c["rate"] if c["rate"] > 0 else 1.0 / 30
