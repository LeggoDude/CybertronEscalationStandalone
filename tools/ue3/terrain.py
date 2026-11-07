"""A FoC level's terrain (UE3's ATerrain), read straight from its package.

Some maps (Ancients) lay their ground out as a terrain: a grid of heights, painted
with layers of materials. umodel doesn't export terrains, so this reads one:

  * the grid: NumVerticesX x NumVerticesY 16-bit heights (32768 is the actor's own
    height), DrawScale3D (Unreal units) apart and up per 128 steps of height: the
    class's default of 256 x 256 x 256 when the map doesn't set it;
  * its layers (TerrainLayerSetup -> TerrainMaterial -> Material, with the material's
    mapping: LocalToMapping takes a vertex's place in the grid, in quads, to its
    texture coordinates), and the weight maps the game draws them with
    (TerrainWeightMapTexture: a weight for each material in the R, G, B and A of a
    texel per vertex);
  * its components (TerrainComponent: the parts it is drawn and lit in) with their
    light maps: each component's light map covers its quads at four texels a quad,
    one texel on each vertex and three between, with a margin of four texels around
    (so it is 4 x quads + 9 texels across).

The game draws a terrain tessellated: each quad is cut into up to 4 x 4 smaller
ones, their heights from a Catmull-Rom spline through the grid's heights around them
(FPatchSampler), so the ground is smooth; and it collides with the surface cut
CollisionTesselationLevel times (4 on Ancients': as smooth as it is drawn; its
components also keep the grid's own vertices, CollisionVertices).

    read_terrains(pkg) -> [Terrain]
    Terrain.surface(tess) -> grid positions, normals (Unreal units, world space)
"""
import struct

import numpy as np

from layout import props_any, read_struct_array
from props import read_props
from ue3pkg import Reader
import lighting


DEFAULT_SCALE = (256.0, 256.0, 256.0)
# A height step is DrawScale3D.Z / 128 Unreal units (TERRAIN_ZSCALE).
Z_STEPS = 128.0
# Light-map texels a quad, and the margin around a component's light map.
LIGHTMAP_TEXELS = 4
LIGHTMAP_MARGIN = 4


def _props(pkg, e, starts=(30, 4, 8, 0, 16, 12)):
    """An object's tagged properties: the first start that parses cleanly with the
    most properties."""
    best = None
    for st in starts + tuple(range(0, 64)):
        try:
            r = Reader(pkg.data, e["offset"] + st)
            pr = read_props(pkg, r, e["offset"] + e["size"])
        except Exception:
            continue
        if pr and all(k.replace("_", "").replace("[", "").replace("]", "").isalnum() for k in pr):
            if best is None or len(pr) > len(best[0]):
                best = (pr, r.o)
    if best is None:
        raise ValueError(f"cannot read {e['name']}")
    return best


def _vec(v):
    return struct.unpack("<3f", v[2][:12])


def _matrix(v):
    """A Matrix struct (4 x 4 floats, Unreal's row-major, row vectors)."""
    return np.array(struct.unpack("<16f", v[2][:64]), np.float64).reshape(4, 4)


class Layer:
    """A painted layer: its name, which alpha map paints it (-1: the base layer,
    everywhere), and its materials: [(material path, LocalToMapping 4x4)]."""

    def __init__(self, name, alpha_index, materials):
        self.name = name
        self.alpha_index = alpha_index
        self.materials = materials


class Component:
    def __init__(self, base, size, lightmap):
        self.base = base  # (x, y) of its first quad
        self.size = size  # its quads (x, y)
        self.lightmap = lightmap  # lighting.read_lightmap's dict, or None

    def lightmap_uv(self, qx, qy):
        """The light-map UV (in its page) of grid point (qx, qy) (quads, may be
        fractional): texel 4 + 4 q of the component's rectangle, at its centre."""
        lm = self.lightmap
        (sx, sy), (bx, by) = lm["coord_scale"], lm["coord_bias"]
        nx = self.size[0] * LIGHTMAP_TEXELS + 2 * LIGHTMAP_MARGIN + 1
        ny = self.size[1] * LIGHTMAP_TEXELS + 2 * LIGHTMAP_MARGIN + 1
        tx = LIGHTMAP_MARGIN + (np.asarray(qx) - self.base[0]) * LIGHTMAP_TEXELS + 0.5
        ty = LIGHTMAP_MARGIN + (np.asarray(qy) - self.base[1]) * LIGHTMAP_TEXELS + 0.5
        return bx + tx / nx * sx, by + ty / ny * sy


class Terrain:
    def __init__(self):
        self.name = None
        self.location = (0.0, 0.0, 0.0)
        self.scale = DEFAULT_SCALE
        self.nx = self.ny = 0
        self.heights = None  # (ny, nx) uint16
        self.info = None  # (ny, nx) uint8: bit 0 = hole
        self.alphas = []
        self.layers = []
        self.weights = None  # (ny, nx, n materials) float, summing to 1
        self.materials = []  # [(material path, LocalToMapping)] in weight order
        self.components = []
        self.collision_level = 1
        self.max_tess = 4

    # -- the grid --------------------------------------------------------------
    def z(self, h):
        return self.location[2] + (np.asarray(h, np.float64) - 32768.0) * self.scale[2] / Z_STEPS

    def grid(self, qx, qy, z):
        """World positions (Unreal units) of grid places (quads) and heights."""
        x = self.location[0] + np.asarray(qx, np.float64) * self.scale[0]
        y = self.location[1] + np.asarray(qy, np.float64) * self.scale[1]
        return np.stack(np.broadcast_arrays(x, y, z), -1)

    def raw_heights(self):
        """(ny, nx) world heights of the grid's vertices (Unreal units)."""
        return self.z(self.heights)

    def surface(self, tess, slopes=False):
        """The terrain cut `tess` times finer, as the game tessellates it: heights
        from the Catmull-Rom spline through each 4 x 4 block of grid heights
        (FPatchSampler). Returns (qx, qy, z): the places in quads ((nx-1) tess + 1 by
        (ny-1) tess + 1) and their world heights; with `slopes`, also the heights'
        rates of change along x and y (Unreal units a quad), from the spline's own
        derivative, as the game takes its normals."""
        H = self.raw_heights()
        ny, nx = H.shape
        t = np.arange(tess + 1, dtype=np.float64) / tess
        # Catmull-Rom basis for the points -1, 0, 1, 2 around each quad, and its slope.
        basis = np.stack([-0.5 * (t ** 3 - 2 * t ** 2 + t),
                          0.5 * (3 * t ** 3 - 5 * t ** 2 + 2),
                          0.5 * (-3 * t ** 3 + 4 * t ** 2 + t),
                          0.5 * (t ** 3 - t ** 2)], -1)  # (tess + 1, 4)
        slope = np.stack([-0.5 * (3 * t ** 2 - 4 * t + 1),
                          0.5 * (9 * t ** 2 - 10 * t),
                          0.5 * (-9 * t ** 2 + 8 * t + 1),
                          0.5 * (3 * t ** 2 - 2 * t)], -1)
        fx = (nx - 1) * tess + 1
        fy = (ny - 1) * tess + 1
        # (The grid is padded by repeating its edges.)
        cols = np.clip(np.arange(-1, nx + 2), 0, nx - 1)
        rows = np.clip(np.arange(-1, ny + 2), 0, ny - 1)

        def along_x(b):
            """For every grid row, the fine samples between its vertices."""
            Hx = H[:, cols]
            out = np.zeros((ny, fx))
            for i in range(nx - 1):
                out[:, i * tess:(i + 1) * tess + 1] = Hx[:, i:i + 4] @ b.T  # points i-1 .. i+2
            return out

        def along_y(R, b):
            Ry = R[rows, :]
            out = np.zeros((fy, fx))
            for j in range(ny - 1):
                out[j * tess:(j + 1) * tess + 1, :] = b @ Ry[j:j + 4, :]
            return out

        rows_fine = along_x(basis)
        out = along_y(rows_fine, basis)
        qx = np.arange(fx) / tess
        qy = np.arange(fy) / tess
        if not slopes:
            return qx, qy, out
        return qx, qy, out, along_y(along_x(slope), basis), along_y(rows_fine, slope)

    def weights_at(self, qx, qy):
        """The materials' weights at grid places (quads), bilinear between the
        vertices' (as the game samples its weight maps)."""
        W = self.weights
        ny, nx = W.shape[:2]
        x = np.clip(np.asarray(qx, np.float64), 0, nx - 1)
        y = np.clip(np.asarray(qy, np.float64), 0, ny - 1)
        x0 = np.minimum(np.floor(x).astype(int), nx - 2)
        y0 = np.minimum(np.floor(y).astype(int), ny - 2)
        fx, fy = (x - x0)[..., None], (y - y0)[..., None]
        w = (W[y0, x0] * (1 - fx) * (1 - fy) + W[y0, x0 + 1] * fx * (1 - fy)
             + W[y0 + 1, x0] * (1 - fx) * fy + W[y0 + 1, x0 + 1] * fx * fy)
        return w / np.maximum(w.sum(-1, keepdims=True), 1e-9)

    def hole(self, qx, qy):
        """True where a quad is cut out (InfoData bit 0)."""
        if self.info is None:
            return np.zeros(np.shape(qx), bool)
        ny, nx = self.info.shape
        x = np.clip(np.floor(qx).astype(int), 0, nx - 1)
        y = np.clip(np.floor(qy).astype(int), 0, ny - 1)
        return (self.info[y, x] & 1) != 0


def _setup_props(pkg, e):
    """A TerrainLayerSetup's properties (its Materials array of filtered materials;
    a parse from inside that array would look cleaner, so the start is fixed)."""
    for st in (4, 0, 8, 12, 16):
        try:
            pr = read_props(pkg, Reader(pkg.data, e["offset"] + st), e["offset"] + e["size"])
        except Exception:
            continue
        if "Materials" in pr:
            return pr
    return {}


def _weight_map(pkg, idx):
    """A TerrainWeightMapTexture's texels: (SizeY, SizeX, 4) uint8 as R, G, B, A."""
    e = pkg.exports[idx - 1]
    pr, r = props_any(pkg, e, (4, 0, 8))
    sx, sy = pr["SizeX"], pr["SizeY"]
    assert pr.get("Format", "PF_A8R8G8B8") == "PF_A8R8G8B8", pr.get("Format")
    blob = pkg.data[r.o:e["offset"] + e["size"]]
    n = sx * sy * 4
    for k in range(0, len(blob) - 16):
        flags, count, size = struct.unpack_from("<iii", blob, k)
        if count == n and size == n and flags & 1 == 0:
            bgra = np.frombuffer(blob[k + 16:k + 16 + n], np.uint8).reshape(sy, sx, 4)
            return bgra[..., [2, 1, 0, 3]]
    raise ValueError("no texels in " + e["name"])


def _component_lightmap(pkg, e):
    """A TerrainComponent's light map (FLightMap2D near the end of its data): found
    by its header, a type of 2 and its lights' GUIDs, just before the references to
    the light-map pages."""
    lm_pages = {i for i, x in enumerate(pkg.exports, 1) if pkg.class_name(x) == "LightMapTexture2D"}
    d = pkg.data[e["offset"]:e["offset"] + e["size"]]
    for k in range(len(d) - 124, 0, -1):
        v = struct.unpack_from("<i", d, k)[0]
        if v not in lm_pages or struct.unpack_from("<i", d, k + 36)[0] not in lm_pages:
            continue
        for back in range(k - 12, max(k - 4000, -1), -1):
            if struct.unpack_from("<i", d, back)[0] != 2:
                continue
            n = struct.unpack_from("<i", d, back + 4)[0]
            if 0 <= n < 1000 and back + 8 + 16 * n + 4 == k:
                return lighting.read_lightmap(Reader(pkg.data, e["offset"] + back))
        return None
    return None


def read_terrains(pkg):
    out = []
    for i, e in enumerate(pkg.exports, 1):
        if pkg.class_name(e) != "Terrain":
            continue
        pr, end = _props(pkg, e)
        t = Terrain()
        t.name = e["name"]
        if "Location" in pr:
            t.location = _vec(pr["Location"])
        if "DrawScale3D" in pr:
            t.scale = _vec(pr["DrawScale3D"])
        ds = pr.get("DrawScale", 1.0)
        t.scale = tuple(s * ds for s in t.scale)
        t.nx, t.ny = pr["NumVerticesX"], pr["NumVerticesY"]
        t.collision_level = pr.get("CollisionTesselationLevel", 1)
        r = Reader(pkg.data, end)
        n = r.i32()
        assert n == t.nx * t.ny, (n, t.nx, t.ny)
        t.heights = np.frombuffer(r.bytes(2 * n), "<u2").reshape(t.ny, t.nx)
        n = r.i32()
        t.info = np.frombuffer(r.bytes(n), np.uint8).reshape(t.ny, t.nx) if n == t.nx * t.ny else None
        t.alphas = []
        for _ in range(r.i32()):
            m = r.i32()
            t.alphas.append(np.frombuffer(r.bytes(m), np.uint8).reshape(t.ny, t.nx) if m == t.nx * t.ny else None)
        weight_maps = [r.i32() for _ in range(r.i32())]

        # Layers, their setups and materials (in order: the weight maps' order).
        t.layers, t.materials = [], []
        for lp in read_struct_array(pkg, pr["Layers"][1]):
            setup = lp.get("Setup", ("obj", 0))[1]
            mats = []
            if setup > 0:
                spr = _setup_props(pkg, pkg.exports[setup - 1])
                for fm in read_struct_array(pkg, spr["Materials"][1]) if "Materials" in spr else []:
                    tm = fm.get("Material", ("obj", 0))[1]
                    if tm <= 0:
                        continue
                    tpr, _ = _props(pkg, pkg.exports[tm - 1], (4, 0, 8))
                    mat = tpr.get("Material", ("obj", 0))[1]
                    mapping = _matrix(tpr["LocalToMapping"]) if "LocalToMapping" in tpr else np.eye(4) / tpr.get("MappingScale", 4.0)
                    mats.append((pkg.full_name(mat) if mat else None, mapping))
            name = lp["Name"][1][4:].rstrip(b"\0").decode("latin-1") if "Name" in lp else ""
            t.layers.append(Layer(name, lp.get("AlphaMapIndex", -1), mats))
            t.materials += mats

        # The game's own weights for its materials (material k in channel k of map k // 4).
        if weight_maps and weight_maps[0] > 0:
            chans = np.concatenate([_weight_map(pkg, w) for w in weight_maps], -1).astype(np.float64)
            w = chans[..., :len(t.materials)]
            if w.shape[:2] != (t.ny, t.nx):
                raise ValueError(f"weight map {w.shape[:2]} vs grid {(t.ny, t.nx)}")
            t.weights = w / np.maximum(w.sum(-1, keepdims=True), 1e-9)
        else:
            t.weights = _weights_from_alphas(t)

        # Components (the true sizes are in quads; SectionSize counts groups of
        # MaxTesselationLevel quads).
        t.components = []
        for ref in struct.unpack_from(f"<{struct.unpack_from('<i', pr['TerrainComponents'][1])[0]}i",
                                      pr["TerrainComponents"][1], 4):
            if ref <= 0:
                continue
            ce = pkg.exports[ref - 1]
            cpr, _ = props_any(pkg, ce, (8, 16, 4, 12))
            base = (cpr.get("SectionBaseX", 0), cpr.get("SectionBaseY", 0))
            size = (cpr.get("TrueSectionSizeX", 0), cpr.get("TrueSectionSizeY", 0))
            if cpr.get("SectionSizeX"):
                t.max_tess = max(1, size[0] // cpr["SectionSizeX"])
            t.components.append(Component(base, size, _component_lightmap(pkg, ce)))
        out.append(t)
    return out


def _weights_from_alphas(t):
    """Without weight maps: each layer over the ones below by its alpha map."""
    n = len(t.layers)
    w = np.zeros((t.ny, t.nx, n))
    left = np.ones((t.ny, t.nx))
    for k in range(n - 1, -1, -1):
        L = t.layers[k]
        a = np.ones((t.ny, t.nx)) if L.alpha_index < 0 else t.alphas[L.alpha_index] / 255.0
        w[..., k] = left * a
        left = left * (1 - a)
    return w / np.maximum(w.sum(-1, keepdims=True), 1e-9)
