"""Splits a .glb's binary data between the .glb and an external .bin beside it, so that
no file is larger than a limit (for copying the demo's files somewhere that limits
a file's size: robot.glb is over 40 MB with all sixteen guns).

    python split_glb.py <in.glb> <out.glb> [limit in MB, default 28]

The .glb keeps its JSON and the first part of its binary chunk as buffer 0; the rest
goes into buffer 1, `<out name>.bin` next to it (its "uri"), the buffer views moved
there pointing into it. The cut is made between buffer views (none is split), as near
the middle as possible. Bevy's glTF loader reads such a file as it reads any other
(glTF 2.0 lets a .glb's buffers other than the first live in files of their own),
and so does tools/gltf_io.py. Running it on a .glb that fits does nothing but copy
it."""
import json
import os
import shutil
import struct
import sys


def read_glb(path):
    data = open(path, "rb").read()
    magic, version, length = struct.unpack_from("<4sII", data, 0)
    if magic != b"glTF" or version != 2:
        raise SystemExit(f"{path} isn't a glTF 2.0 .glb")
    off, doc, blob = 12, None, b""
    while off < length:
        clen, ctype = struct.unpack_from("<I4s", data, off)
        chunk = data[off + 8: off + 8 + clen]
        if ctype == b"JSON":
            doc = json.loads(chunk)
        elif ctype == b"BIN\x00":
            blob = chunk
        off += 8 + clen
    return doc, blob


def write_glb(path, doc, blob):
    js = json.dumps(doc, separators=(",", ":")).encode()
    js += b" " * (-len(js) % 4)
    blob = blob + b"\0" * (-len(blob) % 4)
    with open(path, "wb") as f:
        f.write(struct.pack("<4sII", b"glTF", 2, 12 + 8 + len(js) + 8 + len(blob)))
        f.write(struct.pack("<I4s", len(js), b"JSON"))
        f.write(js)
        f.write(struct.pack("<I4s", len(blob), b"BIN\x00"))
        f.write(blob)


def split(src, dst, limit_mb=28.0):
    if os.path.getsize(src) <= limit_mb * 1e6:
        shutil.copyfile(src, dst)
        return None
    doc, blob = read_glb(src)
    if len(doc.get("buffers", [])) != 1 or "uri" in doc["buffers"][0]:
        raise SystemExit(f"{src}: expected one buffer, its binary chunk")
    views = doc["bufferViews"]
    spans = [(v.get("byteOffset", 0), v.get("byteOffset", 0) + v["byteLength"]) for v in views]
    total = len(blob)
    # The view start nearest the middle that no view straddles (4-byte aligned).
    cuts = sorted({s for s, _ in spans if s % 4 == 0 and not any(a < s < b for a, b in spans)})
    if not cuts:
        raise SystemExit(f"{src}: nowhere to cut its buffer")
    cut = min(cuts, key=lambda s: abs(s - total / 2))
    bin_name = os.path.splitext(os.path.basename(dst))[0] + ".bin"
    for v in views:
        start = v.get("byteOffset", 0)
        if start >= cut:
            v["buffer"] = 1
            v["byteOffset"] = start - cut
    doc["buffers"] = [{"byteLength": cut}, {"byteLength": total - cut, "uri": bin_name}]
    write_glb(dst, doc, blob[:cut])
    with open(os.path.join(os.path.dirname(dst) or ".", bin_name), "wb") as f:
        f.write(blob[cut:])
    return bin_name


if __name__ == "__main__":
    if len(sys.argv) < 3:
        raise SystemExit(__doc__)
    limit = float(sys.argv[3]) if len(sys.argv) > 3 else 28.0
    name = split(sys.argv[1], sys.argv[2], limit)
    if name:
        print(f"wrote {sys.argv[2]} ({os.path.getsize(sys.argv[2]) / 1e6:.1f} MB) and {name} "
              f"({os.path.getsize(os.path.join(os.path.dirname(sys.argv[2]) or '.', name)) / 1e6:.1f} MB)")
    else:
        print(f"{sys.argv[1]} fits: copied")
