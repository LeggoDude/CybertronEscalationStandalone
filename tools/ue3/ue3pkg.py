"""Minimal reader for Transformers: Fall of Cybertron (UE3 v846/181) packages."""
import struct, zlib


class Reader:
    def __init__(self, data, pos=0):
        self.d = data
        self.o = pos

    def u8(self):
        v = self.d[self.o]; self.o += 1; return v

    def i32(self):
        v = struct.unpack_from("<i", self.d, self.o)[0]; self.o += 4; return v

    def u32(self):
        v = struct.unpack_from("<I", self.d, self.o)[0]; self.o += 4; return v

    def u64(self):
        v = struct.unpack_from("<Q", self.d, self.o)[0]; self.o += 8; return v

    def f32(self):
        v = struct.unpack_from("<f", self.d, self.o)[0]; self.o += 4; return v

    def bytes(self, n):
        v = self.d[self.o:self.o + n]; self.o += n; return v

    def fstring(self):
        n = self.i32()
        if n == 0:
            return ""
        if n < 0:  # UTF-16
            s = self.bytes(-n * 2).decode("utf-16-le", "replace")
        else:
            s = self.bytes(n).decode("latin-1")
        return s.rstrip("\x00")


class Package:
    def __init__(self, path):
        raw = open(path, "rb").read()
        r = Reader(raw)
        assert r.u32() == 0x9E2A83C1
        self.ver = struct.unpack_from("<H", raw, 4)[0]
        self.lic = struct.unpack_from("<H", raw, 6)[0]
        r.o = 8
        for _ in range(4):
            r.u32()  # FoC extra fields (licensee >= 181)
        r.u32()  # unk8
        r.u32()  # headers size
        r.fstring()  # package group
        self.flags = r.u32()
        name_count, name_off, exp_count, exp_off, imp_count, imp_off = [r.u32() for _ in range(6)]
        r.u32()  # depends offset
        r.u32()  # unk38
        r.bytes(16)  # guid
        gens = r.u32()
        for _ in range(gens):
            r.u32(); r.u32(); r.u32()
        r.u32(); r.u32()  # engine / cooker version
        comp = r.u32()
        nchunks = r.u32()
        chunks = [(r.u32(), r.u32(), r.u32(), r.u32()) for _ in range(nchunks)]
        if comp and chunks:
            total = max(a + b for a, b, _, _ in chunks)
            out = bytearray(total)
            out[: chunks[0][0]] = raw[: chunks[0][0]]
            for uo, us, co, cs in chunks:
                p = co
                tag, bs, csz, usz = struct.unpack_from("<IIII", raw, p); p += 16
                assert tag == 0x9E2A83C1
                blocks = []
                left = usz
                while left > 0:
                    a, b = struct.unpack_from("<II", raw, p); p += 8
                    blocks.append((a, b)); left -= b
                w = uo
                for a, b in blocks:
                    out[w:w + b] = zlib.decompress(raw[p:p + a]); p += a; w += b
            self.data = bytes(out)
        else:
            self.data = raw
        r = Reader(self.data, name_off)
        self.names = []
        for _ in range(name_count):
            n = r.i32()
            self.names.append(r.bytes(n).decode("latin-1"))
            r.u64()
        r.o = imp_off
        self.imports = []
        for _ in range(imp_count):
            cpkg = self.fname(r); cls = self.fname(r); pkg = r.i32(); name = self.fname(r)
            self.imports.append({"class_package": cpkg, "class": cls, "outer": pkg, "name": name})
        r.o = exp_off
        self.exports = []
        for _ in range(exp_count):
            e = {}
            e["class"] = r.i32(); e["super"] = r.i32(); e["outer"] = r.i32(); e["name"] = self.fname(r)
            e["archetype"] = r.i32()
            e["flags"] = r.u64()
            e["size"] = r.i32(); e["offset"] = r.i32()
            e["export_flags"] = r.u32()
            more = r.u8()  # FoC: flag, remaining fields present only if set
            if more:
                n = r.i32(); r.bytes(4 * n)  # NetObjectCount
                r.bytes(16)  # guid
                r.u32()  # unk
            self.exports.append(e)

    def fname(self, r):
        i = r.i32(); n = r.i32()
        s = self.names[i] if 0 <= i < len(self.names) else f"?{i}"
        return s if n == 0 else f"{s}_{n - 1}"

    def obj_name(self, idx):
        if idx > 0:
            return self.exports[idx - 1]["name"]
        if idx < 0:
            return self.imports[-idx - 1]["name"]
        return "None"

    def class_name(self, e):
        c = e["class"]
        if c == 0:
            return "Class"
        return self.obj_name(c)

    def full_name(self, idx):
        parts = []
        while idx != 0:
            parts.append(self.obj_name(idx))
            idx = self.exports[idx - 1]["outer"] if idx > 0 else self.imports[-idx - 1]["outer"]
        return ".".join(reversed(parts))
