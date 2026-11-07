"""Minimal reader for Scaleform GFX movies (SWF with Scaleform extension tags).

Reads the tags the FoC HUD uses: shapes (with bitmap and solid fills), sprites and
their timelines (PlaceObject2/3, RemoveObject2, FrameLabel), their scaling grids
(DefineScalingGrid), edit-text fields, external images (DefineExternalImage2) and
sub-images, fonts' names, and the exported symbol names.
"""
import struct
import zlib

TAG_NAMES = {
    0: "End", 1: "ShowFrame", 2: "DefineShape", 4: "PlaceObject", 5: "RemoveObject",
    9: "SetBackgroundColor", 10: "DefineFont", 11: "DefineText", 12: "DoAction",
    22: "DefineShape2", 26: "PlaceObject2", 28: "RemoveObject2", 32: "DefineShape3",
    33: "DefineText2", 34: "DefineButton2", 37: "DefineEditText", 39: "DefineSprite",
    43: "FrameLabel", 48: "DefineFont2", 56: "ExportAssets", 57: "ImportAssets",
    59: "DoInitAction", 62: "DefineFontInfo2", 69: "FileAttributes", 70: "PlaceObject3",
    71: "ImportAssets2", 73: "DefineFontAlignZones", 74: "CSMTextSettings",
    75: "DefineFont3", 76: "SymbolClass", 77: "Metadata", 78: "DefineScalingGrid",
    82: "DoABC", 83: "DefineShape4", 84: "DefineMorphShape2", 86: "DefineSceneAndFrameLabelData",
    88: "DefineFontName", 1000: "ExporterInfo", 1001: "FontTextureInfo",
    1002: "DefineExternalGradient", 1003: "DefineGradientMap", 1004: "DefineCompactedFont",
    1005: "DefineExternalSound", 1006: "DefineExternalStreamSound", 1007: "DefineSubImage",
    1008: "DefineExternalImage", 1009: "DefineExternalImage2",
}


class Bits:
    def __init__(self, data, pos=0):
        self.d = data
        self.p = pos * 8

    def align(self):
        self.p = (self.p + 7) & ~7

    @property
    def byte(self):
        self.align()
        return self.p // 8

    def ub(self, n):
        v = 0
        for _ in range(n):
            v = (v << 1) | ((self.d[self.p >> 3] >> (7 - (self.p & 7))) & 1)
            self.p += 1
        return v

    def sb(self, n):
        v = self.ub(n)
        if n and v & (1 << (n - 1)):
            v -= 1 << n
        return v

    def fb(self, n):
        return self.sb(n) / 65536.0

    def u8(self):
        self.align(); v = self.d[self.p >> 3]; self.p += 8; return v

    def u16(self):
        self.align(); v = struct.unpack_from("<H", self.d, self.p >> 3)[0]; self.p += 16; return v

    def s16(self):
        self.align(); v = struct.unpack_from("<h", self.d, self.p >> 3)[0]; self.p += 16; return v

    def u32(self):
        self.align(); v = struct.unpack_from("<I", self.d, self.p >> 3)[0]; self.p += 32; return v

    def f32(self):
        self.align(); v = struct.unpack_from("<f", self.d, self.p >> 3)[0]; self.p += 32; return v

    def fixed8(self):
        return self.s16() / 256.0

    def string(self):
        self.align()
        i = self.p >> 3
        j = self.d.index(b"\0", i)
        self.p = (j + 1) * 8
        return self.d[i:j].decode("latin-1")

    def bytes(self, n):
        self.align(); i = self.p >> 3; self.p += n * 8; return self.d[i:i + n]

    def rect(self):
        self.align()
        n = self.ub(5)
        return [self.sb(n) / 20.0 for _ in range(4)]  # xmin, xmax, ymin, ymax in px

    def matrix(self):
        self.align()
        a, d, b, c = 1.0, 1.0, 0.0, 0.0
        if self.ub(1):
            n = self.ub(5); a = self.fb(n); d = self.fb(n)
        if self.ub(1):
            n = self.ub(5); b = self.fb(n); c = self.fb(n)
        n = self.ub(5)
        tx = self.sb(n) / 20.0; ty = self.sb(n) / 20.0
        # x' = a x + c y + tx ; y' = b x + d y + ty
        return (a, b, c, d, tx, ty)

    def cxform(self, alpha):
        self.align()
        has_add = self.ub(1); has_mul = self.ub(1); n = self.ub(4)
        mul = [1.0, 1.0, 1.0, 1.0]; add = [0, 0, 0, 0]
        k = 4 if alpha else 3
        if has_mul:
            mul = [self.sb(n) / 256.0 for _ in range(k)] + ([] if alpha else [1.0])
        if has_add:
            add = [self.sb(n) for _ in range(k)] + ([] if alpha else [0])
        return (mul, add)

    def rgb(self):
        return (self.u8(), self.u8(), self.u8(), 255)

    def rgba(self):
        return (self.u8(), self.u8(), self.u8(), self.u8())


def mat_mul(m, n):
    """m applied after n."""
    a, b, c, d, tx, ty = m
    a2, b2, c2, d2, tx2, ty2 = n
    return (a * a2 + c * b2, b * a2 + d * b2, a * c2 + c * d2, b * c2 + d * d2,
            a * tx2 + c * ty2 + tx, b * tx2 + d * ty2 + ty)


IDENT = (1.0, 0.0, 0.0, 1.0, 0.0, 0.0)


def read_tags(data, pos):
    """[(code, body_bytes)] until End."""
    out = []
    while pos + 2 <= len(data):
        h = struct.unpack_from("<H", data, pos)[0]; pos += 2
        code, ln = h >> 6, h & 0x3F
        if ln == 0x3F:
            ln = struct.unpack_from("<I", data, pos)[0]; pos += 4
        out.append((code, data[pos:pos + ln]))
        pos += ln
        if code == 0:
            break
    return out


# --- shapes -----------------------------------------------------------------

def _fill_styles(b, shape_ver):
    n = b.u8()
    if n == 0xFF:
        n = b.u16()
    fills = []
    for _ in range(n):
        t = b.u8()
        f = {"type": t}
        if t == 0x00:
            f["color"] = b.rgba() if shape_ver >= 3 else b.rgb()
        elif t in (0x10, 0x12, 0x13):
            f["matrix"] = b.matrix()
            b.align()
            spread = b.ub(2); interp = b.ub(2); cnt = b.ub(4)
            stops = []
            for _ in range(cnt):
                r = b.u8(); c = b.rgba() if shape_ver >= 3 else b.rgb()
                stops.append((r, c))
            f["stops"] = stops
            if t == 0x13:
                f["focal"] = b.fixed8()
        elif t in (0x40, 0x41, 0x42, 0x43):
            f["bitmap"] = b.u16()
            f["matrix"] = b.matrix()
        else:
            raise ValueError("fill type %x" % t)
        fills.append(f)
    return fills


def _line_styles(b, shape_ver):
    n = b.u8()
    if n == 0xFF:
        n = b.u16()
    lines = []
    for _ in range(n):
        w = b.u16() / 20.0
        if shape_ver == 4:
            b.align()
            start_cap = b.ub(2); join = b.ub(2); has_fill = b.ub(1); b.ub(1); b.ub(1); b.ub(1)
            b.ub(5); b.ub(1); end_cap = b.ub(2)
            if join == 2:
                b.u16()
            if has_fill:
                f = _fill_styles_one(b, shape_ver)
                lines.append({"width": w, "fill": f, "color": f.get("color", (255, 255, 255, 255))})
            else:
                lines.append({"width": w, "color": b.rgba()})
        else:
            lines.append({"width": w, "color": b.rgba() if shape_ver >= 3 else b.rgb()})
    return lines


def _fill_styles_one(b, ver):
    # helper: a single FILLSTYLE (used by LINESTYLE2 fill)
    t = b.u8()
    f = {"type": t}
    if t == 0:
        f["color"] = b.rgba()
    elif t in (0x10, 0x12, 0x13):
        f["matrix"] = b.matrix(); b.align()
        b.ub(2); b.ub(2); cnt = b.ub(4)
        f["stops"] = [(b.u8(), b.rgba()) for _ in range(cnt)]
        if t == 0x13:
            b.fixed8()
    elif t in (0x40, 0x41, 0x42, 0x43):
        f["bitmap"] = b.u16(); f["matrix"] = b.matrix()
    return f


def parse_shape(code, body):
    ver = {2: 1, 22: 2, 32: 3, 83: 4}[code]
    b = Bits(body)
    sid = b.u16()
    bounds = b.rect()
    if ver == 4:
        b.rect(); b.u8()
    fills = _fill_styles(b, ver)
    lines = _line_styles(b, ver)
    b.align()
    nf = b.ub(4); nl = b.ub(4)
    # Records -> list of edges per (fill0, fill1, line) with style-table index offsets.
    x = y = 0.0
    f0 = f1 = ln = 0
    paths = []  # (fills, lines, f0, f1, ln, [segments]) segments: ('L',x0,y0,x1,y1) / ('Q',x0,y0,cx,cy,x1,y1)
    style_sets = [(fills, lines)]
    cur = []

    def flush():
        nonlocal cur
        if cur:
            paths.append((len(style_sets) - 1, f0, f1, ln, cur))
        cur = []

    while True:
        is_edge = b.ub(1)
        if not is_edge:
            flags = b.ub(5)
            if flags == 0:
                break
            flush()
            if flags & 1:
                n = b.ub(5); x = b.sb(n) / 20.0; y = b.sb(n) / 20.0
            if flags & 2:
                f0 = b.ub(nf)
            if flags & 4:
                f1 = b.ub(nf)
            if flags & 8:
                ln = b.ub(nl)
            if flags & 16:
                fills2 = _fill_styles(b, ver); lines2 = _line_styles(b, ver)
                style_sets.append((fills2, lines2))
                b.align()
                nf = b.ub(4); nl = b.ub(4)
                f0 = f1 = ln = 0
        else:
            straight = b.ub(1)
            n = b.ub(4) + 2
            if straight:
                general = b.ub(1)
                if general:
                    dx = b.sb(n) / 20.0; dy = b.sb(n) / 20.0
                else:
                    vert = b.ub(1)
                    if vert:
                        dx = 0.0; dy = b.sb(n) / 20.0
                    else:
                        dx = b.sb(n) / 20.0; dy = 0.0
                cur.append(("L", x, y, x + dx, y + dy))
                x += dx; y += dy
            else:
                cdx = b.sb(n) / 20.0; cdy = b.sb(n) / 20.0
                adx = b.sb(n) / 20.0; ady = b.sb(n) / 20.0
                cx, cy = x + cdx, y + cdy
                nx, ny = cx + adx, cy + ady
                cur.append(("Q", x, y, cx, cy, nx, ny))
                x, y = nx, ny
    flush()
    return {"id": sid, "kind": "shape", "bounds": bounds, "styles": style_sets, "paths": paths}


# --- sprites & placement ----------------------------------------------------

def parse_place(code, body, swf_ver=8):
    b = Bits(body)
    if code == 4:
        cid = b.u16(); depth = b.u16(); m = b.matrix()
        return {"depth": depth, "id": cid, "matrix": m, "move": False}
    flags = b.u8()
    flags2 = b.u8() if code == 70 else 0
    depth = b.u16()
    r = {"depth": depth, "move": bool(flags & 1)}
    if code == 70 and (flags2 & 0x08 or (flags2 & 0x10 and flags & 2)):
        r["class"] = b.string()
    if flags & 2:
        r["id"] = b.u16()
    if flags & 4:
        r["matrix"] = b.matrix()
    if flags & 8:
        r["cxform"] = b.cxform(True)
    if flags & 16:
        r["ratio"] = b.u16()
    if flags & 32:
        r["name"] = b.string()
    if flags & 64:
        r["clip"] = b.u16()
    if code == 70:
        if flags2 & 1:
            n = b.u8()  # filter list - stop parsing further fields
            r["filters"] = n
            return r
        if flags2 & 2:
            r["blend"] = b.u8()
    return r


def parse_timeline(tags):
    """Frames: list of dict(depth -> placement) snapshots, plus labels."""
    frames = []
    labels = {}
    display = {}
    for code, body in tags:
        if code in (4, 26, 70):
            pl = parse_place(code, body)
            d = pl["depth"]
            if pl.get("move") and d in display:
                old = dict(display[d])
                old.update({k: v for k, v in pl.items() if k != "move"})
                if "id" in pl and pl["id"] != display[d].get("id"):
                    old = {k: v for k, v in pl.items() if k != "move"}
                display[d] = old
            else:
                display[d] = {k: v for k, v in pl.items() if k != "move"}
        elif code in (5, 28):
            b = Bits(body)
            if code == 5:
                b.u16()
            d = b.u16()
            display.pop(d, None)
        elif code == 43:
            labels[Bits(body).string()] = len(frames)
        elif code == 1:
            frames.append({k: dict(v) for k, v in display.items()})
    if not frames:
        frames.append({k: dict(v) for k, v in display.items()})
    return frames, labels


def parse_edit_text(body):
    b = Bits(body)
    cid = b.u16(); bounds = b.rect()
    b.align()
    has_text = b.ub(1); word_wrap = b.ub(1); multiline = b.ub(1); password = b.ub(1)
    readonly = b.ub(1); has_color = b.ub(1); has_max = b.ub(1); has_font = b.ub(1)
    has_font_class = b.ub(1); auto = b.ub(1); has_layout = b.ub(1); no_sel = b.ub(1)
    border = b.ub(1); was_static = b.ub(1); html = b.ub(1); use_outlines = b.ub(1)
    r = {"id": cid, "kind": "text", "bounds": bounds, "html": bool(html)}
    if has_font:
        r["font"] = b.u16()
    if has_font_class:
        r["font_class"] = b.string()
    if has_font or has_font_class:
        r["height"] = b.u16() / 20.0
    if has_color:
        r["color"] = b.rgba()
    if has_max:
        r["max"] = b.u16()
    if has_layout:
        r["align"] = b.u8(); r["left"] = b.u16() / 20.0; r["right"] = b.u16() / 20.0
        r["indent"] = b.u16() / 20.0; r["leading"] = b.s16() / 20.0
    r["var"] = b.string()
    if has_text:
        r["text"] = b.string()
    return r


def load(path):
    data = open(path, "rb").read()
    sig = data[:3]
    if sig in (b"CFX", b"CWS"):
        data = data[:8] + zlib.decompress(data[8:])
    ver = data[3]
    b = Bits(data, 8)
    frame = b.rect()
    b.align()
    pos = b.byte + 4  # frame rate, frame count
    tags = read_tags(data, pos)
    return ver, frame, tags


class Movie:
    def __init__(self, path):
        self.ver, self.frame, self.tags = load(path)
        self.chars = {}
        self.exports = {}
        self.images = {}   # id -> dict(name, w, h)
        self.sub_images = {}
        self.fonts = {}
        # Sprites' scaling grids (DefineScalingGrid: 9-slice scaling), id -> x0, x1,
        # y0, y1 (px).
        self.grids = {}
        self.labels_root = {}
        for code, body in self.tags:
            self._define(code, body)
        self.root_frames, self.labels_root = parse_timeline(self.tags)

    def _define(self, code, body):
        if code in (2, 22, 32, 83):
            s = parse_shape(code, body)
            self.chars[s["id"]] = s
        elif code == 39:
            b = Bits(body)
            sid = b.u16(); nframes = b.u16()
            sub = read_tags(body, 4)
            frames, labels = parse_timeline(sub)
            self.chars[sid] = {"id": sid, "kind": "sprite", "frames": frames, "labels": labels,
                               "tags": sub}
        elif code == 37:
            t = parse_edit_text(body)
            self.chars[t["id"]] = t
        elif code == 78:
            b = Bits(body)
            cid = b.u16()
            self.grids[cid] = b.rect()
        elif code == 56:
            b = Bits(body)
            n = b.u16()
            for _ in range(n):
                cid = b.u16(); name = b.string()
                self.exports[name] = cid
        elif code == 1009:
            b = Bits(body)
            cid = b.u32(); fmt = b.u16(); w = b.u16(); h = b.u16()
            exp = b.bytes(b.u8()).decode("latin-1")
            name = b.bytes(b.u8()).decode("latin-1")
            self.images[cid] = {"id": cid, "fmt": fmt, "w": w, "h": h, "export": exp, "name": name}
            self.chars[cid] = {"id": cid, "kind": "image", **self.images[cid]}
        elif code == 1008:
            b = Bits(body)
            cid = b.u32(); bt = b.u16(); fmt = b.u16(); w = b.u16(); h = b.u16()
            name = b.string()
            self.images[cid] = {"id": cid, "fmt": fmt, "w": w, "h": h, "name": name}
        elif code == 1007:
            b = Bits(body)
            cid = b.u16(); img = b.u16(); x1, y1, x2, y2 = b.u16(), b.u16(), b.u16(), b.u16()
            self.sub_images[cid] = {"image": img, "rect": (x1, y1, x2, y2)}
        elif code in (48, 75):
            b = Bits(body)
            fid = b.u16(); flags = b.u8(); b.u8(); nl = b.u8()
            name = b.bytes(nl).rstrip(b"\0").decode("latin-1")
            self.fonts[fid] = name
            self.chars[fid] = {"id": fid, "kind": "font", "name": name}
        elif code == 1004:
            b = Bits(body)
            fid = b.u16()
            self.chars[fid] = {"id": fid, "kind": "font", "name": "compacted"}


# --- fonts --------------------------------------------------------------------

def _glyph_edges(b):
    """Contours of one glyph SHAPE (fill style 1), in pixels of a 1024 px em."""
    b.align()
    nf = b.ub(4); nl = b.ub(4)
    x = y = 0.0
    loops = []
    cur = []
    while True:
        if not b.ub(1):
            flags = b.ub(5)
            if flags == 0:
                break
            if flags & 1:
                if cur:
                    loops.append(cur)
                n = b.ub(5); x = b.sb(n) / 20.0; y = b.sb(n) / 20.0
                cur = [(x, y)]
            if flags & 2:
                b.ub(nf)
            if flags & 4:
                b.ub(nf)
            if flags & 8:
                b.ub(nl)
            if flags & 16:
                raise ValueError("new styles in a glyph")
        else:
            if not cur:
                cur = [(x, y)]
            if b.ub(1):
                n = b.ub(4) + 2
                if b.ub(1):
                    dx = b.sb(n) / 20.0; dy = b.sb(n) / 20.0
                elif b.ub(1):
                    dx = 0.0; dy = b.sb(n) / 20.0
                else:
                    dx = b.sb(n) / 20.0; dy = 0.0
                x += dx; y += dy
                cur.append((x, y))
            else:
                n = b.ub(4) + 2
                cx = x + b.sb(n) / 20.0; cy = y + b.sb(n) / 20.0
                nx = cx + b.sb(n) / 20.0; ny = cy + b.sb(n) / 20.0
                for i in range(1, 7):
                    t = i / 6.0
                    cur.append(((1 - t) ** 2 * x + 2 * (1 - t) * t * cx + t * t * nx,
                                (1 - t) ** 2 * y + 2 * (1 - t) * t * cy + t * t * ny))
                x, y = nx, ny
    if cur:
        loops.append(cur)
    return [l for l in loops if len(l) > 2]


def parse_font3(body):
    """DefineFont3: name, layout and glyphs {code: (loops, advance)} in a 1024 px em."""
    b = Bits(body)
    fid = b.u16(); flags = b.u8(); b.u8()
    name = b.bytes(b.u8()).rstrip(b"\0").decode("latin-1")
    n = b.u16()
    wide = bool(flags & 0x08)
    base = b.byte
    offs = [b.u32() if wide else b.u16() for _ in range(n)]
    code_off = b.u32() if wide else b.u16()
    loops = []
    for i in range(n):
        gb = Bits(body, base + offs[i])
        loops.append(_glyph_edges(gb))
    b = Bits(body, base + code_off)
    codes = [b.u16() for _ in range(n)]
    font = {"id": fid, "name": name, "bold": bool(flags & 1), "glyphs": {}}
    if flags & 0x80:
        font["ascent"] = b.u16() / 20.0
        font["descent"] = b.u16() / 20.0
        font["leading"] = b.s16() / 20.0
        # SWF says SI16, but wide glyphs (Energon Numbers' infinity sign, 2 ems)
        # only fit unsigned, as Scaleform reads them.
        adv = [b.u16() / 20.0 for _ in range(n)]
    else:
        font["ascent"], font["descent"], font["leading"] = 1024.0 * 0.8, 1024.0 * 0.2, 0.0
        adv = [1024.0 * 0.5] * n
    for i in range(n):
        font["glyphs"][codes[i]] = (loops[i], adv[i])
    return font
