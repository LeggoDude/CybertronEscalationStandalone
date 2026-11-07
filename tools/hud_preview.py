"""Renders assets/hud/hud.txt the way the demo draws it, to check the HUD without
running the game: python hud_preview.py <assets folder> <out.png> [width height]

It uses the same model as src/hud.rs: the movie's clips at chosen frames, the
corner groups turned by the screen warp and seen through a 45-degree perspective
camera. The state shown (health, ammo...) is set in STATE below.
"""
import math
import os
import re
import sys

import numpy as np
from PIL import Image

HUD_HEIGHT = 720.0      # the movie's stage height; everything scales with the screen
HUD_FOV_DEG = 45.0      # hud_mc._perspfov
WARP_DEG = 6.0          # screenWarpFactor
SAFE = 0.0              # ScreenSafeFrameX/Y: the PC default puts the corners at the edges


class Lib:
    def __init__(self, path):
        self.dir = os.path.dirname(path)
        self.images, self.fonts, self.glyphs = {}, {}, {}
        self.sprites, self.shapes, self.bars, self.texts, self.lines = {}, {}, {}, {}, {}
        self.symbols = {}
        for raw in open(path):
            if raw.startswith("#") or not raw.strip():
                continue
            if raw.startswith("text "):
                head, text = raw.rstrip("\n").split("|", 1)
                p = head.split()
                self.texts[int(p[1])] = dict(rect=list(map(float, p[2:6])), face=p[6], size=float(p[7]),
                                             color=p[8], align=p[9], spacing=float(p[10]), text=text)
                continue
            p = raw.split()
            k = p[0]
            if k == "image":
                self.images[p[1]] = p[2]
            elif k == "font":
                self.fonts[p[1]] = dict(file=p[2], em=int(p[5]), ascent=float(p[6]), descent=float(p[7]),
                                        leading=float(p[8]))
            elif k == "glyph":
                self.glyphs[(p[1], int(p[2]))] = list(map(float, p[3:]))
            elif k == "quad":
                self.shapes.setdefault(int(p[1]), []).append(
                    dict(image=None if p[2] == "-" else p[2], rect=list(map(float, p[3:7])),
                         uv=list(map(float, p[7:11])), color=list(map(float, p[11:15]))))
            elif k == "line":
                v = list(map(float, p[3:]))
                self.lines.setdefault(int(p[1]), []).append(dict(width=float(p[2]), color=v[:4],
                                                                 pts=list(zip(v[4::2], v[5::2]))))
            elif k == "bar":
                v = list(map(float, p[3:]))
                self.bars[int(p[1])] = dict(image=p[2], start=v[0:4], end=v[4:8], uvm=v[8:14])
            elif k == "sprite":
                self.sprites[int(p[1])] = dict(frames=int(p[2]), labels={}, stops=set(), gotos={}, tracks={},
                                               masks=[])
            elif k == "mask":
                self.sprites[int(p[1])]["masks"].append((int(p[2]), int(p[3]), list(map(float, p[4:8]))))
            elif k == "label":
                self.sprites[int(p[1])]["labels"][p[3]] = int(p[2])
            elif k == "stop":
                self.sprites[int(p[1])]["stops"].add(int(p[2]))
            elif k == "goto":
                self.sprites[int(p[1])]["gotos"][int(p[2])] = p[3]
            elif k == "key":
                sid, frame, depth = int(p[1]), int(p[2]), int(p[3])
                if p[4] == "-":
                    key = None
                else:
                    v = list(map(float, p[6:]))
                    key = dict(child=int(p[4]), name=None if p[5] == "-" else p[5], m=v[0:6],
                               mul=v[6:10], add=[x / 255 for x in v[10:14]], ratio=v[14] / 65535,
                               blend=int(v[15]))
                self.sprites[sid]["tracks"].setdefault(depth, []).append((frame, key))
            elif k == "symbol":
                self.symbols[p[1]] = int(p[2])
            elif k == "root":
                self.root = int(p[1])
        self.cache = {}

    def image(self, name):
        if name not in self.cache:
            self.cache[name] = Image.open(os.path.join(self.dir, self.images[name])).convert("RGBA")
        return self.cache[name]

    def font_image(self, face):
        key = "font:" + face
        if key not in self.cache:
            self.cache[key] = Image.open(os.path.join(self.dir, self.fonts[face]["file"])).convert("RGBA")
        return self.cache[key]


def key_at(track, frame):
    k = None
    for f, key in track:
        if f <= frame:
            k = key
        else:
            break
    return k


# --- 4x4 transforms, composed as Bevy does: translation * rotation * scale -----------

def trs(t=(0, 0, 0), rot=np.eye(3), s=(1, 1, 1)):
    m = np.eye(4)
    m[:3, :3] = rot @ np.diag(s)
    m[:3, 3] = t
    return m


def rot_z(a):
    c, s = math.cos(a), math.sin(a)
    return np.array([[c, -s, 0], [s, c, 0], [0, 0, 1]])


def rot_y(a):
    c, s = math.cos(a), math.sin(a)
    return np.array([[c, 0, s], [0, 1, 0], [-s, 0, c]])


def rot_x(a):
    c, s = math.cos(a), math.sin(a)
    return np.array([[1, 0, 0], [0, c, -s], [0, s, c]])


def flash_local(m):
    """A Flash matrix (x' = a x + c y + tx, y' = b x + d y + ty) as translation,
    rotation about z and scale - the decomposition src/hud.rs uses."""
    a, b, c, d, tx, ty = m
    sx = math.hypot(a, b)
    rot = math.atan2(b, a)
    det = a * d - b * c
    sy = det / sx if sx > 1e-9 else math.hypot(c, d)
    return trs((tx, ty, 0), rot_z(rot), (sx, sy, 1))


def cx_compose(parent, child):
    (pm, pa), (cm, ca) = parent, child
    return ([cm[i] * pm[i] for i in range(4)], [ca[i] * pm[i] + pa[i] for i in range(4)])


class Draw:
    def __init__(self, world, cx, kind, data, blend):
        self.world, self.cx, self.kind, self.data, self.blend = world, cx, kind, data, blend


def collect(lib, sid, world, cx, state, path, out, extra3d=None, clip=None):
    """clip: a mask over this sprite, as (the mask's world matrix, its rectangle)."""
    sp = lib.sprites[sid]
    frame = state.get(path + "#frame", 0)
    if isinstance(frame, str):
        frame = sp["labels"][frame]
    for depth in sorted(sp["tracks"]):
        key = key_at(sp["tracks"][depth], frame)
        if key is None:
            continue
        name = key["name"] or "#%d" % depth
        p = path + "/" + name
        if state.get(p + "#hide"):
            continue
        local = flash_local(key["m"])
        if p in state.get("#pos", {}):
            local[:3, 3] = (*state["#pos"][p], 0)
        rot3 = (extra3d or {}).get(p)
        if rot3 is not None:
            local = local @ trs(rot=rot3)
        # Positions set by script (_x, _y).
        if p + "#x" in state:
            local[0, 3] = state[p + "#x"]
        if p + "#y" in state:
            local[1, 3] = state[p + "#y"]
        w = world @ local
        kcx = state.get(p + "#cx", (key["mul"], key["add"]))
        if p + "#alpha" in state:
            kcx = (kcx[0][:3] + [state[p + "#alpha"]], kcx[1])
        c = cx_compose(cx, kcx)
        child = key["child"]
        kclip = clip
        for md, cd, rect in sp.get("masks", []):
            if md < depth <= cd:
                kclip = (world, rect)  # (an inherited mask is not intersected here)
        if child in lib.sprites:
            collect(lib, child, w, c, state, p, out, extra3d, kclip)
            for att in state.get(p + "#attach", []):
                sym, m = att
                aw = w @ flash_local(m)
                collect(lib, lib.symbols[sym], aw, c, state, p + "/" + sym, out, extra3d)
        elif child in lib.shapes:
            for q in lib.shapes[child]:
                if kclip is not None:
                    q = clip_quad(q, np.linalg.inv(w) @ kclip[0], kclip[1])
                    if q is None:
                        continue
                out.append(Draw(w, c, "quad", q, key["blend"]))
            for ln in lib.lines.get(child, []):
                out.append(Draw(w, c, "line", ln, key["blend"]))
        elif child in lib.bars:
            out.append(Draw(w, c, "bar", (lib.bars[child], key["ratio"]), key["blend"]))
        elif child in lib.texts:
            t = dict(lib.texts[child])
            t["text"] = state.get(p + "#text", t["text"])
            out.append(Draw(w, c, "text", t, key["blend"]))


def clip_quad(q, to_local, rect):
    """The quad cut to a mask rectangle (given in another space, mapped by to_local)."""
    pts = [to_local @ np.array([x, y, 0, 1.0]) for x, y in ((rect[0], rect[1]), (rect[2], rect[3]))]
    cx0, cx1 = sorted((pts[0][0], pts[1][0]))
    cy0, cy1 = sorted((pts[0][1], pts[1][1]))
    x0, y0, x1, y1 = q["rect"]
    nx0, ny0, nx1, ny1 = max(x0, cx0), max(y0, cy0), min(x1, cx1), min(y1, cy1)
    if nx0 >= nx1 or ny0 >= ny1:
        return None
    u0, v0, u1, v1 = q["uv"]
    u = lambda x: u0 + (u1 - u0) * (x - x0) / max(x1 - x0, 1e-6)
    v = lambda y: v0 + (v1 - v0) * (y - y0) / max(y1 - y0, 1e-6)
    return dict(q, rect=[nx0, ny0, nx1, ny1], uv=[u(nx0), v(ny0), u(nx1), v(ny1)])


def render(lib, state, W, H):
    aspect = W / H
    stage_w = HUD_HEIGHT * aspect
    # Perspective: hud_mc's 45 degrees across the stage width, as Flash 3D does.
    dist = (stage_w / 2) / math.tan(math.radians(HUD_FOV_DEG) / 2)
    vfov = 2 * math.atan((HUD_HEIGHT / 2) / dist)
    # Root: Flash space (y down) on a plane dist in front of the camera.
    root = trs((0, 0, -dist), np.eye(3), (1, -1, 1))
    corner_x = stage_w * (1 - SAFE) / 2
    corner_y = HUD_HEIGHT * (1 - SAFE) / 2
    warp = math.radians(WARP_DEG)
    pos = {
        "/topLeft_mc": (-corner_x, -corner_y), "/topRight_mc": (corner_x, -corner_y),
        "/bottomLeft_mc": (-corner_x, corner_y), "/bottomRight_mc": (corner_x, corner_y),
        "/midLeft_mc": (-corner_x, 0), "/midRight_mc": (corner_x, 0),
        "/topCenter_mc": (0, -corner_y), "/bottomCenter_mc": (0, corner_y),
        "/lowGlowBottomLeft_mc": (-stage_w / 2, HUD_HEIGHT / 2),
        "/lowGlowBottomRight_mc": (stage_w / 2, HUD_HEIGHT / 2),
    }
    state = dict(state)
    state["#pos"] = pos
    # Flash's 3D frame has z into the screen; here (y flipped to Flash's) z comes out
    # of it, so a Flash rotation of a about x or y is a rotation of -a here.
    flash_y = lambda a: rot_y(-a)
    flash_x = lambda a: rot_x(-a)
    extra = {
        # updateScreenWarp: _yrotation -warp on the left groups, +warp on the right.
        "/topLeft_mc": flash_y(-warp), "/midLeft_mc": flash_y(-warp), "/bottomLeft_mc": flash_y(-warp),
        "/topRight_mc": flash_y(warp), "/midRight_mc": flash_y(warp), "/bottomRight_mc": flash_y(warp),
        # The damage indicators lie on a plane tilted back by 50 degrees (_xrotation = -50).
        "/midCenter_mc/#19": flash_x(math.radians(-50)),
    }
    draws = []
    collect(lib, lib.root, root, ([1, 1, 1, 1], [0, 0, 0, 0]), state, "", draws, extra)
    f = (H / 2) / math.tan(vfov / 2)
    canvas = Image.new("RGBA", (W, H), (0, 0, 0, 0))

    def project(world, x, y):
        p = world @ np.array([x, y, 0, 1.0])
        return (W / 2 + p[0] / -p[2] * f, H / 2 - p[1] / -p[2] * f)

    for d in draws:
        if d.kind == "quad":
            q = d.data
            x0, y0, x1, y1 = q["rect"]
            u0, v0, u1, v1 = q["uv"]
            img = lib.image(q["image"]) if q["image"] else Image.new("RGBA", (4, 4), (255, 255, 255, 255))
            blit(canvas, img, [project(d.world, *p) for p in ((x0, y0), (x1, y0), (x1, y1), (x0, y1))],
                 (u0, v0, u1, v1), d.cx, q["color"], d.blend)
        elif d.kind == "bar":
            b, ratio = d.data
            r = [b["start"][i] + (b["end"][i] - b["start"][i]) * ratio for i in range(4)]
            x0, x1, y0, y1 = r
            a, bb, c, dd, tx, ty = b["uvm"]
            uv = lambda x, y: (a * x + c * y + tx, bb * x + dd * y + ty)
            u0, v0 = uv(x0, y0)
            u1, v1 = uv(x1, y1)
            blit(canvas, lib.image(b["image"]),
                 [project(d.world, *p) for p in ((x0, y0), (x1, y0), (x1, y1), (x0, y1))],
                 (u0, v0, u1, v1), d.cx, [1, 1, 1, 1], d.blend)
        elif d.kind == "text":
            draw_text(lib, canvas, d, project)
    return canvas




def draw_text(lib, canvas, d, project):
    t = d.data
    face = t["face"] if t["face"] in lib.fonts else "$NormalFont"
    font = lib.fonts[face]
    size = t["size"]
    x0, y0, x1, y1 = t["rect"]
    # <font color='#rrggbb' face='...'> runs, as src/hud.rs draws them.
    chars, stack = [], [([1, 1, 1, 1], face)]
    for part in re.split(r"(<font[^>]*>|</font>)", t["text"]):
        if part.startswith("<font"):
            tint, run_face = stack[-1]
            m = re.search(r"\bcolor\s*=\s*['\"]#([0-9A-Fa-f]{6})", part)
            if m:
                tint = [int(m.group(1)[i:i + 2], 16) / 255 for i in (0, 2, 4)] + [1]
            m = re.search(r"\bface\s*=\s*['\"]([^'\"]*)", part)
            if m and m.group(1) in lib.fonts:
                run_face = m.group(1)
            stack.append((tint, run_face))
        elif part == "</font>":
            if len(stack) > 1:
                stack.pop()
        else:
            chars += [(ch, stack[-1][0], stack[-1][1]) for ch in part]
    width = 0.0
    for ch, _, f in chars:
        g = lib.glyphs.get((f, ord(ch)))
        if g:
            width += g[8] * size + t["spacing"]
    x = x0 + 2 if t["align"] == "left" else (x1 - 2 - width if t["align"] == "right" else (x0 + x1 - width) / 2)
    base = y0 + 2 + font["ascent"] * size
    col = t["color"]
    rgba = [int(col[i:i + 2], 16) / 255 for i in (0, 2, 4, 6)]
    for ch, tint, f in chars:
        g = lib.glyphs.get((f, ord(ch)))
        if not g:
            continue
        u0, v0, u1, v1, gx0, gy0, gx1, gy1, adv = g
        colour = rgba if tint == [1, 1, 1, 1] else tint
        pts = [(x + gx0 * size, base + gy0 * size), (x + gx1 * size, base + gy0 * size),
               (x + gx1 * size, base + gy1 * size), (x + gx0 * size, base + gy1 * size)]
        blit(canvas, lib.font_image(f), [project(d.world, *p) for p in pts], (u0, v0, u1, v1), d.cx, colour, 0)
        x += adv * size + t["spacing"]


def blit(canvas, img, quad, uv, cx, color, blend):
    """Draws the uv rect of img onto the projected quad (perspective-correct)."""
    u0, v0, u1, v1 = uv
    W, H = canvas.size
    iw, ih = img.size
    src = [(u0 * iw, v0 * ih), (u1 * iw, v0 * ih), (u1 * iw, v1 * ih), (u0 * iw, v1 * ih)]
    xs = [p[0] for p in quad]; ys = [p[1] for p in quad]
    bx0, by0 = max(0, int(min(xs)) - 1), max(0, int(min(ys)) - 1)
    bx1, by1 = min(W, int(max(xs)) + 2), min(H, int(max(ys)) + 2)
    if bx1 <= bx0 or by1 <= by0:
        return
    coeffs = homography([(x - bx0, y - by0) for x, y in quad], src)
    # Sample with wrap-around clamped to the image.
    big = img
    patch = big.transform((bx1 - bx0, by1 - by0), Image.PERSPECTIVE, coeffs, Image.BILINEAR)
    # Mask to the quad.
    from PIL import ImageDraw
    mask = Image.new("L", patch.size, 0)
    ImageDraw.Draw(mask).polygon([(x - bx0, y - by0) for x, y in quad], fill=255)
    arr = np.asarray(patch).astype(np.float32) / 255
    mul, add = cx
    for i in range(4):
        arr[..., i] = arr[..., i] * color[i] * mul[i] + add[i]
    arr = np.clip(arr, 0, 1)
    arr[..., 3] *= np.asarray(mask) / 255
    dst = np.asarray(canvas.crop((bx0, by0, bx1, by1))).astype(np.float32) / 255
    a = arr[..., 3:4]
    if blend == 8:  # add
        out = dst.copy()
        out[..., :3] = np.clip(dst[..., :3] + arr[..., :3] * a, 0, 1)
    else:
        out = dst.copy()
        out[..., :3] = arr[..., :3] * a + dst[..., :3] * (1 - a)
        out[..., 3:4] = a + dst[..., 3:4] * (1 - a)
    canvas.paste(Image.fromarray((out * 255).astype(np.uint8), "RGBA"), (bx0, by0))


def homography(dst, src):
    """Coefficients mapping output pixels (dst) to input pixels (src) for PIL."""
    A, B = [], []
    for (x, y), (u, v) in zip(dst, src):
        A.append([x, y, 1, 0, 0, 0, -u * x, -u * y]); B.append(u)
        A.append([0, 0, 0, x, y, 1, -v * x, -v * y]); B.append(v)
    return np.linalg.solve(np.array(A), np.array(B)).tolist()


if __name__ == "__main__":
    assets = sys.argv[1]
    out = sys.argv[2]
    W, H = (int(sys.argv[3]), int(sys.argv[4])) if len(sys.argv) > 4 else (1280, 720)
    lib = Lib(os.path.join(assets, "hud", "hud.txt"))
    STATE = {}
    render(lib, STATE, W, H).save(out)
