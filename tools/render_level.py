"""Fast-ish software preview of a big level .glb (frustum cull + splat tiny triangles).
usage: python render_level.py level.glb out.png W H ex ey ez tx ty tz [fov]"""
import math, sys
import numpy as np
from PIL import Image
from gltf_io import Doc
from render import mesh_data, sample


def render(prims, W, H, eye, target, fov=60, bg=(0.35, 0.3, 0.32)):
    f = target - eye; f /= np.linalg.norm(f)
    r = np.cross(f, [0, 1, 0]); r /= np.linalg.norm(r)
    u = np.cross(r, f)
    t = math.tan(math.radians(fov) / 2)
    img = np.ones((H, W, 3)) * bg
    zb = np.full((H, W), np.inf)
    light = np.array([0.35, 0.8, 0.5]); light /= np.linalg.norm(light)
    for P in prims:
        rel = P["pos"] - eye
        x, y, z = rel @ r, rel @ u, rel @ f
        sx = x / (np.maximum(z, 1e-3) * t) * (H / 2) + W / 2
        sy = H / 2 - y / (np.maximum(z, 1e-3) * t) * (H / 2)
        idx = P["idx"]
        zt = z[idx]
        keep = (zt > 0.3).all(1)
        xs_all, ys_all = sx[idx], sy[idx]
        keep &= (xs_all.max(1) >= 0) & (xs_all.min(1) < W) & (ys_all.max(1) >= 0) & (ys_all.min(1) < H)
        # back-face cull (screen-space winding); keep both sides for safety on thin walls? no: cull
        area = (xs_all[:, 1] - xs_all[:, 0]) * (ys_all[:, 2] - ys_all[:, 0]) - (xs_all[:, 2] - xs_all[:, 0]) * (ys_all[:, 1] - ys_all[:, 0])
        keep &= area < 0  # CCW in world -> CW on screen (screen y points down)
        ids = np.nonzero(keep)[0]
        if not len(ids):
            continue
        ext = np.maximum(xs_all[ids].max(1) - xs_all[ids].min(1), ys_all[ids].max(1) - ys_all[ids].min(1))
        small = ids[ext < 1.5]
        big = ids[ext >= 1.5]
        nrm = P["nrm"]
        # splat small triangles at their centroid
        if len(small):
            tri = idx[small]
            cx = xs_all[small].mean(1).astype(int); cy = ys_all[small].mean(1).astype(int); cz = zt[small].mean(1)
            ok = (cx >= 0) & (cx < W) & (cy >= 0) & (cy < H)
            tri, cx, cy, cz = tri[ok], cx[ok], cy[ok], cz[ok]
            uvc = P["uv"][tri].mean(1)
            n = nrm[tri].mean(1); n /= np.maximum(np.linalg.norm(n, axis=1, keepdims=True), 1e-9)
            base = sample(P["tex"], uvc) if P["tex"] is not None else np.broadcast_to(P["color"], (len(tri), 3))
            col = base * (0.3 + 0.8 * np.abs(n @ light))[:, None]
            if P["emi"] is not None:
                col = col + sample(P["emi"], uvc) * 1.5
            order = np.argsort(-cz)  # far first, near overwrites
            for k in order:
                if cz[k] < zb[cy[k], cx[k]]:
                    zb[cy[k], cx[k]] = cz[k]; img[cy[k], cx[k]] = np.clip(col[k], 0, 1)
        for ti in big:
            a, b, c = idx[ti]
            zs = z[[a, b, c]]
            xs = sx[[a, b, c]]; ys = sy[[a, b, c]]
            x0, x1 = max(int(xs.min()), 0), min(int(xs.max()) + 1, W)
            y0, y1 = max(int(ys.min()), 0), min(int(ys.max()) + 1, H)
            if x0 >= x1 or y0 >= y1:
                continue
            gx, gy = np.meshgrid(np.arange(x0, x1) + 0.5, np.arange(y0, y1) + 0.5)
            d = (ys[1] - ys[2]) * (xs[0] - xs[2]) + (xs[2] - xs[1]) * (ys[0] - ys[2])
            if abs(d) < 1e-12:
                continue
            l0 = ((ys[1] - ys[2]) * (gx - xs[2]) + (xs[2] - xs[1]) * (gy - ys[2])) / d
            l1 = ((ys[2] - ys[0]) * (gx - xs[2]) + (xs[0] - xs[2]) * (gy - ys[2])) / d
            l2 = 1 - l0 - l1
            inside = (l0 >= 0) & (l1 >= 0) & (l2 >= 0)
            if not inside.any():
                continue
            zz = 1 / (l0 / zs[0] + l1 / zs[1] + l2 / zs[2])
            sub = zb[y0:y1, x0:x1]
            ok = inside & (zz < sub)
            if not ok.any():
                continue
            w0 = (l0 / zs[0]) * zz; w1 = (l1 / zs[1]) * zz; w2 = (l2 / zs[2]) * zz
            uvp = w0[..., None] * P["uv"][a] + w1[..., None] * P["uv"][b] + w2[..., None] * P["uv"][c]
            n = w0[..., None] * nrm[a] + w1[..., None] * nrm[b] + w2[..., None] * nrm[c]
            n /= np.maximum(np.linalg.norm(n, axis=-1, keepdims=True), 1e-9)
            base = sample(P["tex"], uvp) if P["tex"] is not None else np.broadcast_to(P["color"], uvp.shape[:-1] + (3,))
            col = base * (0.3 + 0.8 * np.abs(n @ light))[..., None]
            if P["emi"] is not None:
                col = col + sample(P["emi"], uvp) * 1.5
            sub[ok] = zz[ok]
            img[y0:y1, x0:x1][ok] = np.clip(col[ok], 0, 1)
    # distance fog like the game
    fog = 1 - np.exp(-np.where(np.isinf(zb), 1e9, zb) / 220.0)
    img = img * (1 - fog[..., None]) + np.array(bg) * fog[..., None]
    return (np.clip(img, 0, 1) * 255).astype(np.uint8)


if __name__ == "__main__":
    path, out, W, H = sys.argv[1], sys.argv[2], int(sys.argv[3]), int(sys.argv[4])
    e = np.array([float(v) for v in sys.argv[5:8]]); tg = np.array([float(v) for v in sys.argv[8:11]])
    fov = float(sys.argv[11]) if len(sys.argv) > 11 else 60
    prims = mesh_data(Doc(path))
    Image.fromarray(render(prims, W, H, e, tg, fov)).save(out)
    print("saved", out)
