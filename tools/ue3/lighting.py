"""Baked lighting of a FoC level package: light maps, lights, fog and post processing.

FoC levels are lit with Lightmass. Each static mesh component and each group of BSP
nodes (a ModelComponent element) owns a light map:

  * 2D light maps live in shared 1024x1024 atlas pages (LightMapTexture2D, DXT1). The
    pixel data is not in the level package: it is zlib-compressed bulk data in the
    level's "<name>_LM.xxx" companion file. A light map is a pair of pages:
      - DirectionalMaxComponent: R, G, B hold how much light arrives from each of three
        fixed directions around the surface normal (the classic Half-Life 2 basis);
      - NormalizedAverageColor: the colour of that light, scaled so its brightest
        channel is 1.
    Each component's part of a page is its CoordinateScale/CoordinateBias rectangle,
    and each component has its own ScaleVectors (the brightest value it stores).
  * 1D (vertex) light maps store the same two colours per vertex, for meshes without
    light map UVs.

Values are stored gamma-encoded, with a gamma that grows with the light map's range:
linear = ((byte / 255) ^ 2.2) ^ exponent * scale. Each light map stores, for each of
the two colours, its scale (the brightest value it holds) and that exponent, which is
max(1, log2 scale): so a light map brighter than 2 always stores 1.0 as the same byte
(about 186) and squeezes the light above that into the bytes above it. (Seen in the
records: the exponent is log2 of the scale in every light map of the game's maps; and
only this decoding makes the light maps of neighbouring terrain components, each with
its own scale, meet at their edges.)
`flat_irradiance` turns that into plain, non-directional light (what reaches a surface
facing along its normal), which is what Bevy's Lightmap component needs. The
directions it comes from are kept apart (`bake_direction_pages`, `vertex_directions`)
for the surfaces that are shaded as the game shades them, with their normal maps and
their highlights (the Customize garage's, src/levelfx.rs's SurfaceMaterial).

Coordinates of lights are Unreal ones (cm, Z up); build_level.py converts them.
"""
import struct, zlib, io
import numpy as np
from ue3pkg import Package, Reader
from props import read_props
from layout import props_any

GAMMA = 2.2


# ---------------------------------------------------------------------------
# Light map records
# ---------------------------------------------------------------------------

def _vec(r, n):
    return struct.unpack(f"<{n}f", r.bytes(4 * n))


def read_lightmap(r):
    """FLightMapRef (FoC). Returns None, or a dict with kind "2d" / "1d"."""
    typ = r.i32()
    if typ == 0:
        return None
    guids = [r.bytes(16) for _ in range(r.i32())]
    if typ == 2:
        r.i32()  # always 0
        coefs = []
        for _ in range(3):  # NormalizedAverageColor, DirectionalMaxComponent, (unused)
            tex = r.i32()
            scale, exponent = _vec(r, 4), _vec(r, 4)
            coefs.append((tex, scale, exponent))
        cs, cb = _vec(r, 2), _vec(r, 2)
        return {"kind": "2d", "lights": guids, "nac": coefs[0][0], "dirmax": coefs[1][0],
                "nac_scale": coefs[0][1][:3], "dirmax_scale": coefs[1][1][:3],
                "nac_exp": coefs[0][2][:3], "dirmax_exp": coefs[1][2][:3],
                "coord_scale": cs, "coord_bias": cb}
    if typ == 1:
        r.i32()  # owner
        flags, count, size, offset, _key = (r.i32() for _ in range(5))
        assert flags & 1 == 0, "vertex light map stored outside the package"
        data = r.bytes(size)
        if flags & 2:
            data = _decompress(data, 0)
        samples = np.frombuffer(data, np.uint8).reshape(count, 2, 4)[:, :, [2, 1, 0]]  # BGRA -> RGB
        scales = []
        for _ in range(3):
            scales.append((_vec(r, 3), _vec(r, 3)))  # scale, exponent
            r.i32()
        return {"kind": "1d", "lights": guids, "samples": samples,
                "nac_scale": scales[0][0], "dirmax_scale": scales[1][0],
                "nac_exp": scales[0][1], "dirmax_exp": scales[1][1]}
    raise ValueError(f"unknown light map type {typ}")


def static_mesh_lightmaps(pkg):
    """{StaticMeshComponent export index: light map of its LOD 0}"""
    out = {}
    for i, e in enumerate(pkg.exports):
        if pkg.class_name(e) != "StaticMeshComponent":
            continue
        try:
            pr, r = props_any(pkg, e, (8, 16, 4, 12))
        except ValueError:
            continue
        r.u8()
        if r.i32() < 1:
            continue
        [r.i32() for _ in range(r.i32())]  # shadow maps
        [r.i32() for _ in range(r.i32())]  # shadow vertex buffers
        lm = read_lightmap(r)
        if lm:
            out[i + 1] = lm
    return out


def model_lightmaps(pkg, model_index):
    """{BSP node index: light map} for the ModelComponents of one Model."""
    out = {}
    for i, e in enumerate(pkg.exports):
        if pkg.class_name(e) != "ModelComponent":
            continue
        pr, r = props_any(pkg, e, (8,))
        r.i32()
        if r.i32() != model_index:
            continue
        for _ in range(r.i32()):  # elements
            r.i32(); r.i32(); r.i32()  # ?, component, material
            n = r.i32()
            nodes = struct.unpack_from(f"<{n}H", r.d, r.o); r.o += 2 * n
            [r.i32() for _ in range(r.i32())]  # shadow maps
            r.i32()
            lm = read_lightmap(r)
            [r.bytes(16) for _ in range(r.i32())]  # irrelevant lights
            if lm:
                for node in nodes:
                    out[node] = lm
    return out


# ---------------------------------------------------------------------------
# Light map pages
# ---------------------------------------------------------------------------

def _decompress(d, p):
    tag, _bs, _csz, usz = struct.unpack_from("<IIII", d, p); p += 16
    assert tag == 0x9E2A83C1
    blocks, left = [], usz
    while left > 0:
        a, b = struct.unpack_from("<II", d, p); p += 8
        blocks.append(a); left -= b
    out = bytearray()
    for a in blocks:
        out += zlib.decompress(d[p:p + a]); p += a
    return bytes(out)


def _dxt1(data, w, h):
    from PIL import Image
    hdr = (b"DDS " + struct.pack("<7I", 124, 0x1007, h, w, len(data), 0, 1) + b"\0" * 44
           + struct.pack("<2I4s5I", 32, 4, b"DXT1", 0, 0, 0, 0, 0) + struct.pack("<5I", 0x1000, 0, 0, 0, 0))
    return np.asarray(Image.open(io.BytesIO(hdr + data)).convert("RGB"))


def lightmap_pages(pkg, lm_path):
    """{LightMapTexture2D export index: (name, RGB uint8 array)}, read from <level>_LM.xxx."""
    bulk = open(lm_path, "rb").read()
    out = {}
    for i, e in enumerate(pkg.exports):
        if pkg.class_name(e) != "LightMapTexture2D":
            continue
        pr, r = props_any(pkg, e, (4, 0, 8))
        r.i32()  # mip count; mip 0 follows
        flags, _count, size, offset, _key = struct.unpack_from("<iiiii", pkg.data, r.o)
        assert flags & 1, "light map page expected in the _LM file"
        raw = _decompress(bulk, offset) if flags & 2 else bulk[offset:offset + size]
        assert pr.get("Format") == "PF_DXT1"
        out[i + 1] = (e["name"], _dxt1(raw, pr["SizeX"], pr["SizeY"]))
    return out


def decode(a, scale=(1, 1, 1), exponent=(1, 1, 1)):
    """Stored bytes (last axis RGB) -> linear values: ((byte / 255) ^ 2.2) ^ exponent
    * scale."""
    return ((a.astype(np.float32) / 255.0) ** (GAMMA * np.asarray(exponent, np.float32))
            * np.asarray(scale, np.float32))


def flat_irradiance(nac, dirmax, nac_scale, dirmax_scale, nac_exp=(1, 1, 1), dirmax_exp=(1, 1, 1)):
    """Light reaching a surface along its normal, from the two stored colours (arrays
    of bytes, last axis RGB). Lightmass stores, for each of the three basis directions
    (all 54.7 degrees from the normal), the light weighted by its cosine to that
    direction; stock UE3 weights them by their cosine to the normal again (1/sqrt(3)
    each), which for an unperturbed normal sums back to the irradiance, and that is
    what this returns. (FoC's shaders weigh each by (1.127 x that cosine)^2 instead:
    src/lighting.rs's LIGHTMAP_FLAT scales the pages to that.)"""
    d = decode(dirmax, dirmax_scale, dirmax_exp)
    c = decode(nac, nac_scale, nac_exp)
    return c * (d.sum(-1, keepdims=True) / np.sqrt(3.0))


def lightmap_irradiance(lm, nac, dirmax):
    """flat_irradiance with a light map's own scales and exponents."""
    return flat_irradiance(nac, dirmax, lm["nac_scale"], lm["dirmax_scale"],
                           lm.get("nac_exp", (1, 1, 1)), lm.get("dirmax_exp", (1, 1, 1)))


def bake_pages(pages, lightmaps, pad=2):
    """Decodes every 2D light map into irradiance pages.

    Returns {DirectionalMaxComponent page index: float32 HxWx3}. Every light map is
    decoded with its own scales inside its rectangle (grown by `pad` texels so the
    filtering margin around it matches)."""
    by_page = {}
    for lm in lightmaps:
        if lm["kind"] == "2d" and lm["dirmax"] in pages and lm["nac"] in pages:
            by_page.setdefault((lm["dirmax"], lm["nac"]), []).append(lm)
    out = {}
    for (dm_idx, nac_idx), lms in by_page.items():
        dm, nac = pages[dm_idx][1], pages[nac_idx][1]
        h, w = dm.shape[:2]
        irr = flat_irradiance(nac, dm, (1, 1, 1), (1, 1, 1))
        seen = set()
        rects = []
        for lm in lms:
            key = (lm["coord_scale"], lm["coord_bias"], lm["dirmax_scale"], lm["nac_scale"],
                   lm.get("dirmax_exp"), lm.get("nac_exp"))
            if key in seen:
                continue
            seen.add(key)
            (sx, sy), (bx, by) = lm["coord_scale"], lm["coord_bias"]
            x0, y0 = int(round(bx * w)), int(round(by * h))
            x1, y1 = int(round((bx + sx) * w)), int(round((by + sy) * h))
            rects.append((x0, y0, x1, y1, lm))
        for grow in (pad, 0):  # margins first, then the rectangles themselves on top
            for x0, y0, x1, y1, lm in rects:
                xa, ya, xb, yb = max(x0 - grow, 0), max(y0 - grow, 0), min(x1 + grow, w), min(y1 + grow, h)
                irr[ya:yb, xa:xb] = lightmap_irradiance(lm, nac[ya:yb, xa:xb], dm[ya:yb, xa:xb])
        out[dm_idx] = irr
    return out


def direction_weights(dirmax, scale=(1, 1, 1), exponent=(1, 1, 1)):
    """How the light a light map holds is shared between its three basis directions
    (stored bytes, last axis RGB, one per direction): each one's part, summing to 1
    (a third each where there is no light)."""
    d = decode(dirmax, scale, exponent)
    total = d.sum(-1, keepdims=True)
    return np.where(total > 1e-6, d / np.maximum(total, 1e-6), 1.0 / 3.0).astype(np.float32)


def bake_direction_pages(pages, lightmaps, pad=2):
    """The direction weights (direction_weights) of every 2D light map, laid out
    as bake_pages lays out their light: {DirectionalMaxComponent page index:
    float32 HxWx3}."""
    by_page = {}
    for lm in lightmaps:
        if lm["kind"] == "2d" and lm["dirmax"] in pages:
            by_page.setdefault(lm["dirmax"], []).append(lm)
    out = {}
    for dm_idx, lms in by_page.items():
        dm = pages[dm_idx][1]
        h, w = dm.shape[:2]
        dirs = direction_weights(dm)
        seen, rects = set(), []
        for lm in lms:
            key = (lm["coord_scale"], lm["coord_bias"], lm["dirmax_scale"], lm.get("dirmax_exp"))
            if key in seen:
                continue
            seen.add(key)
            (sx, sy), (bx, by) = lm["coord_scale"], lm["coord_bias"]
            x0, y0 = int(round(bx * w)), int(round(by * h))
            x1, y1 = int(round((bx + sx) * w)), int(round((by + sy) * h))
            rects.append((x0, y0, x1, y1, lm))
        for grow in (pad, 0):
            for x0, y0, x1, y1, lm in rects:
                xa, ya, xb, yb = max(x0 - grow, 0), max(y0 - grow, 0), min(x1 + grow, w), min(y1 + grow, h)
                dirs[ya:yb, xa:xb] = direction_weights(dm[ya:yb, xa:xb], lm["dirmax_scale"],
                                                       lm.get("dirmax_exp", (1, 1, 1)))
        out[dm_idx] = dirs
    return out


def vertex_directions(lm):
    """Per-vertex direction weights (N x 3 float32) of a 1D light map."""
    return direction_weights(lm["samples"][:, 1], lm["dirmax_scale"], lm.get("dirmax_exp", (1, 1, 1)))


def vertex_irradiance(lm):
    """Per-vertex irradiance (N x 3 float32) of a 1D light map."""
    s = lm["samples"]
    return lightmap_irradiance(lm, s[:, 0], s[:, 1])


# ---------------------------------------------------------------------------
# Lights, fog, post processing
# ---------------------------------------------------------------------------

def _color(v):
    """FColor property (stored B, G, R, A) -> sRGB bytes (R, G, B)."""
    b, g, r, _a = v[2][:4]
    return (r, g, b)


def _props(pkg, e):
    for st in (8, 16, 4, 12, 0, 30):
        try:
            pr, _ = props_any(pkg, e, (st,))
            return pr
        except ValueError:
            pass
    return {}


def lights(pkg):
    """The level's lights (dict per light; position in Unreal units, direction = the
    light's X axis). Defaults are UE3's when a property is not saved."""
    out = []
    for i, e in enumerate(pkg.exports):
        cls = pkg.class_name(e)
        if cls not in ("PointLightComponent", "SpotLightComponent", "DominantDirectionalLightComponent",
                       "PointLightToggleableComponent", "SpotLightToggleableComponent"):
            continue
        if e["name"].endswith("0") and pkg.obj_name(e["outer"]).startswith("Default__"):
            continue
        pr = _props(pkg, e)
        if "CachedParentToWorld" in pr:
            m = np.frombuffer(pr["CachedParentToWorld"][2], "<f4").reshape(4, 4).astype(np.float64)
        else:
            m = None
        light = {
            "class": cls,
            "name": e["name"],
            "brightness": pr.get("Brightness", 1.0),
            "color": _color(pr["LightColor"]) if "LightColor" in pr else (255, 255, 255),
            "enabled": pr.get("bEnabled", True),
            "guid": pr["LightGuid"][2].hex() if "LightGuid" in pr else None,
        }
        if cls.startswith(("Point", "Spot")):
            if m is None:
                continue
            x = m[0, :3] / max(np.linalg.norm(m[0, :3]), 1e-9)
            light.update(position=tuple(m[3, :3]), direction=tuple(x),
                         radius=pr.get("Radius", 1024.0), falloff=pr.get("FalloffExponent", 2.0))
            if cls.startswith("Spot"):
                light.update(inner=pr.get("InnerConeAngle", 0.0), outer=pr.get("OuterConeAngle", 44.0))
        # A light that lights only what moves (the characters): neither the BSP nor
        # the static meshes are in its lighting channels (UE3's default has both).
        lc = pr.get("LightingChannels")
        if isinstance(lc, tuple) and len(lc) > 2 and isinstance(lc[2], (bytes, bytearray)):
            ch = read_props(pkg, Reader(lc[2]), len(lc[2]))
            light["dynamic_only"] = ch.get("BSP", True) is False and ch.get("Static", True) is False
        else:
            light["dynamic_only"] = False
        out.append(light)
    return out


def directional_light(pkg):
    """The DominantDirectionalLight: direction (Unreal, pointing where the light goes),
    brightness and colour."""
    for i, e in enumerate(pkg.exports):
        if pkg.class_name(e) != "DominantDirectionalLight":
            continue
        pr = _props(pkg, e)
        pitch, yaw, roll = struct.unpack("<iii", pr["Rotation"][2][:12])
        k = np.pi * 2 / 65536.0
        p, y = pitch * k, yaw * k
        d = (np.cos(p) * np.cos(y), np.cos(p) * np.sin(y), np.sin(p))
        comp = pkg.exports[pr["LightComponent"][1] - 1]
        cp = _props(pkg, comp)
        return {"direction": d, "brightness": cp.get("Brightness", 1.0),
                "color": _color(cp["LightColor"]) if "LightColor" in cp else (255, 255, 255)}
    return None


# LightComponent's light-shaft defaults (Engine.xxx's Default__LightComponent), for
# what a light doesn't save.
SHAFT_DEFAULTS = {"BloomScale": 2.0, "BloomThreshold": 0.0, "BloomScreenBlendThreshold": 1.0,
                  "OcclusionDepthRange": 20000.0, "OcclusionMaskDarkness": 0.3}


def shaft_light(pkg):
    """The directional light that draws the level's light shafts (its
    bRenderLightShafts), if it has one: the DominantDirectionalLight's own, or one of
    the DirectionalLightComponents its StaticLightCollectionActor holds (whose
    placements follow the actor's properties: a matrix each, in the order of its
    LightComponents; the light shines along the matrix's X axis). Its direction
    (Unreal, pointing where the light goes), BloomTint (sRGB bytes) and its
    BloomScale, BloomThreshold, BloomScreenBlendThreshold, OcclusionDepthRange
    (Unreal units) and OcclusionMaskDarkness."""
    def settings(cp, direction):
        out = {k: cp.get(k, v) for k, v in SHAFT_DEFAULTS.items()}
        out["direction"] = tuple(float(x) for x in direction)
        out["tint"] = _color(cp["BloomTint"]) if "BloomTint" in cp else (255, 255, 255)
        return out
    for e in pkg.exports:
        cls = pkg.class_name(e)
        if cls == "DominantDirectionalLight":
            pr = _props(pkg, e)
            comp = pkg.exports[pr["LightComponent"][1] - 1]
            cp = _props(pkg, comp)
            if cp.get("bRenderLightShafts"):
                pitch, yaw, _ = struct.unpack("<iii", pr["Rotation"][2][:12])
                k = np.pi * 2 / 65536.0
                p, y = pitch * k, yaw * k
                return settings(cp, (np.cos(p) * np.cos(y), np.cos(p) * np.sin(y), np.sin(p)))
        elif cls == "StaticLightCollectionActor":
            for st in (8, 16, 4, 12, 0, 30):
                try:
                    pr, r = props_any(pkg, e, (st,))
                    break
                except ValueError:
                    continue
            else:
                continue
            lc = pr.get("LightComponents")
            raw = lc[2] if isinstance(lc, tuple) and len(lc) > 2 else (lc[1] if isinstance(lc, tuple) else b"")
            if len(raw) < 4:
                continue
            n = struct.unpack_from("<i", raw, 0)[0]
            comps = struct.unpack_from("<%di" % n, raw, 4)
            matrices = pkg.data[r.o:e["offset"] + e["size"]]
            for k, ci in enumerate(comps):
                if ci <= 0 or 64 * (k + 1) > len(matrices):
                    continue
                ce = pkg.exports[ci - 1]
                if "DirectionalLightComponent" not in pkg.class_name(ce):
                    continue
                cp = _props(pkg, ce)
                if not cp.get("bRenderLightShafts"):
                    continue
                m = np.frombuffer(matrices, "<f4", 16, 64 * k).reshape(4, 4).astype(np.float64)
                x = m[0, :3] / max(np.linalg.norm(m[0, :3]), 1e-9)
                return settings(cp, x)
    return None


def world_fog_scale(pkgs):
    """The FOG_DensityWorldScale of a level's persistent WorldInfo (its
    DefaultPostProcessSettings'): Fall of Cybertron scales every height-fog layer's
    density by it (FSceneRenderer::InitFogConstants: a layer thins light by
    1 - scale x Density each Unreal unit). Of `pkgs` (a level's packages), the
    persistent one is the one whose WorldInfo streams the others in
    (StreamingLevels), else the first with a WorldInfo. Ignition's is 0.3, the other
    Escalation maps' 1. 1 if none says."""
    found = []
    for pkg in pkgs:
        for e in pkg.exports:
            if pkg.class_name(e) not in ("WorldInfo", "TnWorldInfo"):
                continue
            for st in (30, 8, 16, 4, 12, 0):
                try:
                    pr, _ = props_any(pkg, e, (st,))
                except ValueError:
                    continue
                if "DefaultPostProcessSettings" in pr:
                    d = pr["DefaultPostProcessSettings"][2]
                    s = read_props(pkg, Reader(d), len(d))
                    found.append(("StreamingLevels" in pr, s.get("FOG_DensityWorldScale")))
                    break
    found.sort(key=lambda f: not f[0])
    for _, scale in found:
        if isinstance(scale, float):
            return scale
    return 1.0


def fog_and_post(pkg):
    """HeightFog, the post-process volume's height fog and its PostProcessSettings.
    A level can have several HeightFog actors (the game draws up to four layers):
    "HeightFogLayers" lists them all, "HeightFogComponent" is the last."""
    out = {}
    for i, e in enumerate(pkg.exports):
        cls = pkg.class_name(e)
        if cls in ("HeightFogComponent", "PostProcessVolumeHeightFogComponent"):
            pr = _props(pkg, e)
            out[cls] = {"height": pr.get("Height", 0.0), "density": pr.get("Density", 0.00005),
                        "light_brightness": pr.get("LightBrightness", 0.1),
                        "color": _color(pr["LightColor"]) if "LightColor" in pr else (255, 255, 255),
                        "start": pr.get("StartDistance", 0.0)}
            if cls == "HeightFogComponent" and pr.get("bEnabled", True):
                out.setdefault("HeightFogLayers", []).append(out[cls])
        elif cls == "PostProcessVolume":
            pr = _props(pkg, e)
            d = pr["Settings"][2]
            s = read_props(pkg, Reader(d), len(d))
            pp = {}
            for k, v in s.items():
                if isinstance(v, (int, float, bool, str)):
                    pp[k] = v
                elif isinstance(v, tuple) and v[0] == "obj":
                    pp[k] = pkg.obj_name(v[1])
            out["PostProcessSettings"] = pp
    return out


def texture3d(pkg, name):
    """A small uncompressed Texture3D (the colour-grading table), as DxHxWx3 uint8
    (index [b][g][r])."""
    for i, e in enumerate(pkg.exports):
        if e["name"] != name or pkg.class_name(e) != "Texture3D":
            continue
        pr, r = props_any(pkg, e, (4, 0, 8))
        sx, sy, sz = pr["SizeX"], pr["SizeY"], pr["SizeZ"]
        blob = pkg.data[r.o:e["offset"] + e["size"]]
        n = sx * sy * sz * 4
        # the single mip's pixels are the last n bytes before a short trailer
        for p in range(0, len(blob) - n + 1):
            flags, count, size = struct.unpack_from("<iii", blob, p)
            if count == n and size == n and flags & 1 == 0:
                start = p + 20
                px = np.frombuffer(blob[start:start + n], np.uint8).reshape(sz, sy, sx, 4)
                return px[..., [2, 1, 0]], pr.get("Format")
        raise ValueError("no pixel data found in " + name)
    return None, None
