"""Tiny glTF 2.0 reader/writer helpers (numpy only)."""
import json, struct, io
import numpy as np

CT = {5126: np.float32, 5123: np.uint16, 5125: np.uint32, 5121: np.uint8, 5122: np.int16, 5120: np.int8}
CT_INV = {np.dtype(np.float32): 5126, np.dtype(np.uint16): 5123, np.dtype(np.uint32): 5125, np.dtype(np.uint8): 5121}
NC = {"SCALAR": 1, "VEC2": 2, "VEC3": 3, "VEC4": 4, "MAT4": 16}
TYPE_BY_N = {1: "SCALAR", 2: "VEC2", 3: "VEC3", 4: "VEC4", 16: "MAT4"}


class Doc:
    """A loaded glTF (from .gltf+.bin or .glb)."""

    def __init__(self, path):
        self.path = path
        if path.endswith(".glb"):
            data = open(path, "rb").read()
            magic, ver, length = struct.unpack_from("<4sII", data, 0)
            assert magic == b"glTF"
            off = 12
            self.bin = b""
            while off < length:
                clen, ctype = struct.unpack_from("<I4s", data, off)
                chunk = data[off + 8: off + 8 + clen]
                if ctype == b"JSON":
                    self.g = json.loads(chunk)
                else:
                    self.bin = chunk
                off += 8 + clen
            # (Its first buffer is the binary chunk; any others are files beside it,
            # as split_glb.py leaves robot.glb's.)
            import os
            base = os.path.dirname(path)
            self.buffers = [
                open(os.path.join(base, b["uri"]), "rb").read() if "uri" in b else self.bin
                for b in self.g.get("buffers", [])
            ] or [self.bin]
        else:
            import os
            self.g = json.load(open(path))
            base = os.path.dirname(path)
            self.buffers = [open(os.path.join(base, b["uri"]), "rb").read() for b in self.g["buffers"]]

    def view_bytes(self, i):
        bv = self.g["bufferViews"][i]
        buf = self.buffers[bv["buffer"]]
        o = bv.get("byteOffset", 0)
        return buf[o: o + bv["byteLength"]]

    def acc(self, i):
        a = self.g["accessors"][i]
        bv = self.g["bufferViews"][a["bufferView"]]
        dt = np.dtype(CT[a["componentType"]])
        n = NC[a["type"]]
        buf = self.buffers[bv["buffer"]]
        off = bv.get("byteOffset", 0) + a.get("byteOffset", 0)
        stride = bv.get("byteStride", 0)
        if stride and stride != dt.itemsize * n:
            raw = np.frombuffer(buf, np.uint8, a["count"] * stride, off).reshape(a["count"], stride)
            arr = raw[:, : dt.itemsize * n].copy().view(dt).reshape(a["count"], n)
        else:
            arr = np.frombuffer(buf, dt, a["count"] * n, off).reshape(a["count"], n).copy()
        if a.get("normalized") and dt.kind in "ui":
            arr = arr.astype(np.float32) / np.iinfo(dt).max
        return arr


class Writer:
    """Builds a .glb with everything in one binary buffer.

    With `share` set, an accessor whose data (and type, target and flags) matches
    one already written is reused instead of stored again. Animations repeat the
    same keyframe times on every channel and the same still poses in clip after
    clip: robot.glb's 25,000 animation tracks hold under 3,000 different arrays,
    and sharing them keeps its JSON small."""

    def __init__(self, share=False, image_dir=None, image_uri=None):
        """With `image_dir`, images are written there as files (named after their
        key) and the glTF points at them as `image_uri`/<file>, a path relative to
        the .glb, instead of carrying them in its buffer: big maps stay small
        files, each image its own."""
        self.g = {"asset": {"version": "2.0", "generator": "foc_esc build_assets.py"},
                  "scene": 0, "scenes": [{"nodes": []}], "nodes": [], "meshes": [], "materials": [],
                  "accessors": [], "bufferViews": [], "buffers": [], "textures": [], "images": [],
                  "samplers": [], "skins": []}
        self.blob = bytearray()
        self._images = {}
        self._shared = {} if share else None
        self.image_dir, self.image_uri = image_dir, image_uri

    def _view(self, data, target=None):
        while len(self.blob) % 4:
            self.blob.append(0)
        bv = {"buffer": 0, "byteOffset": len(self.blob), "byteLength": len(data)}
        if target:
            bv["target"] = target
        self.blob += data
        self.g["bufferViews"].append(bv)
        return len(self.g["bufferViews"]) - 1

    def accessor(self, arr, target=None, minmax=False, normalized=False):
        arr = np.ascontiguousarray(arr)
        if arr.ndim == 1:
            arr = arr.reshape(-1, 1)
        n = arr.shape[1]
        key = None
        if self._shared is not None:
            key = (arr.dtype.str, arr.shape, target, minmax, normalized, arr.tobytes())
            if key in self._shared:
                return self._shared[key]
        a = {"bufferView": self._view(arr.tobytes(), target), "componentType": CT_INV[arr.dtype],
             "count": arr.shape[0], "type": TYPE_BY_N[n]}
        if normalized:
            a["normalized"] = True
        if minmax:
            a["min"] = arr.min(0).astype(float).tolist()
            a["max"] = arr.max(0).astype(float).tolist()
        self.g["accessors"].append(a)
        if key is not None:
            self._shared[key] = len(self.g["accessors"]) - 1
        return len(self.g["accessors"]) - 1

    def sampler(self, wrap=10497):
        s = {"magFilter": 9729, "minFilter": 9987, "wrapS": wrap, "wrapT": wrap}
        if s in self.g["samplers"]:
            return self.g["samplers"].index(s)
        self.g["samplers"].append(s)
        return len(self.g["samplers"]) - 1

    def texture(self, key, png_bytes, wrap=10497):
        """A texture of an encoded image: PNG, JPEG or KTX2 (ktx2_io.py; only as a
        file next to the .glb, which Bevy loads by its extension)."""
        if key not in self._images:
            jpeg = png_bytes[:3] == b"\xff\xd8\xff"
            ktx2 = png_bytes[:12] == b"\xabKTX 20\xbb\r\n\x1a\n"
            if self.image_dir:
                import os
                name = key + (".ktx2" if ktx2 else ".jpg" if jpeg else ".png")
                os.makedirs(self.image_dir, exist_ok=True)
                with open(os.path.join(self.image_dir, name), "wb") as fh:
                    fh.write(png_bytes)
                self.g["images"].append({"uri": f"{self.image_uri}/{name}", "name": key})
            else:
                assert not ktx2, "KTX2 images are written as files (image_dir)"
                bv = self._view(png_bytes)
                mime = "image/jpeg" if jpeg else "image/png"
                self.g["images"].append({"bufferView": bv, "mimeType": mime, "name": key})
            self._images[key] = len(self.g["images"]) - 1
        self.g["textures"].append({"source": self._images[key], "sampler": self.sampler(wrap)})
        return len(self.g["textures"]) - 1

    def material(self, m):
        self.g["materials"].append(m)
        return len(self.g["materials"]) - 1

    def node(self, n, root=False):
        self.g["nodes"].append(n)
        i = len(self.g["nodes"]) - 1
        if root:
            self.g["scenes"][0]["nodes"].append(i)
        return i

    def save(self, path):
        # Bevy builds a glTF scene in a world of its own, then copies it into the
        # game archetype by archetype, oldest first. A mesh's parts arrive in that
        # world without visibility components (Mesh3d only asks for them inside the
        # app), so they gain them during the copy. If the archetype of their node
        # is younger than theirs, the node hasn't been copied yet, and Bevy logs a
        # B0004 warning for every part. A node gets a younger archetype when it is
        # the first of its kind (with or without extras) after the first mesh.
        # So when some nodes carry extras, every node gets them, which keeps all
        # nodes in one archetype that the root creates before any mesh.
        if any("extras" in n for n in self.g["nodes"]):
            for n in self.g["nodes"]:
                n.setdefault("extras", {})
        g = {k: v for k, v in self.g.items() if v != []}
        while len(self.blob) % 4:
            self.blob.append(0)
        g["buffers"] = [{"byteLength": len(self.blob)}]
        js = json.dumps(g, separators=(",", ":")).encode()
        while len(js) % 4:
            js += b" "
        out = io.BytesIO()
        total = 12 + 8 + len(js) + 8 + len(self.blob)
        out.write(struct.pack("<4sII", b"glTF", 2, total))
        out.write(struct.pack("<I4s", len(js), b"JSON")); out.write(js)
        out.write(struct.pack("<I4s", len(self.blob), b"BIN\x00")); out.write(self.blob)
        open(path, "wb").write(out.getvalue())
        return total


def quat_to_mat(q):
    x, y, z, w = q
    return np.array([[1 - 2 * (y * y + z * z), 2 * (x * y - z * w), 2 * (x * z + y * w)],
                     [2 * (x * y + z * w), 1 - 2 * (x * x + z * z), 2 * (y * z - x * w)],
                     [2 * (x * z - y * w), 2 * (y * z + x * w), 1 - 2 * (x * x + y * y)]])


def node_local(n):
    if "matrix" in n:
        return np.array(n["matrix"]).reshape(4, 4).T
    m = np.eye(4)
    s = np.array(n.get("scale", [1, 1, 1]))
    m[:3, :3] = quat_to_mat(n.get("rotation", [0, 0, 0, 1])) * s
    m[:3, 3] = n.get("translation", [0, 0, 0])
    return m


def node_globals(g):
    G = {}
    def walk(i, parent):
        G[i] = parent @ node_local(g["nodes"][i])
        for c in g["nodes"][i].get("children", []):
            walk(c, G[i])
    for r in g["scenes"][g.get("scene", 0)]["nodes"]:
        walk(r, np.eye(4))
    return G
