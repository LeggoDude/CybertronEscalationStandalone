"""The parts of a level build that tools/build_map.py (the Escalation maps) and
tools/build_garage.py (the customization garage) share: texture-set materials
(Mats), collision meshes, the level's BSP floors and walls (read straight from its
Model object: umodel can't export BSP), its baked light maps (Lighting) and its
lights, fog and post-process settings (write_lights).

Baked lighting: every light-mapped node of a level's glb carries glTF "extras" of the
form {"lightmap": "<page path> x0 y0 x1 y1"}: the page image (relative to assets) and
the rectangle of it that the mesh's second UV set (TEXCOORD_1) spans. Page texels are
the light reaching the surface divided by LIGHTMAP_RANGE, stored as sRGB. Vertex-lit
meshes get their light as COLOR_0 (relative to their brightest vertex) and an all-zero
TEXCOORD_1 pointing at one texel of the "vertex" page, which holds that brightest light.

Coordinates: Unreal (x, y, z) in cm -> glTF/Bevy (x, z, y) in m, exactly like umodel's
exporter, so umodel meshes and the level line up without any extra rotation.
"""
import io, os, sys
import numpy as np
from PIL import Image

sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "ue3"))
sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), "ue3"))
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import lighting  # noqa: E402

MAX_TEX = int(os.environ.get("FOC_LEVEL_TEX", "1024"))
# The umodel export, the level data and the assets folder: each builder sets them
# (configure()).
SRC = DATA = OUT = ST = TX = None
EXTRA_TX = []


def configure(src, data, out):
    global SRC, DATA, OUT, ST, TX, EXTRA_TX
    SRC, DATA, OUT = src, data, out
    ST = os.path.join(SRC, "StaticMesh3")
    TX = os.path.join(SRC, "Texture2D")
    EXTRA_TX = [os.path.join(os.path.dirname(SRC), d, "Texture2D") for d in ("ENV_AllShader_p", "ENV_GLB_Textures")]

# Unreal -> glTF basis change (swap Y/Z) and cm -> m.
P = np.array([[1, 0, 0], [0, 0, 1], [0, 1, 0]], np.float64)

# Light map texels hold light / LIGHTMAP_RANGE (lighting.rs scales them back up).
LIGHTMAP_RANGE = 4.0
VERTEX_PAGE = 32  # the vertex-light page: one texel per vertex-lit prop


def jpg(arr, q=85, max_size=None):
    b = io.BytesIO()
    im = Image.fromarray(arr)
    if max_size and max(im.size) > max_size:
        s = max_size / max(im.size)
        im = im.resize((max(1, int(im.size[0] * s)), max(1, int(im.size[1] * s))), Image.LANCZOS)
    im.save(b, "JPEG", quality=q, optimize=True)
    return b.getvalue()


def load_rgba(path):
    im = Image.open(path).convert("RGBA")
    if max(im.size) > MAX_TEX:
        s = MAX_TEX / max(im.size)
        im = im.resize((max(1, int(im.size[0] * s)), max(1, int(im.size[1] * s))), Image.LANCZOS)
    return np.asarray(im)


def find_tex(name):
    for d in [TX] + EXTRA_TX:
        p = os.path.join(d, name + ".png")
        if os.path.exists(p):
            return p
    return None


class Mats:
    """Converts FoC packed TEXSETs into glTF materials (JPEG images to keep the file small)."""

    def __init__(self, w, texsets):
        self.w = w
        self.texsets = texsets
        self.cache = {}
        self.missing = set()
        bases = set()
        for f in os.listdir(TX):
            if f.endswith("_Color_NormX.png"):
                bases.add(f[: -len("_Color_NormX.png")])
        self.bases = bases

    def texset_for(self, mat):
        if mat in ALIASES:
            return ALIASES[mat]
        if mat in self.texsets and self.texsets[mat] in self.bases:
            return self.texsets[mat]
        # fall back on a name match (MyThing_MATINST -> MyThing_..._TEXSET)
        stem = mat
        for suf in ("_MAT_INST", "_MATINST", "_Mat_INST", "_matINST", "_Mat", "_MAT", "_mat"):
            if stem.endswith(suf):
                stem = stem[: -len(suf)]
        stem = stem.replace("ENV_KON_", "")
        cands = sorted(b for b in self.bases if b.lower().startswith(stem.lower()))
        return cands[0] if cands else None

    def get(self, mat):
        if mat in self.cache:
            return self.cache[mat]
        w = self.w
        base = self.texset_for(mat) if mat else None
        if base:
            c = load_rgba(os.path.join(TX, base + "_Color_NormX.png"))
            h, wd = c.shape[:2]
            m = np.asarray(Image.open(os.path.join(TX, base + "_Masks_NormY.png")).convert("RGBA").resize((wd, h), Image.BILINEAR))
            color = c[..., :3].copy()
            nx = c[..., 3].astype(np.float32) / 127.5 - 1
            ny = -(m[..., 3].astype(np.float32) / 127.5 - 1)
            nz = np.sqrt(np.clip(1 - nx * nx - ny * ny, 0, 1))
            normal = np.clip(np.stack([nx, ny, nz], -1) * 127.5 + 127.5, 0, 255).astype(np.uint8)
            spec = m[..., 1].astype(np.float32) / 255
            mr = np.stack([np.zeros_like(spec), np.clip(0.9 - 0.65 * spec, 0.25, 0.95), np.clip(0.2 + 0.5 * spec, 0, 1)], -1)
            mr = (mr * 255).astype(np.uint8)
            glow = np.clip((m[..., 0].astype(np.float32) - 140) / 115, 0, 1)
            gm = {"name": mat,
                  "pbrMetallicRoughness": {"baseColorTexture": {"index": w.texture(base + "_c", jpg(color))},
                                           "metallicRoughnessTexture": {"index": w.texture(base + "_mr", jpg(mr, 80, 512))},
                                           # Not metallic: the game's diffuse light is never dimmed by
                                           # shininess (and the baked light maps assume full diffuse).
                                           "metallicFactor": 0.0, "roughnessFactor": 1.0},
                  "normalTexture": {"index": w.texture(base + "_n", jpg(normal, 88, 512))}}
            frac = (glow > 0.25).mean()
            if 0.0005 < frac < 0.35:
                em = (color.astype(np.float32) * glow[..., None] * 1.2).clip(0, 255).astype(np.uint8)
                gm["emissiveTexture"] = {"index": w.texture(base + "_e", jpg(em, 80, 512))}
                gm["emissiveFactor"] = [1, 1, 1]
                gm["extensions"] = {"KHR_materials_emissive_strength": {"emissiveStrength": 2.5}}
                w.g.setdefault("extensionsUsed", [])
                if "KHR_materials_emissive_strength" not in w.g["extensionsUsed"]:
                    w.g["extensionsUsed"].append("KHR_materials_emissive_strength")
        elif mat and "BSP_Base" in mat and find_tex("Grid_Grey"):
            g = load_rgba(find_tex("Grid_Grey"))[..., :3]
            gm = {"name": mat, "pbrMetallicRoughness": {"baseColorTexture": {"index": w.texture("grid", jpg(g))},
                                                        "metallicFactor": 0.0, "roughnessFactor": 0.7}}
        else:
            self.missing.add(mat)
            gm = {"name": mat or "none", "pbrMetallicRoughness": {"baseColorFactor": [0.32, 0.31, 0.33, 1], "metallicFactor": 0.0, "roughnessFactor": 0.6}}
        self.cache[mat] = w.material(gm)
        return self.cache[mat]


# Texture sets the material table doesn't resolve.
ALIASES = {"BigCables_Off_MATINST": "BigCable_TEXSET", "CellCurvedPillar_EmisBright_MAT_INST": "CellCurvedPillar_TEXSET"}
COL_CELL = float(os.environ.get("FOC_COL_CELL", "0.6"))
_col_cache = {}


def collision_mesh(name, d, scale=1.0):
    """Vertex-clustering simplification of a mesh (local space) for collision, its
    vertices merged within COL_CELL of each other where it is placed `scale` times
    its size or larger (at smaller sizes, as at its own: a mesh blown up to cliffs
    keeps its shape)."""
    scale = round(max(float(scale), 1.0), 1)
    key = (name, scale)
    if key in _col_cache:
        return _col_cache[key]
    vs, ts = [], []
    off = 0
    for p in d.g["meshes"][0]["primitives"]:
        if d.g["accessors"][p["indices"]]["count"] == 0:
            continue
        v = d.acc(p["attributes"]["POSITION"]).astype(np.float64)
        vs.append(v); ts.append(d.acc(p["indices"]).reshape(-1, 3).astype(np.int64) + off); off += len(v)
    if not vs:
        _col_cache[key] = None
        return None
    v = np.concatenate(vs); t = np.concatenate(ts)
    q = np.round(v / (COL_CELL / scale)).astype(np.int64)
    uq, inv = np.unique(q, axis=0, return_inverse=True)
    inv = inv.reshape(-1)
    # representative position: mean of the cluster
    rep = np.zeros((len(uq), 3)); cnt = np.zeros(len(uq))
    np.add.at(rep, inv, v); np.add.at(cnt, inv, 1)
    rep /= cnt[:, None]
    tt = inv[t]
    ok = (tt[:, 0] != tt[:, 1]) & (tt[:, 1] != tt[:, 2]) & (tt[:, 0] != tt[:, 2])
    tt = tt[ok]
    _, keep = np.unique(np.sort(tt, 1), axis=0, return_index=True)
    tt = tt[np.sort(keep)]
    _col_cache[key] = (rep, tt)
    return _col_cache[key]


def last(path):
    return path.split(".")[-1] if path else None


def split_tri(tri, max_edge=6.0):
    """Recursively halves the longest edge until every edge is shorter than max_edge."""
    out, stack = [], [tri]
    while stack:
        t = stack.pop()
        e = [np.linalg.norm(t[(i + 1) % 3][0] - t[i][0]) for i in range(3)]
        i = int(np.argmax(e))
        if e[i] <= max_edge:
            out.append(t)
            continue
        a, b, c = t[i], t[(i + 1) % 3], t[(i + 2) % 3]
        m = ((a[0] + b[0]) / 2, (a[1] + b[1]) / 2)
        stack.append([a, m, c])
        stack.append([m, b, c])
    return out


def bsp_geometry(pkg_path, node_lightmaps, pages, model, split=True):
    """BSP polygons grouped by (material, light map page). The UV is 4 wide: the texture
    UV, then the position in the light map page (0, 0 when not light-mapped). With
    `split`, big polygons are cut into pieces at most 6 m across."""
    import bsp
    _, vectors, points, nodes, surfs, verts, shadow_uv = bsp.extract(pkg_path, model, with_shadow_uv=True)
    node_lightmaps = node_lightmaps or {}
    groups = {}
    for ni, (vpool, isurf, nv, plane) in enumerate(nodes):
        if nv < 3 or isurf < 0:
            continue
        s = surfs[isurf]
        idx = verts[vpool:vpool + nv]
        if len(idx) < nv or idx.max() >= len(points):
            continue
        Pu = points[idx].astype(np.float64)
        base = points[s["base"]].astype(np.float64)
        tu = vectors[s["u"]].astype(np.float64)
        tv = vectors[s["v"]].astype(np.float64)
        uv = np.stack([(Pu - base) @ tu, (Pu - base) @ tv], 1) / 128.0
        lm = node_lightmaps.get(ni)
        page = None
        luv = np.zeros((nv, 2))
        if lm is not None and lm["kind"] == "2d" and lm["dirmax"] in pages:
            su = shadow_uv[vpool:vpool + nv].astype(np.float64)
            if np.isfinite(su).all():
                page = lm["dirmax"]
                luv = su * lm["coord_scale"] + lm["coord_bias"]
        uv = np.concatenate([uv, luv], 1)
        n_ue = np.array(plane[:3], np.float64)
        mat_name = last(s["material"])
        g = groups.setdefault((mat_name, page), {"pos": [], "nrm": [], "uv": [], "tan": []})
        pos = (Pu @ P.T) * 0.01
        n = P @ n_ue
        t = P @ tu
        t = t - n * (t @ n)
        t /= max(np.linalg.norm(t), 1e-9)
        # bitangent sign: does V follow n x t?
        b = P @ tv
        sign = 1.0 if np.cross(n, t) @ b >= 0 else -1.0
        for k in range(1, nv - 1):
            tri = [0, k, k + 1]
            a, bb, c = pos[tri]
            if np.cross(bb - a, c - a) @ n < 0:  # make CCW around the normal (glTF front face)
                tri = [0, k + 1, k]
            # split big polygons into pieces (better vertex lighting/fog and a tighter collision grid)
            pieces = [[(pos[j], uv[j]) for j in tri]] if mat_name == "BSP_Base" or not split else split_tri([(pos[j], uv[j]) for j in tri])
            for (p0, u0), (p1, u1), (p2, u2) in pieces:
                for pp, uu in ((p0, u0), (p1, u1), (p2, u2)):
                    g["pos"].append(pp); g["uv"].append(uu); g["nrm"].append(n); g["tan"].append([*t, sign])
    return groups


def srgb8(v):
    """Linear 0-1 values -> sRGB bytes."""
    v = np.clip(v, 0, 1)
    return np.round(np.where(v <= 0.0031308, v * 12.92, 1.055 * np.power(v, 1 / 2.4) - 0.055) * 255).astype(np.uint8)


def srgb_to_linear(rgb8):
    """sRGB bytes -> linear 0-1 values."""
    v = np.asarray(rgb8, np.float64) / 255.0
    return np.where(v <= 0.04045, v / 12.92, ((v + 0.055) / 1.055) ** 2.4)


# The distance (Unreal units) at which a fog layer that starts some way out
# (StartDistance) is matched by the demo's fog, which starts at the camera.
FOG_MATCH = 6000.0


def fog_layers_combined(layers, floor=None):
    """A level's height-fog layers (lighting.fog_and_post's "HeightFogLayers", of all
    its packages) as the one fog the demo draws over the play area: their densities
    added and their colours (x their brightness, as light) mixed by density, which is
    how fogs of different colours mix in the air. Without `floor`, the play area lies
    below all of them, and those that only start some way out (StartDistance) are
    left out. With it (the play area's height, Unreal units), a layer whose top is
    below it is left out too, as views across the play area don't reach it, and one
    that starts some way out counts for as much as it fogs at FOG_MATCH (left out if
    it starts further out than that). None if no layer is left."""
    if floor is None:
        near = [f for f in layers if f["start"] <= 0.0] or layers

        def share(f):
            return 1.0
    else:
        def share(f):
            return max(0.0, 1.0 - f["start"] / FOG_MATCH)
        near = [f for f in layers if share(f) > 0 and f["height"] > floor]
    if not near:
        return None
    density = sum(f["density"] * share(f) for f in near)
    light = sum(f["density"] * share(f) * f["light_brightness"] * srgb_to_linear(f["color"]) for f in near) / max(density, 1e-12)
    brightness = float(max(light.max(), 1e-6))
    return {"color": tuple(int(c) for c in srgb8(light / brightness)), "light_brightness": brightness,
            "density": density, "start": 0.0, "height": min(f["height"] for f in near)}


_zero_uv = {}


def zero_uv(w, n):
    """A shared all-zero second UV set of n vertices."""
    if n not in _zero_uv:
        _zero_uv[n] = w.accessor(np.zeros((n, 2), np.float32), 34962)
    return _zero_uv[n]


def level_model(pkg):
    """The name of the level's own BSP Model (TheWorld.PersistentLevel's; the other
    Models are volumes' and brushes')."""
    best = None
    for i, e in enumerate(pkg.exports):
        if pkg.class_name(e) == "Model" and pkg.obj_name(e["outer"]) == "PersistentLevel":
            if best is None or e["size"] > best["size"]:
                best = e
    return best["name"] if best else None


class Lighting:
    """The level's baked lighting (see lighting.py), or nothing without the _LM file."""

    def __init__(self, level_path, model, lightmap_dir, prefix="", extra=(), directions=False):
        """`model`: the level's BSP Model (None if it has no BSP, "auto" for the
        level's own); `lightmap_dir`: where the pages go under the assets folder;
        `prefix`: the start of their file names (when two packages' pages share
        the folder); `extra`: other light maps of the level to decode with the
        meshes' and the BSP's (its terrain's, ue3/terrain.py); `directions`: also
        write where the light comes from, a page of direction weights beside each
        page (<page>_dir.png: lighting.direction_weights, as bytes)."""
        lm_path = level_path[:-len(".xxx")] + "_LM.xxx"
        self.ok = os.path.exists(lm_path)
        self.pkg = None
        self.dir = lightmap_dir
        self.prefix = prefix
        self.meshes, self.nodes, self.files = {}, {}, {}
        self.pages = {}
        self.vertex_peaks = []
        self.vertex_dirs = []
        self.directions = directions
        self.dirs = {}
        if not self.ok:
            print(f"  ! {os.path.basename(lm_path)} not found: building without the baked lighting")
            return
        from ue3pkg import Package
        self.pkg = pkg = Package(level_path)
        self.meshes = lighting.static_mesh_lightmaps(pkg)
        if model == "auto":
            model = level_model(pkg)
        if model:
            index = next(i + 1 for i, e in enumerate(pkg.exports) if e["name"] == model)
            self.nodes = lighting.model_lightmaps(pkg, index)
        # (The stored pages too, {LightMapTexture2D export: (name, RGB bytes)}: for a
        # surface shaded from its light map's own colours, build_map.py's Cybertron.)
        self.pages = pages = lighting.lightmap_pages(pkg, lm_path)
        self.baked = lighting.bake_pages(pages, list(self.meshes.values()) + list(self.nodes.values()) + list(extra))
        if directions:
            self.dirs = lighting.bake_direction_pages(
                pages, list(self.meshes.values()) + list(self.nodes.values()) + list(extra))
        print(f"  light maps: {len(self.baked)} pages, {len(self.meshes)} mesh + {len(self.nodes)} BSP node"
              + (f" + {len(extra)} terrain" if extra else "") + " light maps")

    def write_pages(self):
        """Writes the pages that were used; returns their count."""
        d = os.path.join(OUT, self.dir)
        os.makedirs(d, exist_ok=True)
        for f in os.listdir(d):
            if f.endswith(".png") and f.startswith(self.prefix) and (self.prefix or f[0].isdigit() or f == "vertex.png"):
                os.remove(os.path.join(d, f))
        for page, name in self.files.items():
            if page == "vertex":
                v = np.zeros((VERTEX_PAGE, VERTEX_PAGE, 3), np.float32)
                for i, peak in enumerate(self.vertex_peaks):
                    v[i // VERTEX_PAGE, i % VERTEX_PAGE] = peak
            else:
                v = self.baked[page]
            Image.fromarray(srgb8(v / LIGHTMAP_RANGE)).save(os.path.join(OUT, name), optimize=True)
            if self.directions:
                if page == "vertex":
                    dirs = np.full((VERTEX_PAGE, VERTEX_PAGE, 3), 1.0 / 3.0, np.float32)
                    for i, dw in enumerate(self.vertex_dirs):
                        dirs[i // VERTEX_PAGE, i % VERTEX_PAGE] = dw
                else:
                    dirs = self.dirs[page]
                Image.fromarray(np.round(np.clip(dirs, 0, 1) * 255).astype(np.uint8)).save(
                    os.path.join(OUT, name[:-len(".png")] + "_dir.png"), optimize=True)
        return len(self.files)

    def page_file(self, page):
        if page not in self.files:
            n = sum(1 for k in self.files if k != "vertex")
            self.files[page] = f"{self.dir}/{self.prefix}{'vertex' if page == 'vertex' else n}.png"
        return self.files[page]

    def vertex_texel(self, peak, dirs=None):
        """A texel of the vertex-light page holding `peak` (RGB light) and, in its
        direction page, `dirs` (the light's direction weights); returns its centre as
        a UV."""
        i = len(self.vertex_peaks)
        assert i < VERTEX_PAGE * VERTEX_PAGE, "vertex-light page full"
        self.vertex_peaks.append(np.asarray(peak, np.float32))
        self.vertex_dirs.append(np.asarray(dirs if dirs is not None else (1 / 3, 1 / 3, 1 / 3), np.float32))
        return ((i % VERTEX_PAGE + 0.5) / VERTEX_PAGE, (i // VERTEX_PAGE + 0.5) / VERTEX_PAGE)

    def extras(self, page, rect=(0.0, 0.0, 1.0, 1.0)):
        return {"lightmap": self.page_file(page) + " " + " ".join(f"{v:.6f}" for v in rect)}


def bevy(v):
    """Unreal position (cm) -> Bevy (m)."""
    return (v[0] * 0.01, v[2] * 0.01, v[1] * 0.01)


# The cube map the characters reflect where a level has no captured one of its own:
# their materials' ReflectionCubeMapSample's DefaultReflectionCubeMap.
DEFAULT_REFLECTION = ("all_shared.xxx", "ENV_AllShader_p.WFC2.CubeMaps.rommCubeMap")
REFLECT_HEADER = "# reflect <cube map .ktx2> <mip count>   (what the characters reflect: paint.wgsl)"


def write_reflection(pkgs, name, cooked):
    """The cube map the characters reflect in level `name` (src/character.rs,
    assets/shaders/paint.wgsl): their materials sample the cube map of the capture
    nearest them (High Moon's ReflectionCubeMapSample: the level's
    StaticSceneCaptureCubeMapActor's), or their DefaultReflectionCubeMap, rommCubeMap,
    where the level has none (from `cooked`'s all_shared.xxx). Written as
    textures/levelfx/<name>_cube.ktx2 (the first of `pkgs` with a capture: its
    faces' DXT5 data as BC3, sRGB) or textures/levelfx/romm_cube.ktx2 (rommCubeMap's,
    linear: its SRGB is off), with their mips. Returns the lights file's "reflect"
    line, or None."""
    import ktx2_io
    import texture
    from ue3pkg import Package
    from levelfx import actor_props
    found = None
    for pkg in pkgs:
        for e in pkg.exports:
            if pkg.class_name(e) == "StaticSceneCaptureCubeMapActor":
                ref = actor_props(pkg, e).get("CubeMap")
                if ref and ref[1]:
                    found = (pkg, ref[1], f"{name}_cube", True)
                    break
        if found:
            break
    if not found:
        path = os.path.join(cooked or "", DEFAULT_REFLECTION[0])
        if not (cooked and os.path.exists(path)):
            print(f"  ! no reflection cube map: {DEFAULT_REFLECTION[0]} missing")
            return None
        pkg = Package(path)
        idx = next((i for i in range(1, len(pkg.exports) + 1) if pkg.full_name(i) == DEFAULT_REFLECTION[1]), None)
        if idx is None:
            print(f"  ! no reflection cube map: {DEFAULT_REFLECTION[1]} missing")
            return None
        found = (pkg, idx, "romm_cube", False)
    pkg, idx, stem, srgb = found
    fmt, edge, faces = texture.read_cube(pkg, idx)
    if fmt != "PF_DXT5":
        print(f"  ! reflection cube map: format {fmt}")
        return None
    n = min(len(f) for f in faces)
    levels = [b"".join(f[m] for f in faces) for m in range(n)]
    rel = f"textures/levelfx/{stem}.ktx2"
    os.makedirs(os.path.join(OUT, "textures", "levelfx"), exist_ok=True)
    with open(os.path.join(OUT, rel), "wb") as fh:
        fh.write(ktx2_io.write("bc3", srgb, edge, edge, levels, faces=6))
    return f"reflect {rel} {n}"


def fog_light(f):
    """The light a height-fog layer `f` (lighting.fog_and_post's) lays over what it
    hides completely, as the game has it: UE3's FogInScattering is its LightColor x
    LightBrightness / ln(0.5), times (Scattering - 1) where the fog thins the light
    to Scattering, so 1 / ln 2 of it at full cover (FSceneRenderer::InitFogConstants
    and the shaders' height fog)."""
    return f["light_brightness"] / np.log(2.0)


def fog_density(f, scale=1.0):
    """How fast a height-fog layer `f` thins the light (per metre), as the game has it:
    1 - `scale` x Density each Unreal unit (`scale`: the level's
    FOG_DensityWorldScale, lighting.world_fog_scale)."""
    return float(-np.log(1.0 - min(scale * f["density"], 0.999999)) * 100.0)


# FPostProcessSettings' depth-of-field defaults (Engine.xxx's struct defaults), for
# what a volume leaves as they are.
DOF_DEFAULTS = {"DOF_FocusDistance": 0.0, "DOF_FocusNearFalloff": 2000.0, "DOF_FocusFarFalloff": 2000.0,
                "DOF_FalloffExponent": 4.0, "DOF_MaxNearBlurAmount": 1.0, "DOF_MaxFarBlurAmount": 1.0,
                "DOF_BlurKernelSize": 16.0}


def dof_line(pp):
    """The "dof" line for a level's post-process settings `pp` (lighting.fog_and_post's),
    if its volume turns depth of field on (bEnableDOF): focus distance, near and far
    falloffs (m), falloff exponent, most near and far blur, blur kernel (pixels at
    1280 wide) - src/dof.rs draws it as the game does. (Inside its volume the
    volume's settings are the game's, its WorldInfo's defaults left aside: Ancients'
    BASE WorldInfo turns depth of field on, its volume doesn't.) Only FOCUS_Distance
    is drawn (the Escalation maps' own)."""
    if not pp.get("bEnableDOF"):
        return None
    if pp.get("DOF_FocusType", "FOCUS_Distance") != "FOCUS_Distance":
        print("  depth of field focus type", pp["DOF_FocusType"], "drawn as FOCUS_Distance")
    g = {k: float(pp.get(k, v)) for k, v in DOF_DEFAULTS.items()}
    return "dof %.2f %.2f %.2f  %.3f %.3f %.3f  %.1f" % (
        g["DOF_FocusDistance"] * 0.01, g["DOF_FocusNearFalloff"] * 0.01, g["DOF_FocusFarFalloff"] * 0.01,
        g["DOF_FalloffExponent"], g["DOF_MaxNearBlurAmount"], g["DOF_MaxFarBlurAmount"], g["DOF_BlurKernelSize"])


def write_lights(pkg, name, title, others=(), floor=None, height_fogs=False, cooked=None):
    """levels/<name>_lights.txt: the sun, every light, the fog and the post-process
    values, in Bevy space, and the cube map the characters reflect (write_reflection;
    `cooked`: the CookedPC folder, else configure()'s data folder).
    Colours are sRGB bytes; brightness, radius (m), falloff exponent and cone angles
    (degrees) are the game's values. See lighting.rs (MapLighting) for how they are
    used. The fog is the level's height fog: its own and its `others` packages'
    layers, as they reach a play area at height `floor` (fog_layers_combined); with
    `height_fogs`, each layer too, as the game has it ("heightfog": the Customize
    garage draws them, customize.rs's GarageFog). Each as the game lays it
    (fog_light, fog_density): its light at full cover 1 / ln 2 of its LightColor x
    LightBrightness, its density scaled by the level's FOG_DensityWorldScale. The
    volume's depth of field, if it has it (dof_line)."""
    scale = lighting.world_fog_scale([pkg, *others])
    lines = ["# " + title,
             "# sun   dx dy dz  r g b  brightness             (direction the light travels)",
             "# shafts dx dy dz  r g b  bloom_scale threshold screen_blend occlusion_range_m darkness",
             "#       (the light that draws the level's light shafts: its direction, BloomTint, BloomScale,",
             "#       BloomThreshold, BloomScreenBlendThreshold, OcclusionDepthRange, OcclusionMaskDarkness:",
             "#       src/lightshafts.rs)",
             "# point x y z  r g b  brightness radius falloff [dynamic]",
             "# spot  x y z  dx dy dz  r g b  brightness radius falloff inner_deg outer_deg [dynamic]",
             "#       (dynamic: it lights only what moves, the characters, not the level)",
             "# fog   r g b  brightness density_per_m start_m height_m",
             "# heightfog r g b  brightness density_per_m start_m height_m   (a layer: fog below height_m)",
             "#       (brightness: the light it lays over what it hides, the game's LightBrightness / ln 2;",
             "#       density: as the game thins light, x the level's FOG_DensityWorldScale, %g)" % scale,
             "# bloom scale",
             "# dof   focus_m near_falloff_m far_falloff_m  exponent max_near max_far  kernel_px",
             "#       (depth of field, where the post-process volume turns it on: src/dof.rs)",
             "# grade <file of 32x32x32 RGB bytes, red fastest>",
             REFLECT_HEADER]
    sun = lighting.directional_light(pkg)
    if sun:
        d = sun["direction"]
        lines.append("sun %.4f %.4f %.4f  %d %d %d  %.3f" % (d[0], d[2], d[1], *sun["color"], sun["brightness"]))
    shafts = lighting.shaft_light(pkg)
    if shafts:
        d = shafts["direction"]
        lines.append("shafts %.4f %.4f %.4f  %d %d %d  %.3f %.3f %.3f %.1f %.3f" % (
            d[0], d[2], d[1], *shafts["tint"], shafts["BloomScale"], shafts["BloomThreshold"],
            shafts["BloomScreenBlendThreshold"], shafts["OcclusionDepthRange"] * 0.01, shafts["OcclusionMaskDarkness"]))
    n = 0
    for l in lighting.lights(pkg):
        if not l["enabled"] or "position" not in l or l["brightness"] <= 0:
            continue
        x, y, z = bevy(l["position"])
        r = l["radius"] * 0.01
        flag = "  dynamic" if l.get("dynamic_only") else ""
        if l["class"].startswith("Spot"):
            d = l["direction"]
            lines.append("spot %.2f %.2f %.2f  %.4f %.4f %.4f  %d %d %d  %.3f %.2f %.2f %.1f %.1f%s" % (
                x, y, z, d[0], d[2], d[1], *l["color"], l["brightness"], r, l["falloff"], l["inner"], l["outer"], flag))
        else:
            lines.append("point %.2f %.2f %.2f  %d %d %d  %.3f %.2f %.2f%s" % (x, y, z, *l["color"], l["brightness"], r,
                                                                         l["falloff"], flag))
        n += 1
    fp = lighting.fog_and_post(pkg)
    layers = list(fp.get("HeightFogLayers", []))
    for other in others:
        layers += lighting.fog_and_post(other).get("HeightFogLayers", [])
    if len(layers) > 1 or (layers and floor is not None):
        fp["HeightFogComponent"] = fog_layers_combined(layers, floor)
    for k in ("PostProcessVolumeHeightFogComponent", "HeightFogComponent"):
        if fp.get(k):
            f = fp[k]
            lines.append("fog %d %d %d  %.4f %.5f %.1f %.1f" % (*f["color"], fog_light(f), fog_density(f, scale),
                                                               f["start"] * 0.01, f["height"] * 0.01))
    if height_fogs:
        for f in layers:
            lines.append("heightfog %d %d %d  %.4f %.5f %.2f %.3f" % (
                *f["color"], fog_light(f), fog_density(f, scale), f["start"] * 0.01, f["height"] * 0.01))
    pp = fp.get("PostProcessSettings", {})
    lines.append("bloom %.3f" % pp.get("Bloom_Scale", 1.0))
    dof = dof_line(pp)
    if dof:
        lines.append(dof)
    clut = pp.get("ColorCorrectionTexture")
    if clut:
        table, _ = lighting.texture3d(pkg, clut)
        if table is not None and table.shape == (32, 32, 32, 3):
            open(os.path.join(OUT, "levels", f"{name}_grade.bin"), "wb").write(table.tobytes())
            lines.append(f"grade levels/{name}_grade.bin")
    reflect = write_reflection([pkg, *others], name, cooked or DATA)
    if reflect:
        lines.append(reflect)
    with open(os.path.join(OUT, "levels", f"{name}_lights.txt"), "w") as fh:
        fh.write("\n".join(lines) + "\n")
    print(f"wrote levels/{name}_lights.txt ({n} lights)" + (f" and levels/{name}_grade.bin" if clut else ""))

