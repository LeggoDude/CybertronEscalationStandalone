"""Software preview renderer for the generated .glb files (applies skinning, textures,
simple lighting). usage: python render.py out.png W H file.glb[:yaw_deg[:pitch_deg]] ..."""
import io, math, sys
import numpy as np
from PIL import Image
from gltf_io import Doc, node_globals


def load_images(d):
    import os
    imgs = []
    for im in d.g.get("images", []):
        if "uri" in im:  # an image file next to the .glb (build_map.py)
            src = os.path.join(os.path.dirname(d.path), im["uri"])
            if src.endswith(".ktx2"):
                import ktx2_io
                with open(src, "rb") as fh:
                    imgs.append(ktx2_io.decode(fh.read())[..., :3].astype(np.float32) / 255)
                continue
        else:
            src = io.BytesIO(d.view_bytes(im["bufferView"]))
        imgs.append(np.asarray(Image.open(src).convert("RGB")).astype(np.float32) / 255)
    return imgs


def mesh_data(d):
    G = node_globals(d.g)
    imgs = load_images(d)
    tris = []
    for ni, n in enumerate(d.g["nodes"]):
        if "mesh" not in n or ni not in G:
            continue
        for p in d.g["meshes"][n["mesh"]]["primitives"]:
            at = p["attributes"]
            pos = d.acc(at["POSITION"]).astype(np.float64)
            nrm = d.acc(at["NORMAL"]).astype(np.float64)
            if "skin" in n:
                skin = d.g["skins"][n["skin"]]
                ibm = d.acc(skin["inverseBindMatrices"]).reshape(-1, 4, 4).transpose(0, 2, 1)
                J = np.stack([G[j] @ ibm[k] for k, j in enumerate(skin["joints"])])
                jt = d.acc(at["JOINTS_0"]).astype(int)
                wt = d.acc(at["WEIGHTS_0"]).astype(np.float64)
                M = np.einsum("vk,vkij->vij", wt, J[jt])
                ph = np.concatenate([pos, np.ones((len(pos), 1))], 1)
                pos = np.einsum("vij,vj->vi", M, ph)[:, :3]
                nrm = np.einsum("vij,vj->vi", M[:, :3, :3], nrm)
            else:
                M = G[ni]
                pos = pos @ M[:3, :3].T + M[:3, 3]
                nrm = nrm @ M[:3, :3].T
            nrm /= np.maximum(np.linalg.norm(nrm, axis=1, keepdims=True), 1e-9)
            uv = d.acc(at["TEXCOORD_0"]).astype(np.float64) if "TEXCOORD_0" in at else np.zeros((len(pos), 2))
            idx = d.acc(p["indices"]).reshape(-1, 3)
            mat = d.g["materials"][p["material"]]
            # (Effect meshes: drawn by the demo's effect shader, src/levelfx.rs.)
            if "fx:" in str(mat.get("extras", {}).get("foc", "")):
                continue
            pbr = mat.get("pbrMetallicRoughness", {})
            tex = None
            if "baseColorTexture" in pbr:
                tex = imgs[d.g["textures"][pbr["baseColorTexture"]["index"]]["source"]]
            emi = None
            if "emissiveTexture" in mat:
                emi = imgs[d.g["textures"][mat["emissiveTexture"]["index"]]["source"]]
            unlit = "KHR_materials_unlit" in mat.get("extensions", {})
            # The pickups' outline shells only work with back faces culled (glTF's
            # default); everything else is drawn from both sides here.
            cull = mat.get("name") == "PickupOutline"
            tris.append(dict(pos=pos, nrm=nrm, uv=uv, idx=idx, tex=tex, emi=emi, cull=cull,
                             color=np.array(pbr.get("baseColorFactor", [1, 1, 1, 1])[:3]),
                             emif=np.array(mat.get("emissiveFactor", [0, 0, 0])), unlit=unlit))
    return tris


def sample(img, uv):
    h, w = img.shape[:2]
    u = (uv[..., 0] % 1.0) * (w - 1)
    v = (uv[..., 1] % 1.0) * (h - 1)
    return img[v.astype(int), u.astype(int)]


def render(prims, W, H, eye, target, fov=35, bg=(0.12, 0.13, 0.16)):
    f = target - eye; f /= np.linalg.norm(f)
    r = np.cross(f, [0, 1, 0]); r /= np.linalg.norm(r)
    u = np.cross(r, f)
    t = math.tan(math.radians(fov) / 2)
    img = np.ones((H, W, 3)) * bg
    zb = np.full((H, W), np.inf)
    light = np.array([0.4, 0.8, 0.45]); light /= np.linalg.norm(light)
    for P in prims:
        rel = P["pos"] - eye
        x, y, z = rel @ r, rel @ u, rel @ f
        z = np.where(z < 0.05, np.nan, z)
        sx = (x / (z * t) * (H / 2)) + W / 2
        sy = H / 2 - (y / (z * t) * (H / 2))
        for tri in P["idx"]:
            a, b, c = tri
            zs = z[[a, b, c]]
            if np.isnan(zs).any():
                continue
            xs = sx[[a, b, c]]; ys = sy[[a, b, c]]
            x0, x1 = max(int(xs.min()), 0), min(int(xs.max()) + 1, W)
            y0, y1 = max(int(ys.min()), 0), min(int(ys.max()) + 1, H)
            if x0 >= x1 or y0 >= y1:
                continue
            gx, gy = np.meshgrid(np.arange(x0, x1) + 0.5, np.arange(y0, y1) + 0.5)
            d = (ys[1] - ys[2]) * (xs[0] - xs[2]) + (xs[2] - xs[1]) * (ys[0] - ys[2])
            if abs(d) < 1e-12 or (P.get("cull") and d > 0):
                continue
            l0 = ((ys[1] - ys[2]) * (gx - xs[2]) + (xs[2] - xs[1]) * (gy - ys[2])) / d
            l1 = ((ys[2] - ys[0]) * (gx - xs[2]) + (xs[0] - xs[2]) * (gy - ys[2])) / d
            l2 = 1 - l0 - l1
            inside = (l0 >= 0) & (l1 >= 0) & (l2 >= 0)
            if not inside.any():
                continue
            iz = l0 / zs[0] + l1 / zs[1] + l2 / zs[2]
            zz = 1 / iz
            sub = zb[y0:y1, x0:x1]
            ok = inside & (zz < sub)
            if not ok.any():
                continue
            w0 = (l0 / zs[0]) * zz; w1 = (l1 / zs[1]) * zz; w2 = (l2 / zs[2]) * zz
            uvp = w0[..., None] * P["uv"][a] + w1[..., None] * P["uv"][b] + w2[..., None] * P["uv"][c]
            n = w0[..., None] * P["nrm"][a] + w1[..., None] * P["nrm"][b] + w2[..., None] * P["nrm"][c]
            n /= np.maximum(np.linalg.norm(n, axis=-1, keepdims=True), 1e-9)
            base = sample(P["tex"], uvp) if P["tex"] is not None else np.broadcast_to(P["color"], uvp.shape[:-1] + (3,))
            if P["unlit"]:
                col = base
            else:
                lam = np.abs(n @ light)[..., None]
                col = base * (0.25 + 0.85 * lam)
                if P["emi"] is not None:
                    col = col + sample(P["emi"], uvp) * 1.5
                elif P["emif"].any():
                    col = col + P["emif"] * 0.6
            sub[ok] = zz[ok]
            img[y0:y1, x0:x1][ok] = np.clip(col[ok], 0, 1)
    return (img * 255).astype(np.uint8)


def view(path, yaw, pitch, W, H, dist_mul=2.2):
    prims = mesh_data(Doc(path))
    allp = np.concatenate([p["pos"] for p in prims])
    lo, hi = allp.min(0), allp.max(0)
    c = (lo + hi) / 2
    rad = np.linalg.norm(hi - lo) / 2
    yr, pr = math.radians(yaw), math.radians(pitch)
    eye = c + rad * dist_mul * np.array([math.sin(yr) * math.cos(pr), math.sin(pr), math.cos(yr) * math.cos(pr)])
    return render(prims, W, H, eye, c), lo, hi


if __name__ == "__main__":
    out, W, H = sys.argv[1], int(sys.argv[2]), int(sys.argv[3])
    tiles = []
    for spec in sys.argv[4:]:
        parts = spec.split(":")
        yaw = float(parts[1]) if len(parts) > 1 else 0
        pitch = float(parts[2]) if len(parts) > 2 else 10
        im, lo, hi = view(parts[0], yaw, pitch, W, H)
        print(parts[0], "yaw", yaw, "bbox", lo.round(2), hi.round(2))
        tiles.append(im)
    Image.fromarray(np.concatenate(tiles, 1)).save(out)
