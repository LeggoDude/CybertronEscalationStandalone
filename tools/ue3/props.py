"""UE3 tagged property reader (enough for level actors/components in FoC)."""
import struct
from ue3pkg import Reader


def read_props(pkg, r, end):
    """Returns dict name -> value (raw bytes for unknown types, parsed for simple ones)."""
    props = {}
    while r.o < end:
        ni = struct.unpack_from("<i", r.d, r.o)[0]
        if not (0 <= ni < len(pkg.names)):
            raise ValueError("bad property name index")
        name = pkg.fname(r)
        if name == "None":
            break
        typ = pkg.fname(r)
        size = r.i32()
        idx = r.i32()
        if size < 0 or r.o + size > end + 64:
            raise ValueError("bad property size")
        key = name if idx == 0 else f"{name}[{idx}]"
        if typ == "StructProperty":
            sname = pkg.fname(r)
            data = r.bytes(size)
            props[key] = ("struct", sname, data)
        elif typ == "BoolProperty":
            # FoC (ArVer 566): bool value stored as 4 bytes after the tag
            v = r.i32()
            props[key] = bool(v)
        elif typ == "ByteProperty":
            # no enum name in the tag at this version (added in UE3 v633)
            data = r.bytes(size)
            if size == 8:
                props[key] = pkg.fname(Reader(data))
            else:
                props[key] = data[0] if data else 0
        elif typ == "IntProperty":
            props[key] = struct.unpack("<i", r.bytes(size))[0]
        elif typ == "FloatProperty":
            props[key] = struct.unpack("<f", r.bytes(size))[0]
        elif typ in ("ObjectProperty", "ComponentProperty", "ClassProperty"):
            props[key] = ("obj", struct.unpack("<i", r.bytes(size))[0])
        elif typ == "NameProperty":
            props[key] = pkg.fname(Reader(r.bytes(size)))
        elif typ == "ArrayProperty":
            props[key] = ("array", r.bytes(size))
        else:
            props[key] = (typ, r.bytes(size))
    return props
