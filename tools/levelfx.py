"""The maps' effects, for src/levelfx.rs: the game's effect meshes (light shafts,
glow spheres, smoke cards and other soft see-through shapes), its particle emitters
and its lens flares, each drawn with its own game material.

build_map.py makes a LevelFx for the map it builds; then

    index = fx.material(name)    # an effect mesh's material, compiled (None if it can't be)
    fx.scan(pkg)                 # the level's particle emitters and lens flares
    fx.write(assets_dir)         # what levelfx.rs reads (below)

writes:

  * levels/<map>_fx.txt - the materials (their shader def, blend, lighting,
    textures, and whether they shift the picture behind them: "distort=1", their
    Distortion), the particle systems (their emitters' settings and modules, see
    ue3/particles.py), where the level places them (and whether the game lights
    each: a place line's last field, its component accepting lights with a light
    environment), the lens flares and where they stand. Positions and sizes are
    in Unreal's axes and units, as the game has them (levelfx.rs simulates there
    and converts what it draws).
  * textures/levelfx/ - the materials' textures (KTX2, block-compressed with their
    mips; the small colour ramps as PNG), by their names (two different textures
    of one name: the second also by its path, LevelFx.texture_stems).
  * shaders/levelfx/<map>.wgsl - the map's materials, each the game's material graph
    in WGSL (ue3/fxmat.py) as `fn fx_material`, inside `#ifdef FXM_<MAP>_<n>`; and
    shaders/levelfx.wgsl, the effects' shader: tools/levelfx_shader.wgsl with every
    built map's materials put in.

Which emitters are drawn: those the level starts with running (their component
auto-activates and Kismet doesn't stop them as the game starts); the ones Kismet
starts later - traps, the flamethrowers, Matinee-driven shots - are left out, as are
mesh emitters whose meshes aren't in the umodel export.
"""
import os
import re
import struct
import sys

import numpy as np
from PIL import Image

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.path.join(HERE, "ue3"))
sys.path.insert(0, HERE)
import fxmat  # noqa: E402
import kismet  # noqa: E402
import ktx2_io  # noqa: E402
import particles  # noqa: E402
import shadercache  # noqa: E402
import staticmesh  # noqa: E402
import texture  # noqa: E402
from fxmat import obj_array, struct_array, sub  # noqa: E402

TEMPLATE = os.path.join(HERE, "levelfx_shader.wgsl")
# Materials drawn with parameters of their own, as the game sets them at runtime on
# what spawns them: "<material>@<variant>" -> (the material, its parameters). The
# Diffraction Barrier in each side's colour, both by its vector EnergonColor, which
# TnBarrierSpawnable.Initialize's CharacterApplier.ApplyColorsToMesh sets on its
# mesh to the player's: his side's energon colour (TnFactionTeamDecepticons' and
# TnFactionTeamAutobots' EnergonColor, as src/character.rs's DECEPTICON_ENERGON and
# AUTOBOT_ENERGON have them). Its edge (TR_Barrier_EdgeGlow_MAT) glows in it: its
# LinearInterpolate between the vector EnergonColor (blue, (0.06, 0.06, 2) as
# cooked) and a red constant takes its Alpha from a constant 0, so the glow is the
# vector parameter's colour and the material's scalar EnergonColor goes unused.
# Its sweeping band of hexagons (TR_Barrier_Distortion_AA_MAT) is its scanline times
# the vector (white, (2, 2, 2), as cooked).
DECEPTICON_ENERGON = (0.2, 0.1, 1.5, 1.0)
AUTOBOT_ENERGON = (1.25, 0.05, 0.05, 1.0)
VARIANTS = {
    "TR_Barrier_EdgeGlow_MAT_INST@Decepticon": ("TR_Barrier_EdgeGlow_MAT_INST",
                                                {"vector": {"EnergonColor": DECEPTICON_ENERGON}}),
    "TR_Barrier_EdgeGlow_MAT_INST@Autobot": ("TR_Barrier_EdgeGlow_MAT_INST",
                                             {"vector": {"EnergonColor": AUTOBOT_ENERGON}}),
    "TR_Barrier_Distortion_AA_MATINST@Decepticon": ("TR_Barrier_Distortion_AA_MATINST",
                                                    {"vector": {"EnergonColor": DECEPTICON_ENERGON}}),
    "TR_Barrier_Distortion_AA_MATINST@Autobot": ("TR_Barrier_Distortion_AA_MATINST",
                                                 {"vector": {"EnergonColor": AUTOBOT_ENERGON}}),
}
BLENDS = {"BLEND_Translucent": "translucent", "BLEND_Additive": "additive", "BLEND_Modulate": "modulate",
          "BLEND_Masked": "masked", "BLEND_Opaque": "masked"}
ADDRESS = {"TA_Wrap": "wrap", "TA_Clamp": "clamp", "TA_Mirror": "mirror"}
# Particle system components and what an Emitter or LensFlareSource holds them in.
EMITTER_CLASSES = ("Emitter",)
FLARE_CLASSES = ("LensFlareSource",)


def _num(x):
    return particles._num(float(x))


def _clean(k):
    return all(c.isalnum() or c in "_[]" for c in k)


def actor_props(pkg, e, maxoff=120):
    """An actor's tagged properties (after its state frame: the longest clean parse)."""
    from props import read_props
    from ue3pkg import Reader
    best = {}
    for st in range(0, maxoff):
        try:
            r = Reader(pkg.data, e["offset"] + st)
            pr = read_props(pkg, r, e["offset"] + e["size"])
        except Exception:
            continue
        if pr and all(_clean(k) for k in pr) and len(pr) > len(best):
            best = pr
    return best


def chain_props(pkg, idx, depth=6):
    """An object's properties with its archetypes' (its own win)."""
    out = {}
    while idx and idx > 0 and depth:
        e = pkg.exports[idx - 1]
        for k, v in actor_props(pkg, e, 60).items():
            out.setdefault(k, v)
        idx = e["archetype"]
        depth -= 1
    return out


def _vec3(pr, key, default=(0.0, 0.0, 0.0)):
    return struct.unpack("<3f", pr[key][2][:12]) if key in pr else default


def _rot3(pr, key="Rotation"):
    return struct.unpack("<3i", pr[key][2][:12]) if key in pr else (0, 0, 0)


def rotator_matrix(pitch, yaw, roll):
    """UE3's FRotationMatrix (row vectors: local X, Y, Z in the world)."""
    k = 2 * np.pi / 65536.0
    sp, cp = np.sin(pitch * k), np.cos(pitch * k)
    sy, cy = np.sin(yaw * k), np.cos(yaw * k)
    sr, cr = np.sin(roll * k), np.cos(roll * k)
    return np.array([
        [cp * cy, cp * sy, sp],
        [sr * sp * cy - cr * sy, sr * sp * sy + cr * cy, -sr * cp],
        [-(cr * sp * cy + sr * sy), cy * sr - cr * sp * sy, cr * cp],
    ])


def actor_frame(pr):
    """An actor's location and its scaled axes (rows: local X, Y, Z in the world)."""
    ds = pr.get("DrawScale", 1.0)
    ds3 = _vec3(pr, "DrawScale3D", (1.0, 1.0, 1.0))
    axes = np.diag([ds * ds3[0], ds * ds3[1], ds * ds3[2]]) @ rotator_matrix(*_rot3(pr))
    return np.array(_vec3(pr, "Location")), axes


def level_actors(pkg):
    lvl = {i for i, e in enumerate(pkg.exports, 1) if pkg.class_name(e) == "Level"}
    return [i for i, e in enumerate(pkg.exports, 1) if e["outer"] in lvl]


def _bgra(raw):
    b, g, r, a = raw[:4]
    return r, g, b, a


def kismet_control(pkg):
    """{actor: what Kismet does to it}: "stopped" (turned off or toggled as the game
    starts, or played by a Matinee) or "started" later (turned on by an event)."""
    g = kismet.Graph(pkg)
    at_start = g.reached(kismet.START)
    any_time = g.reached(kismet.ROOTS)
    out = {}
    for oi, op in g.ops.items():
        cls = op["class"]
        for _vname, vars_ in op["vars"].items():
            for v in vars_:
                for obj in g.objects(v):
                    if cls == "SeqAct_Interp":
                        out[obj] = "stopped"
                    elif cls == "SeqAct_Toggle":
                        start_inputs = {k for (i, k) in at_start if i == oi}
                        if start_inputs & {1, 2}:
                            out[obj] = "stopped"
                        elif not start_inputs and {k for (i, k) in any_time if i == oi} & {0}:
                            out.setdefault(obj, "started")
    return out


class LevelFx:
    def __init__(self, stem, cooked, files, mesh_export=None, txt_path=None, lods=False, mesh_built=None):
        """`mesh_export`: a umodel glTF export holding the static meshes that mesh
        emitters draw (<folder>/<package>/StaticMesh3/<mesh>.gltf); without one,
        mesh emitters are left out. `mesh_built`: a models/fx folder of meshes an
        earlier build wrote (<mesh>.fxmesh), kept as they are for those the export
        hasn't got. `txt_path`: where the effects file goes under the
        assets (levels/<stem>_fx.txt by default). `lods`: also write each system's
        less detailed levels that differ from its first (as systems of their own,
        named <system>@<level>, with "lod" lines saying from how far away each is
        used), for effects spawned at a distance from the camera."""
        self.stem = stem
        self.lods = lods
        self.cooked = cooked
        self.mesh_export = mesh_export
        self.mesh_built = mesh_built
        self.txt_path = txt_path or f"levels/{stem}_fx.txt"
        self.meshes = {}        # mesh name -> its glTF (umodel's export)
        # Meshes cut from another: name -> (the mesh exported, static or skeletal, the
        # material of the section kept), used when the export has no <name>.gltf.
        self.mesh_cuts = {}
        self.mesh_section = {}  # mesh name -> the material of the section it is
        self.lib = fxmat.Library(cooked, files)
        self.mats = []          # [(name, Compiled)] by index
        self.mat_index = {}     # material name -> index (None: can't be drawn)
        self.systems = []       # [particles.System]
        self.sys_index = {}
        self.places = []        # [(system index, location, axes, {param: rgba}, lit)]
        self.flares = []        # [(name, [element dicts], screen map Dist)]
        self.flare_index = {}
        self.flare_places = []  # [(flare index, location, axes, colour rgba, size)]
        self.skipped = []
        self.caches = None      # the game's shader caches (ue3/shadercache.py), once needed
        self.cube = None        # (path, mip count) of the level's reflection cube map
        self.nofog = set()      # indices of materials drawn outside the fog (the far backdrop)
        # The decal materials whose decal emitters are left out (their marks never show
        # in the game's videos: build_weapon_fx.py's UNSEEN_DECALS).
        self.unseen_decals = set()
        # The solid (masked) materials lit as the translucent lit ones are, where
        # their effect is lit, unless they are unlit (MLM_Unlit; any other lighting
        # model, MLM_NormalizedBlinnPhong too, is lit): the still meshes of
        # build_weapon_fx.py's lit MESH_SYSTEMS (a destructible's chunks, which the
        # game lights by their light environments). Every other solid material's
        # lit=0: the maps draw theirs as surfaces lit by their light maps.
        self.lit_solid = set()
        # Texture path -> the file name it is written under, for those that share
        # their name with another texture here (texture_stems).
        self.stems = {}

    # -- materials --

    def material(self, name, fog=True):
        """The index of material `name` (compiled on first use), or None. `fog`
        False: drawn outside the fog (the far backdrop: Oblivion's space)."""
        if not name:
            return None
        if name in self.mat_index:
            if not fog and self.mat_index[name] is not None:
                self.nofog.add(self.mat_index[name])
            return self.mat_index[name]
        try:
            # (A variant, "<material>@<variant>": the material with VARIANTS' parameters.)
            base, overrides = VARIANTS.get(name, (name, None))
            c = self.lib.compile(base, "fx_material", overrides)
        except fxmat.CompileError as ex:
            print(f"  ! effect material {name}: {ex}")
            self.mat_index[name] = None
            return None
        if c.blend not in BLENDS:
            print(f"  ! effect material {name}: blend {c.blend}")
            self.mat_index[name] = None
            return None
        if c.blur_buffer:
            print(f"  ! effect material {name}: a blur buffer material (the game's screen blur)")
            self.mat_index[name] = None
            return None
        if c.draws_gbuffer:
            print(f"  ! effect material {name}: draws the particle G-buffers (the game's lit smoke)")
            self.mat_index[name] = None
            return None
        self.mat_index[name] = len(self.mats)
        self.mats.append((name, c))
        if not fog:
            self.nofog.add(self.mat_index[name])
        return self.mat_index[name]

    def surface(self, name):
        """The index of material `name` as a surface of the level (compiled on first
        use), or None: an opaque material only (src/levelfx.rs's SurfaceMaterial
        draws it, lit by the light maps)."""
        if not name or self.mat_index.get(name, 0) is None:
            return None
        try:
            c = self.lib.compile(name, "fx_material") if name not in self.mat_index else self.mats[self.mat_index[name]][1]
        except fxmat.CompileError as ex:
            print(f"  ! surface material {name}: {ex}")
            self.mat_index[name] = None
            return None
        if c.blend != "BLEND_Opaque":
            return None
        if name not in self.mat_index:
            self.mat_index[name] = len(self.mats)
            self.mats.append((name, c))
        return self.mat_index[name]

    def reflection_cube(self, pkg, out):
        """The level's captured cube map (its StaticSceneCaptureCubeMapActor's), which
        its surfaces' ReflectionCubeMapSamples reflect: written as
        textures/levelfx/<stem>_cube.ktx2 (its faces' DXT5 data as BC3, with their
        mips). Returns its path, or None."""
        import texture
        for i, e in enumerate(pkg.exports):
            if pkg.class_name(e) != "StaticSceneCaptureCubeMapActor":
                continue
            ref = actor_props(pkg, e).get("CubeMap")
            if not ref or not ref[1]:
                continue
            fmt, edge, faces = texture.read_cube(pkg, ref[1])
            if fmt != "PF_DXT5":
                print(f"  ! reflection cube map: format {fmt}")
                return None
            n = min(len(f) for f in faces)
            levels = [b"".join(f[m] for f in faces) for m in range(n)]
            path = f"textures/levelfx/{self.stem}_cube.ktx2"
            os.makedirs(os.path.join(out, "textures", "levelfx"), exist_ok=True)
            with open(os.path.join(out, path), "wb") as fh:
                fh.write(ktx2_io.write("bc3", True, edge, edge, levels, faces=6))
            self.cube = (path, n)
            return path
        return None

    def material_ref(self, pkg, ref):
        """The index of the material an object property points at."""
        if not ref:
            return None
        return self.material(pkg.obj_name(ref))

    # -- particle systems --

    def _read_drawn(self, name, found, level, quiet=False, game_shaders=False, shown=()):
        """System `found` at level of detail `level`, with only the emitters that can
        be drawn; None if none can. `game_shaders`: also leave out the emitters the
        game itself can't draw (see _undrawable), but for those named in `shown`,
        which the game's videos show all the same."""
        try:
            s = particles.read_system(self.lib, *found, level)
        except particles.ParticleError as ex:
            print(f"  ! particle system {name}: {ex}")
            return None
        for why in s.left_out:
            if not quiet:
                print(f"  ! {name}: left out {why}")
        drawn = []
        # (The emitters others put their particles where theirs are: LocationEmitter's
        # and LocationEmitterDirect's sources.)
        sources = {m.name for em in s.emitters for m in em.modules if m.kind in (0x13, 0x14) and m.name}
        for em in s.emitters:
            if em.typedata and em.typedata["kind"] == "decal" and em.material_name in self.unseen_decals:
                continue
            why = self._undrawable(em) if game_shaders and em.name not in shown else None
            if why and em.name.replace(" ", "_") in sources:
                # Undrawn, but its particles lead others': run, not drawn ("draw=0",
                # no material of its own). The Snipers' and Sharpshooters' tracer's
                # Bolt, which its lightning and smoke trail follow.
                if not quiet:
                    print(f"  ! {name}: {em.name} not drawn ({why}), but kept: others follow its particles")
                em.req["draw"] = 0
                em.mat = 0
                drawn.append(em)
                continue
            if why:
                if not quiet:
                    print(f"  ! {name}: left out {em.name}, which the game doesn't draw: {why}")
                continue
            em.mat = self.material_ref(em.material[0], em.material[1]) if em.material else None
            if em.mat is None:
                if not quiet:
                    print(f"  ! {name}: emitter {em.name} has no material it can be drawn with")
                continue
            if em.typedata and em.typedata["kind"] == "mesh" and not self._mesh(em):
                continue
            if getattr(em, "unsupported", None) and not quiet:
                print(f"  ! {name}: emitter {em.name} leaves out {', '.join(em.unsupported)}")
            drawn.append(em)
        s.emitters = drawn
        return s if drawn else None

    def _undrawable(self, em):
        """Why the game never draws emitter `em`, or None: its material has no compiled
        shaders for the vertex factory the game draws it with (the game's shader
        caches, ue3/shadercache.py's drawn_with: its kind's, with dynamic parameters if
        the material reads them or the emitter has a DynamicParameter module), as for
        a sub-UV emitter whose material has shaders for plain sprites only, or an
        emitter with a DynamicParameter module whose material doesn't read them."""
        if not em.material_name:
            return None
        if self.caches is None:
            self.caches = shadercache.Caches(self.lib)
        have = self.caches.vertex_factories(em.material_name)
        if have is None:
            return None
        vf = shadercache.drawn_with(em, have)
        if vf is None or vf in have:
            return None
        return f"its material, {em.material_name}, has no shaders for {vf}"

    @staticmethod
    def _lines(s):
        return [line for em in s.emitters for line in particles.emitter_lines(em, em.mat)]

    def mesh_has_colours(self, name):
        """Whether static mesh `name` (looked up in the packages) has vertex colours of
        its own: umodel's export gives one without them colours out of memory it
        never filled in, black or noise (ue3/staticmesh.py), which the game draws
        white. True when it can't tell."""
        if not hasattr(self, "_colours"):
            self._colours = {}
        if name not in self._colours:
            found = self.lib.find(name, ("StaticMesh",))
            has = staticmesh.has_colours(*found) if found else None
            self._colours[name] = has is not False
        return self._colours[name]

    def system(self, pkg, ref, game_shaders=False, rename=None, shown=()):
        """The index of the particle system an object property points at (None if it
        can't be drawn). `game_shaders`: leave out the emitters the game itself
        can't draw (_undrawable), but for those named in `shown`. `rename`: the name
        it is written under (for one of two systems of the same name)."""
        name = pkg.obj_name(ref)
        key = rename or name
        if key in self.sys_index:
            return self.sys_index[key]
        found = self.lib.resolve_ref(pkg, ref, ("ParticleSystem",))
        idx = None
        if found:
            s = self._read_drawn(name, found, 0, game_shaders=game_shaders, shown=shown)
            if s is not None and rename:
                s.name = rename
            if s is not None:
                idx = len(self.systems)
                self.systems.append(s)
                s.lods = []
                if self.lods:
                    # Its other levels of detail, where they differ from the first.
                    first = self._lines(s)
                    for level, dist in enumerate(s.lod_distances[1:], 1):
                        other = self._read_drawn(name, found, level, quiet=True, game_shaders=game_shaders,
                                                 shown=shown)
                        if other is None:
                            continue
                        if self._lines(other) == first:
                            continue
                        other.name = f"{s.name}@{level}"
                        other.lods = []
                        s.lods.append((dist, len(self.systems)))
                        self.systems.append(other)
        else:
            print(f"  ! particle system {name} not found")
        self.sys_index[key] = idx
        return idx

    def system_named(self, name, game_shaders=False, rename=None, shown=()):
        """The index of particle system `name` (looked up in the packages; a dotted
        name is a full one, for a name more than one system has), written under
        `rename` if given (`game_shaders` and `shown`: see system)."""
        if "." in name:
            found = self.lib.find_full(name, ("ParticleSystem",))
        else:
            found = self.lib.find(name, ("ParticleSystem",))
        if not found:
            print(f"  ! particle system {name} not found")
            return None
        return self.system(*found, game_shaders=game_shaders, rename=rename, shown=shown)

    def _mesh(self, em):
        """Points mesh emitter `em` at its mesh's file (models/fx/<mesh>.fxmesh,
        written with the effects); False if the mesh can't be had. A mesh named
        "<mesh>#<k>" is that mesh's section k alone (its k-th material's triangles:
        models/fx/<mesh>#<k>.fxmesh), for a mesh drawn in more than one material."""
        td = em.typedata
        name = td["mesh_name"]
        if name not in self.meshes:
            path = None
            mesh = name.split("#")[0]
            if self.mesh_export:
                for root, _dirs, files in os.walk(self.mesh_export, followlinks=True):
                    if f"{mesh}.gltf" in files and os.path.basename(root) == "StaticMesh3":
                        path = os.path.join(root, f"{mesh}.gltf")
                        break
                if path is None and name in self.mesh_cuts:
                    whole, material = self.mesh_cuts[name]
                    for root, _dirs, files in os.walk(self.mesh_export, followlinks=True):
                        if f"{whole}.gltf" in files and os.path.basename(root) in ("StaticMesh3", "SkeletalMesh3"):
                            path = os.path.join(root, f"{whole}.gltf")
                            self.mesh_section[name] = material
                            break
            self.meshes[name] = path
        if self.meshes[name] is None:
            if self.mesh_built and os.path.exists(os.path.join(self.mesh_built, f"{name}.fxmesh")):
                # (Written by an earlier build, from an export no longer at hand.)
                td["file"] = f"models/fx/{name}.fxmesh"
                return True
            print(f"  ! emitter {em.name}: mesh {name} not in the umodel export; left out")
            return False
        td["file"] = f"models/fx/{name}.fxmesh"
        return True

    def write_meshes(self, out):
        """models/fx/<mesh>.fxmesh for each mesh emitter's mesh: "FXM1", u32 vertex
        count, u32 index count, then per vertex position (3 f32), normal (3),
        tangent (4, w the bitangent's sign) and UV (2) - Unreal's axes and
        centimetres - then the u32 indices (as umodel wrote them: counter-clockwise
        from outside once the points are back in Bevy's axes)."""
        import gltf_io
        folder = os.path.join(out, "models", "fx")
        os.makedirs(folder, exist_ok=True)
        for name, path in self.meshes.items():
            if path is None:
                continue
            d = gltf_io.Doc(path)
            pos_all, nrm_all, tan_all, uv_all, idx_all = [], [], [], [], []
            base = 0
            prims = d.g["meshes"][0]["primitives"]
            if "#" in name:
                prims = [prims[int(name.split("#")[1])]]
            if name in self.mesh_section:
                # (A section of another mesh: its primitives in that material.)
                prims = [p for p in prims if d.g["materials"][p["material"]].get("name") == self.mesh_section[name]]
            for p in prims:
                at = p["attributes"]
                n = int(d.g["accessors"][at["POSITION"]]["count"])
                pos, nrm, tan, uv0, _uv1, _col = fx_mesh_attributes(d, at, n)
                # umodel's glTF has Bevy's axes (Unreal's x, z, y) in metres.
                pos_all.append(pos[:, [0, 2, 1]] * 100.0)
                nrm_all.append(nrm[:, [0, 2, 1]])
                t = tan[:, [0, 2, 1, 3]].copy()
                # (Swapping two axes mirrors the frame: the bitangent's sign turns.)
                t[:, 3] = -t[:, 3]
                tan_all.append(t)
                uv_all.append(uv0)
                ind = d.acc(p["indices"]).astype(np.int64).reshape(-1, 3)
                idx_all.append(ind + base)
                base += n
            pos, nrm, tan, uv = (np.concatenate(a).astype(np.float32) for a in (pos_all, nrm_all, tan_all, uv_all))
            ind = np.concatenate(idx_all).astype(np.uint32).ravel()
            body = np.concatenate([pos, nrm, tan, uv], 1).astype("<f4").tobytes()
            with open(os.path.join(folder, f"{name}.fxmesh"), "wb") as fh:
                fh.write(b"FXM1" + struct.pack("<II", len(pos), len(ind)) + body + ind.astype("<u4").tobytes())
            r = np.linalg.norm(pos, axis=1).max()
            print(f"wrote models/fx/{name}.fxmesh ({len(pos)} vertices, {len(ind) // 3} triangles, "
                  f"{r:.0f} units from its pivot at most)")

    # -- lens flares --

    def flare(self, pkg, ref):
        name = pkg.obj_name(ref)
        if name in self.flare_index:
            return self.flare_index[name]
        found = self.lib.resolve_ref(pkg, ref, ("LensFlare",))
        idx = None
        if found:
            fpkg, fidx = found
            pr = self.lib.props(fpkg, fidx)
            elements = []
            src = sub(fpkg, pr.get("SourceElement")) if "SourceElement" in pr else None
            if src is not None:
                el = self._flare_element(fpkg, src, True)
                if el:
                    elements.append(el)
            for r in struct_array(fpkg, pr.get("Reflections")) if "Reflections" in pr else []:
                el = self._flare_element(fpkg, r, False)
                if el:
                    elements.append(el)
            screen = particles.raw_dist(fpkg, pr.get("ScreenPercentageMap"), False) if "ScreenPercentageMap" in pr \
                else particles.Dist(2, 1, [0.0, 1.0, 0.0, 1.0, 0.0, 1.0])
            if elements:
                idx = len(self.flares)
                self.flares.append((fpkg.exports[fidx - 1]["name"], elements, screen))
        self.flare_index[name] = idx
        return idx

    def _flare_element(self, pkg, s, source):
        if s.get("bIsEnabled", True) is False:
            return None
        mats = obj_array(s.get("LFMaterials")) if "LFMaterials" in s else []
        mi = particles.raw_dist(pkg, s.get("LFMaterialIndex"), False) if "LFMaterialIndex" in s else None
        k = int(mi.table[0]) if mi and mi.table else 0
        if not mats:
            return None
        mat = self.material_ref(pkg, mats[min(max(k, 0), len(mats) - 1)])
        if mat is None:
            return None

        def dist(key, vector, default):
            d = particles.raw_dist(pkg, s.get(key), vector) if key in s else None
            return d or default

        one, ones = particles.const_float(1.0), particles.const_vector((1.0, 1.0, 1.0))
        return {
            "source": source,
            "mat": mat,
            "ray": float(s.get("RayDistance", 0.0)),
            "size": _vec3(s, "Size", (0.5, 0.5, 0.0)) if "Size" in s and isinstance(s["Size"], tuple) else (0.5, 0.5, 0.0),
            "normrad": int(bool(s.get("bNormalizeRadialDistance", False))),
            "usesrc": int(bool(s.get("bUseSourceDistance", False))),
            "dists": [("scaling", dist("Scaling", False, one)),
                      ("axis", dist("AxisScaling", True, ones)),
                      ("rotation", dist("Rotation", False, particles.const_float(0.0))),
                      ("color", dist("Color", True, ones)),
                      ("alpha", dist("Alpha", False, one)),
                      ("offset", dist("Offset", True, particles.const_vector((0.0, 0.0, 0.0)))),
                      ("distscale", dist("DistMap_Scale", True, ones)),
                      ("distcolor", dist("DistMap_Color", True, ones)),
                      ("distalpha", dist("DistMap_Alpha", False, one))],
        }

    # -- the level --

    def scan(self, pkg):
        """The emitters and lens flares level package `pkg` places."""
        control = kismet_control(pkg)
        for i in level_actors(pkg):
            e = pkg.exports[i - 1]
            cls = pkg.class_name(e)
            if cls not in EMITTER_CLASSES + FLARE_CLASSES:
                continue
            pr = actor_props(pkg, e)
            if pr.get("bHidden"):
                continue
            comp_key = "ParticleSystemComponent" if cls in EMITTER_CLASSES else "LensFlareComp"
            comp = pr.get(comp_key)
            if not (isinstance(comp, tuple) and comp[0] == "obj" and comp[1] and comp[1] > 0):
                continue
            cpr = chain_props(pkg, comp[1])
            tmpl = cpr.get("Template")
            if not (isinstance(tmpl, tuple) and tmpl[1]):
                continue
            where = f"{e['name']} ({pkg.obj_name(tmpl[1])})"
            if cpr.get("bAutoActivate", True) is False or cpr.get("bSuppressSpawning") or cpr.get("bWasDeactivated"):
                self.skipped.append(f"{where}: starts off")
                continue
            if control.get(i) == "stopped":
                self.skipped.append(f"{where}: Kismet stops or plays it")
                continue
            loc, axes = actor_frame(pr)
            if cls in EMITTER_CLASSES:
                s = self.system(pkg, tmpl[1])
                if s is None:
                    continue
                params = {}
                for p in struct_array(pkg, cpr.get("InstanceParameters")) if "InstanceParameters" in cpr else []:
                    if p.get("ParamType") == "PSPT_Color" and isinstance(p.get("Color"), tuple):
                        params[str(p.get("Name"))] = _bgra(p["Color"][2])
                # (Lit by the level's lights only where the game lights it: its
                # component accepts lights and has a light environment. An engine
                # particle component does neither by default.)
                lit = bool(cpr.get("bAcceptsLights")) and cpr.get("LightEnvironment") is not None
                self.places.append((s, loc, axes, params, lit))
            else:
                f = self.flare(pkg, tmpl[1])
                if f is None:
                    continue
                cm = cpr.get("ColorMultiplier")
                color = struct.unpack("<4f", cm[2][:16]) if isinstance(cm, tuple) else (1.0, 1.0, 1.0, 1.0)
                self.flare_places.append((f, loc, axes, color, float(cpr.get("DrawSizeMultiplier", 1.0))))

    # -- output --

    def texture_stems(self, out):
        """The file name each material texture here is written under: its own name,
        unless two different textures of that name are drawn here, as the game's
        packages have them (FX_OS_Common_Textures_p's Streak_CLR, the thin streak
        Streak_horz_mat draws in the Photon Burst Rifle's flash, and FX_Textures_p's,
        the broad bar of the tank's tracer, Tank_Tracer_MAT): then those with the
        pixels a file of that name already holds (the first's, if there is none yet)
        keep it, and the others are named after their paths too
        (Streak_CLR__FX_OS_Common_Textures_p_Textures). Copies of one picture in
        several packages (DiffClouds_CLR) share the one file. Only a name drawn
        twice is checked: a texture drawn here alone takes the file of its name as
        it is."""
        sources = {}
        for _name, c in self.mats:
            for tpkg, tidx in c.textures:
                name = tpkg.exports[tidx - 1]["name"]
                sources.setdefault(name, {}).setdefault(tpkg.full_name(tidx), (tpkg, tidx))
        stems = {}
        folder = os.path.join(out, "textures", "levelfx")
        for name, each in sources.items():
            if len(each) < 2:
                continue
            pictures = {full: texture.read_texture(tpkg, tidx, self.cooked)[0][..., :3].astype(np.float32)
                        for full, (tpkg, tidx) in each.items()}
            have, holder = pictures[next(iter(each))], None
            for ext in ("ktx2", "png"):
                path = os.path.join(folder, f"{name}.{ext}")
                if os.path.exists(path):
                    with open(path, "rb") as fh:
                        data = fh.read()
                    img = ktx2_io.decode(data) if ext == "ktx2" else Image.open(path).convert("RGBA")
                    have, holder = np.asarray(img, dtype=np.float32)[..., :3], f"textures/levelfx/{name}.{ext}"
                    break
            for full, img in pictures.items():
                # (Block compression leaves the same picture within a few levels.)
                same = img.shape == have.shape and float(np.abs(img - have).mean()) < 8.0
                if not same:
                    stems[full] = f"{name}__{full.rsplit('.', 1)[0].replace('.', '_')}"
                    print(f"  {full}: {stems[full]} (another {name} {'is ' + holder if holder else 'is drawn'})")
        return stems

    def texture_file(self, out, tpkg, tidx):
        """Writes a material texture (once) and returns (path, sRGB, address U, V)."""
        e = tpkg.exports[tidx - 1]
        pr = self.lib.props(tpkg, tidx)
        name = self.stems.get(tpkg.full_name(tidx), e["name"])
        srgb = bool(pr.get("SRGB", True))
        au = ADDRESS.get(pr.get("AddressX", "TA_Wrap"), "wrap")
        av = ADDRESS.get(pr.get("AddressY", "TA_Wrap"), "wrap")
        folder = os.path.join(out, "textures", "levelfx")
        os.makedirs(folder, exist_ok=True)
        for ext in ("ktx2", "png"):
            if os.path.exists(os.path.join(folder, f"{name}.{ext}")):
                return f"textures/levelfx/{name}.{ext}", srgb, au, av
        img, _ = texture.read_texture(tpkg, tidx, self.cooked)
        h, w = img.shape[:2]
        if img[..., 3].min() == 255:
            img = img[..., :3]
        if min(w, h) < 16:
            path = f"textures/levelfx/{name}.png"
            Image.fromarray(np.ascontiguousarray(img)).save(os.path.join(out, path))
        else:
            path = f"textures/levelfx/{name}.ktx2"
            with open(os.path.join(out, path), "wb") as fh:
                fh.write(ktx2_io.encode(img, "color" if srgb else "data"))
        return path, srgb, au, av

    def cube_file(self, out, cpkg, cidx):
        """Writes a material's cube map (once: its faces' DXT1 or DXT5 data as BC1 or
        BC3, with their mips) and returns its path."""
        name = cpkg.exports[cidx - 1]["name"]
        path = f"textures/levelfx/{name}.ktx2"
        if os.path.exists(os.path.join(out, path)):
            return path
        fmt, edge, faces = texture.read_cube(cpkg, cidx)
        kind = {"PF_DXT1": "bc1", "PF_DXT5": "bc3"}.get(fmt)
        if kind is None:
            raise ValueError(f"{name}: cube map format {fmt}")
        srgb = bool(self.lib.props(cpkg, cidx).get("SRGB", True))
        n = min(len(f) for f in faces)
        levels = [b"".join(f[m] for f in faces) for m in range(n)]
        os.makedirs(os.path.join(out, "textures", "levelfx"), exist_ok=True)
        with open(os.path.join(out, path), "wb") as fh:
            fh.write(ktx2_io.write(kind, srgb, edge, edge, levels, faces=6))
        return path

    def shader_def(self, k):
        return f"FXM_{self.stem.upper()}_{k}"

    def write(self, out):
        lines = [f"# {self.stem}'s effects, read from the game by tools/levelfx.py (see there and src/levelfx.rs).",
                 "# Positions, directions and sizes in Unreal's axes and units (centimetres)."]
        pieces = [f"// {self.stem}'s effect materials, from the game's material graphs (tools/levelfx.py,",
                  "// tools/ue3/fxmat.py). Put into shaders/levelfx.wgsl.", ""]
        self.stems = self.texture_stems(out)
        for k, (name, c) in enumerate(self.mats):
            texs = []
            for tpkg, tidx in c.textures:
                path, srgb, au, av = self.texture_file(out, tpkg, tidx)
                texs.append(f"{path} {int(srgb)} {au} {av}")
            lit = int(c.lit and BLENDS[c.blend] == "translucent"
                      or k in self.lit_solid and c.model != "MLM_Unlit")
            cube = f" cuberough={_num(c.cube_roughness)}" if c.cube_roughness is not None else ""
            nofog = " nofog=1" if k in self.nofog else ""
            own_cube = f" cubetex={self.cube_file(out, *c.cube)}" if getattr(c, "cube", None) else ""
            distort = " distort=1" if getattr(c, "distorts", False) else ""
            lines.append(f"mat {k} {self.shader_def(k)} {name} blend={BLENDS[c.blend]} lit={lit} "
                         f"twosided={int(c.two_sided)} scene={int(c.uses_scene)} clip={_num(c.opacity_clip)}{cube}"
                         f"{nofog}{own_cube}{distort}")
            lines += [f" tex {t}" for t in texs]
            pieces += [f"#ifdef {self.shader_def(k)}", f"// {' <- '.join(c.chain)}", c.wgsl, "#endif", ""]
        if self.cube:
            lines.append(f"cube {self.cube[0]} {self.cube[1]}")
        for k, s in enumerate(self.systems):
            lines.append(f"system {k} {s.name} warmup={_num(s.warmup)} lit={int(s.lit)}")
            for dist, j in getattr(s, "lods", []):
                lines.append(f"lod {_num(dist)} {j}")
            for em in s.emitters:
                lines += particles.emitter_lines(em, em.mat)
        for s, loc, axes, params, lit in self.places:
            lines.append(f"place {s} " + " ".join(_num(v) for v in list(loc) + list(np.ravel(axes))) + f" {int(lit)}")
            for pname, rgba in params.items():
                lines.append(f" param {pname} " + " ".join(str(int(v)) for v in rgba))
        for k, (name, elements, screen) in enumerate(self.flares):
            lines.append(f"flare {k} {name} screenmap {screen.text()}")
            for el in elements:
                lines.append(f" element {'source' if el['source'] else 'reflection'} mat={el['mat']} "
                             f"ray={_num(el['ray'])} size={','.join(_num(v) for v in el['size'])} "
                             f"normrad={el['normrad']} usesrc={el['usesrc']}")
                lines += [f"  {key} {d.text()}" for key, d in el["dists"]]
        for f, loc, axes, color, size in self.flare_places:
            lines.append(f"flareplace {f} " + " ".join(_num(v) for v in list(loc) + list(np.ravel(axes)) + list(color))
                         + f" {_num(size)}")
        txt = os.path.join(out, *self.txt_path.split("/"))
        os.makedirs(os.path.dirname(txt), exist_ok=True)
        with open(txt, "w") as fh:
            fh.write("\n".join(lines) + "\n")
        self.write_meshes(out)
        shader_dir = os.path.join(out, "shaders", "levelfx")
        os.makedirs(shader_dir, exist_ok=True)
        with open(os.path.join(shader_dir, f"{self.stem}.wgsl"), "w") as fh:
            fh.write("\n".join(pieces) + "\n")
        write_shader(out)
        print(f"wrote {self.txt_path} ({len(self.mats)} materials, {len(self.systems)} particle systems "
              f"placed {len(self.places)} times, {len(self.flares)} lens flares placed {len(self.flare_places)} times)")
        for s in self.skipped:
            print(f"  (left out: {s})")


def write_shader(out):
    """shaders/levelfx.wgsl: the template with every built map's materials."""
    shader_dir = os.path.join(out, "shaders", "levelfx")
    parts = []
    if os.path.isdir(shader_dir):
        for f in sorted(os.listdir(shader_dir)):
            if f.endswith(".wgsl"):
                with open(os.path.join(shader_dir, f)) as fh:
                    parts.append(fh.read().rstrip())
    with open(TEMPLATE) as fh:
        template = fh.read()
    head = ("// GENERATED by tools/levelfx.py from tools/levelfx_shader.wgsl and the built maps' materials\n"
            "// (shaders/levelfx/<map>.wgsl): edit those, not this.\n\n")
    text = head + template.replace("//{{MATERIALS}}", "\n\n".join(parts) if parts else "// (none built)")
    with open(os.path.join(out, "shaders", "levelfx.wgsl"), "w") as fh:
        fh.write(text)


def fx_mesh_attributes(d, at, nverts, colours=True):
    """An effect mesh primitive's vertex data as levelfx.rs's shader takes it:
    position, normal, tangent, both UV sets (a missing second set repeats the first,
    as UE3 does) and colours (white when it has none: `colours` False, a mesh the
    package says has none, LevelFx.mesh_has_colours, whatever umodel wrote)."""
    pos = d.acc(at["POSITION"]).astype(np.float32)
    nrm = d.acc(at["NORMAL"]).astype(np.float32)
    uv0 = d.acc(at["TEXCOORD_0"]).astype(np.float32)
    uv1 = d.acc(at["TEXCOORD_1"]).astype(np.float32) if "TEXCOORD_1" in at else uv0
    if "TANGENT" in at:
        tan = d.acc(at["TANGENT"]).astype(np.float32)
    else:
        ref = np.where(np.abs(nrm[:, 1:2]) < 0.9, [[0.0, 1.0, 0.0]], [[1.0, 0.0, 0.0]])
        t = np.cross(ref, nrm)
        t /= np.maximum(np.linalg.norm(t, axis=1, keepdims=True), 1e-9)
        tan = np.concatenate([t, np.ones((len(t), 1))], 1).astype(np.float32)
    if "COLOR_0" in at and colours:
        col = d.acc(at["COLOR_0"]).astype(np.float32)
        if col.max() > 1.0:
            col = col / 255.0
        if col.shape[1] == 3:
            col = np.concatenate([col, np.ones((len(col), 1), np.float32)], 1)
    else:
        col = np.ones((nverts, 4), np.float32)
    return pos, nrm, tan, uv0, uv1, col


def fx_gltf_material(stem, index, name, sky=False):
    """The glTF material standing for effect material `index`: levelfx.rs draws its
    meshes with the effect material instead (extras "fx:<n>"); by itself it is
    invisible. `sky`: for effect meshes in the sky, whose vertex colours are the fog
    they are seen through (build_map.py's SKY_FX_DISTANCE; extras "skyfog")."""
    return {"name": f"fx_{name}" + ("_sky" if sky else ""),
            "extras": {"foc": f"fx:{index} noshadow" + (" skyfog" if sky else "")},
            "extensions": {"KHR_materials_unlit": {}}, "alphaMode": "BLEND",
            "pbrMetallicRoughness": {"baseColorFactor": [1.0, 1.0, 1.0, 0.0], "metallicFactor": 0.0}}
