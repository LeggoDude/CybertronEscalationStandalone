"""Which of the game's vertex factories each material has compiled shaders for, read
from the shader caches cooked into its packages (their ShaderCache objects).

A particle emitter is drawn with the vertex factory its kind calls for (sprites,
sub-UV sprites or beams and trails; meshes with the mesh's own), with dynamic
parameters if its material reads them or its modules set them. Fall of Cybertron's
cooker compiles a material's particle shaders with dynamic parameters only if the
material reads them, and without them only if it doesn't, so where material and
modules disagree only one way can be drawn:

  * a material that reads dynamic parameters on an emitter with no DynamicParameter
    module is drawn (TFOC.exe: the sprite, sub-UV and beam and trail emitters' render
    data set bUsesDynamicParameter, and pick their vertex factory, from the
    material's resource; the parameters are 1 in the demo, src/levelfx.rs's
    dynamic_parameters_without_a_module_are_1). The game's videos show them: the
    Intercooler's buff (Intercooler_FX, whose two emitters are both such) crackling
    with its lightning and its cyan streaks round the player, the EMP grenade's
    lightning on whoever it catches;
  * an emitter with a DynamicParameter module whose material doesn't read them never
    shows in the game's videos, though the material has shaders for plain particles:
    the boosters' cross beams (Booster_Jazz_FX's CrossBeamVert_Export and
    CrossBeamHorz_Export, Glow_Mod_Depth_MAT: two glows 10 to 15 m long through
    each booster that no video of a boosting car shows) and the X-18 Scrapmaker's
    dark smoke (Smoke_Dark, Clouds_Basic_MAT: dark puffs 1.5 to 5 m across in front
    of the muzzle, which its videos never show against the bright corridor), so the
    demo takes the module to call for the shaders with them, which the material
    hasn't got, and leaves them out;

and so is an emitter of a kind its material has no shaders for at all (a sub-UV
emitter whose material was compiled for plain sprites only).

    caches = shadercache.Caches(lib)               # fxmat.Library's packages
    caches.vertex_factories("Mod_Glow2x2_MAT")     # set of names, or None if unknown
    shadercache.drawn_with(emitter, have)          # the one the game draws a particles.Emitter with
"""
import struct

# A ShaderCache object (FoC's): an int at 25 counts the shaders, which follow from 29,
# each [type name 8][id 16][end offset 4 (in the package)][target 2][code size 4]...
# Then the material shader maps: their count and each one's
#   static parameters: base material id 16, switches (count, 48 bytes each), component
#     masks (count, 52 each)
#   two words, the record's end offset (in the package)
#   the material's own shaders (count, 32 bytes each)
#   the mesh shader maps (count; each its shaders (count, 32 bytes each) and then its
#     vertex factory's name)
#   the friendly name ("Material Package.Group.Name"), and the rest up to the end.
SWITCH_SIZE = 48
MASK_SIZE = 52
SHADER_REF_SIZE = 32


def shader_maps(pkg, switches=None):
    """{(class, object name): set of vertex factory names} for the shader maps cached
    in package `pkg`. With `switches` (a dict), also fills it with each map's static
    switch values: {(class, object name): {switch name: chosen input (0: the first,
    "Enabled")}}, as the material was compiled with them."""
    out = {}
    for e in pkg.exports:
        if pkg.class_name(e) != "ShaderCache":
            continue
        d = pkg.data[e["offset"]:e["offset"] + e["size"]]
        try:
            _read_maps(pkg, d, e["offset"], out, switches)
        except (struct.error, IndexError, UnicodeDecodeError, ValueError):
            # (A cache laid out some other way: its maps are left unknown.)
            continue
    return out


def _read_maps(pkg, d, base, out, switches=None):
    names = pkg.names

    def name_at(o):
        i = struct.unpack_from("<i", d, o)[0]
        return names[i] if 0 <= i < len(names) else None

    count = struct.unpack_from("<I", d, 25)[0]
    o = 29
    for _ in range(count):
        o = struct.unpack_from("<I", d, o + 24)[0] - base
    maps = struct.unpack_from("<I", d, o)[0]
    o += 4
    for _ in range(maps):
        o += 16
        n = struct.unpack_from("<I", d, o)[0]
        # A switch: its name, category, the input chosen, its inputs' names,
        # whether the material sets it, and a GUID.
        chosen = {}
        for k in range(n):
            ints = struct.unpack_from("<8i", d, o + 4 + SWITCH_SIZE * k)
            sw = names[ints[0]] if 0 <= ints[0] < len(names) else None
            if sw is not None:
                chosen[sw] = ints[4]
        o += 4 + SWITCH_SIZE * n
        n = struct.unpack_from("<I", d, o)[0]
        o += 4 + MASK_SIZE * n
        end = struct.unpack_from("<I", d, o + 8)[0] - base
        o += 12
        n = struct.unpack_from("<I", d, o)[0]
        o += 4 + SHADER_REF_SIZE * n
        n = struct.unpack_from("<I", d, o)[0]
        o += 4
        vfs = set()
        for _ in range(n):
            k = struct.unpack_from("<I", d, o)[0]
            if k > 1000:
                raise ValueError("bad shader count")
            o += 4 + SHADER_REF_SIZE * k
            vf = name_at(o)
            if vf is None:
                raise ValueError("bad vertex factory name")
            vfs.add(vf)
            o += 8
        n = struct.unpack_from("<i", d, o)[0]
        if not 0 < n < 1024:
            raise ValueError("bad friendly name")
        friendly = d[o + 4:o + 3 + n].decode("latin-1")
        cls, _, path = friendly.partition(" ")
        out.setdefault((cls, path.split(".")[-1]), set()).update(vfs)
        if switches is not None and chosen:
            switches.setdefault((cls, path.split(".")[-1]), chosen)
        if end <= o:
            raise ValueError("bad record end")
        o = end


class Caches:
    """The shader maps cached in a material library's packages."""

    def __init__(self, lib):
        self.lib = lib
        self.maps = {}
        for pkg in lib.pkgs:
            for key, vfs in shader_maps(pkg).items():
                self.maps.setdefault(key, set()).update(vfs)

    def vertex_factories(self, name):
        """The vertex factories material `name` can be drawn with: those of the shader
        map it uses (an instance's own, if its static parameters give it one, else its
        parent's). None if the caches don't have it."""
        import fxmat
        found = self.lib.find(name, fxmat.MATERIAL_CLASSES)
        for _ in range(16):
            if not found:
                return None
            pkg, idx = found
            e = pkg.exports[idx - 1]
            cls = pkg.class_name(e)
            own = self.maps.get((cls, e["name"]))
            if own is not None:
                return own
            if cls == "Material":
                return None
            parent = self.lib.props(pkg, idx).get("Parent")
            if not parent or not parent[1]:
                return None
            found = self.lib.resolve_ref(pkg, parent[1], fxmat.MATERIAL_CLASSES)
        return None


def drawn_with(em, have):
    """The vertex factory the game draws particles.Emitter `em` with, its material
    having shaders for the vertex factories `have` (None for mesh and decal emitters,
    drawn with their meshes' and decals' own): its kind's, with dynamic parameters if
    the material reads them (its particle shaders are those with them) or the emitter
    has a DynamicParameter module (see the top)."""
    reads = any(v.startswith("FParticle") and "DynamicParameter" in v for v in have)
    return _kind_vf(em, "DynamicParameter" if reads or _sets_dynamic(em) else "")


def needed(em):
    """The vertex factory particles.Emitter `em`'s modules call for (with dynamic
    parameters if it has a DynamicParameter module), whatever its material reads;
    None for mesh and decal emitters."""
    return _kind_vf(em, "DynamicParameter" if _sets_dynamic(em) else "")


def _sets_dynamic(em):
    return any(m.kind == 0x1D for m in em.modules)


def _kind_vf(em, dynamic):
    td = em.typedata
    if td is None:
        kind = "FParticleSubUV" if em.req.get("interp", 0) != 0 else "FParticle"
    elif td["kind"] in ("trail", "beam"):
        kind = "FParticleBeamTrail"
    else:
        return None
    return f"{kind}{dynamic}VertexFactory"
