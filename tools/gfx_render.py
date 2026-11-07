"""Renders a Scaleform movie's display tree to an image (for reference/checking).

Handles shapes with bitmap and solid fills (gradients as their mean colour),
sprites at a chosen frame, colour transforms and placement matrices. Text fields
are drawn as outlined boxes with their default text when no font is given.

A fill covers the region its edges bound (each edge with the fill on one side
only, Flash's fill0 / fill1), filled even-odd so that holes stay open, and
antialiased (SUPERSAMPLE times over in each direction).
"""
import os
import re

import numpy as np
from PIL import Image, ImageDraw, ImageFont

import gfx_io

SUPERSAMPLE = 4


def _flatten(seg, steps=8):
    if seg[0] == "L":
        return [(seg[1], seg[2]), (seg[3], seg[4])]
    _, x0, y0, cx, cy, x1, y1 = seg
    pts = []
    for i in range(steps + 1):
        t = i / steps
        a = (1 - t) ** 2; b = 2 * (1 - t) * t; c = t * t
        pts.append((a * x0 + b * cx + c * x1, a * y0 + b * cy + c * y1))
    return pts


def _apply(m, x, y):
    a, b, c, d, tx, ty = m
    return (a * x + c * y + tx, b * x + d * y + ty)


def _inv(m):
    a, b, c, d, tx, ty = m
    det = a * d - b * c
    if abs(det) < 1e-12:
        return None
    ia, ib, ic, id_ = d / det, -b / det, -c / det, a / det
    return (ia, ib, ic, id_, -(ia * tx + ic * ty), -(ib * tx + id_ * ty))


def _cx_mul(cx, cx2):
    """Compose colour transforms: cx applied after cx2."""
    (m1, a1), (m2, a2) = cx, cx2
    return ([m1[i] * m2[i] for i in range(4)], [a1[i] + m1[i] * a2[i] for i in range(4)])


CX_IDENT = ([1.0, 1.0, 1.0, 1.0], [0, 0, 0, 0])


def fill_mask(size, polylines, scale, ss=SUPERSAMPLE):
    """The coverage (an "L" image of `size`) of the region `polylines` bound (in the
    movie's units, times `scale` for pixels): even-odd, sampled ss x ss per pixel."""
    W, H = size
    while ss > 1 and W * H * ss * ss > 16_000_000:
        ss //= 2
    Ws, Hs = W * ss, H * ss
    k = scale * ss
    rows = []
    xs = []
    for line in polylines:
        p = np.asarray(line, np.float64) * k
        if len(p) < 2:
            continue
        x0, y0 = p[:-1, 0], p[:-1, 1]
        x1, y1 = p[1:, 0], p[1:, 1]
        lo, hi = np.minimum(y0, y1), np.maximum(y0, y1)
        for i in np.nonzero(hi > lo)[0]:
            # Sample rows (centres at r + 0.5) in [lo, hi).
            r0 = max(int(np.ceil(lo[i] - 0.5)), 0)
            r1 = min(int(np.ceil(hi[i] - 0.5)), Hs)
            if r1 <= r0:
                continue
            yc = np.arange(r0, r1) + 0.5
            t = (yc - y0[i]) / (y1[i] - y0[i])
            rows.append(np.arange(r0, r1))
            xs.append(x0[i] + t * (x1[i] - x0[i]))
    if not rows:
        return Image.new("L", size, 0)
    rows = np.concatenate(rows)
    cols = np.clip(np.ceil(np.concatenate(xs) - 0.5), 0, Ws).astype(np.int64)
    crossings = np.zeros((Hs, Ws + 1), np.int32)
    np.add.at(crossings, (rows, cols), 1)
    inside = (np.cumsum(crossings, axis=1)[:, :Ws] & 1).astype(np.float32)
    cover = inside.reshape(H, ss, W, ss).mean(axis=(1, 3))
    return Image.fromarray(np.round(cover * 255).astype(np.uint8), "L")


def _loops(segs):
    """Join a path's edges into closed loops of points."""
    pts = []
    loops = []
    for s in segs:
        p = _flatten(s)
        if pts and (abs(pts[-1][0] - p[0][0]) > 0.01 or abs(pts[-1][1] - p[0][1]) > 0.01):
            loops.append(pts)
            pts = []
        pts.extend(p if not pts else p[1:])
    if pts:
        loops.append(pts)
    return loops


class Renderer:
    def __init__(self, movie, image_dir, font=None, scale=1.0):
        self.m = movie
        self.dir = image_dir
        self.cache = {}
        self.scale = scale
        self.font_path = font

    def image(self, cid):
        if cid in self.cache:
            return self.cache[cid]
        info = self.m.images.get(cid)
        im = None
        if info:
            name = os.path.splitext(info["name"])[0]
            for n in (name, info.get("export") or name):
                p = os.path.join(self.dir, n + ".png")
                if os.path.exists(p):
                    im = Image.open(p).convert("RGBA")
                    break
        self.cache[cid] = im
        return im

    def draw(self, canvas, cid, m, cx, frames=None, depth=0, path=""):
        c = self.m.chars.get(cid)
        if not c or depth > 30:
            return
        k = c["kind"]
        if k == "shape":
            self._shape(canvas, c, m, cx)
        elif k == "sprite":
            fr = (frames or {}).get(path, 0)
            if isinstance(fr, str):
                fr = c["labels"].get(fr, 0)
            fr = min(fr, len(c["frames"]) - 1)
            for d, pl in sorted(c["frames"][fr].items()):
                if "clip" in pl:
                    continue  # masks: skip
                mm = gfx_io.mat_mul(m, pl.get("matrix", gfx_io.IDENT))
                cc = _cx_mul(cx, pl.get("cxform", CX_IDENT))
                name = pl.get("name", "#%d" % d)
                self.draw(canvas, pl.get("id"), mm, cc, frames, depth + 1, path + "/" + name)
        elif k == "text":
            self._text(canvas, c, m, cx)
        elif k == "image":
            im = self.image(cid)
            if im:
                self._blit(canvas, im, m, cx, None)

    def _blit(self, canvas, im, m, cx, mask):
        s = self.scale
        mm = (m[0] * s, m[1] * s, m[2] * s, m[3] * s, m[4] * s, m[5] * s)
        inv = _inv(mm)
        if inv is None:
            return
        W, H = canvas.size
        a, b, c, d, tx, ty = inv
        layer = im.transform((W, H), Image.AFFINE, (a, c, tx, b, d, ty), resample=Image.BILINEAR)
        layer = _cx_image(layer, cx)
        if mask is not None:
            r, g, bb, al = layer.split()
            from PIL import ImageChops
            al = ImageChops.multiply(al, mask)
            layer = Image.merge("RGBA", (r, g, bb, al))
        canvas.alpha_composite(layer)

    def _shape(self, canvas, c, m, cx):
        s = self.scale
        # Each fill's region: its edges (those with it on one side only).
        regions = {}
        for set_i, f0, f1, ln, segs in c["paths"]:
            if f0 == f1:
                continue
            for fi in (f0, f1):
                if fi:
                    edges = regions.setdefault((set_i, fi), [])
                    edges.extend([_apply(m, x, y) for x, y in _flatten(seg)] for seg in segs)
        for (set_i, fi), edges in regions.items():
            fills, lines = c["styles"][set_i]
            f = fills[fi - 1]
            mask = fill_mask(canvas.size, edges, s)
            if "bitmap" in f:
                im = self.image(f["bitmap"])
                if im is None:
                    continue
                bm = f["matrix"]
                # Bitmap matrix is in twips per pixel.
                bm = (bm[0] / 20, bm[1] / 20, bm[2] / 20, bm[3] / 20, bm[4], bm[5])
                self._blit(canvas, im, gfx_io.mat_mul(m, bm), cx, mask)
            else:
                col = f.get("color")
                if col is None and f.get("stops"):
                    st = f["stops"]
                    col = tuple(sum(x[1][i] for x in st) // len(st) for i in range(4))
                col = _cx_color(col or (255, 255, 255, 255), cx)
                layer = Image.new("RGBA", canvas.size, col[:3] + (0,))
                alpha = mask.point(lambda v, a=col[3]: v * a // 255)
                layer.putalpha(alpha)
                canvas.alpha_composite(layer)
        for set_i, f0, f1, ln, segs in c["paths"]:
            fills, lines = c["styles"][set_i]
            if ln:
                st = lines[ln - 1]
                col = _cx_color(st["color"], cx)
                layer = Image.new("RGBA", canvas.size, (0, 0, 0, 0))
                dr = ImageDraw.Draw(layer)
                for seg in segs:
                    pts = [_apply(m, x, y) for x, y in _flatten(seg)]
                    dr.line([(x * s, y * s) for x, y in pts], fill=col,
                            width=max(1, int(st["width"] * s * abs(m[0]) + 0.5)))
                canvas.alpha_composite(layer)

    def _text(self, canvas, c, m, cx):
        s = self.scale
        x0, x1, y0, y1 = c["bounds"]
        txt = re.sub(r"<[^>]+>", "", c.get("text") or "").replace("&nbsp;", " ")
        col = c.get("color", (255, 255, 255, 255))
        mcol = re.search(r'color="#([0-9A-Fa-f]{6})"', c.get("text") or "")
        if mcol:
            v = int(mcol.group(1), 16)
            col = ((v >> 16) & 255, (v >> 8) & 255, v & 255, col[3])
        col = _cx_color(col, cx)
        p0 = _apply(m, x0, y0); p1 = _apply(m, x1, y1)
        layer = Image.new("RGBA", canvas.size, (0, 0, 0, 0))
        dr = ImageDraw.Draw(layer)
        size = max(6, int(c.get("height", 12) * s * abs(m[3])))
        try:
            font = ImageFont.truetype(self.font_path or "DejaVuSans.ttf", size)
        except Exception:
            font = ImageFont.load_default()
        if txt.strip():
            align = "left"
            if 'align="right"' in (c.get("text") or ""):
                align = "right"
            elif 'align="center"' in (c.get("text") or ""):
                align = "center"
            tw = dr.textlength(txt, font=font)
            X0, X1 = p0[0] * s, p1[0] * s
            x = X0 if align == "left" else (X1 - tw if align == "right" else (X0 + X1 - tw) / 2)
            dr.text((x, p0[1] * s), txt, fill=col, font=font)
        canvas.alpha_composite(layer)


def _cx_color(col, cx):
    mul, add = cx
    return tuple(max(0, min(255, int(col[i] * mul[i] + add[i]))) for i in range(4))


def _cx_image(im, cx):
    mul, add = cx
    if mul == [1.0, 1.0, 1.0, 1.0] and add == [0, 0, 0, 0]:
        return im
    bands = im.split()
    out = [b.point(lambda v, i=i: max(0, min(255, int(v * mul[i] + add[i])))) for i, b in enumerate(bands)]
    return Image.merge("RGBA", out)
