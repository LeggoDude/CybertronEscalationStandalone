"""FoC's particle systems (ParticleSystem), read for the demo's level effects
(src/levelfx.rs simulates them as the game's runtime does).

A ParticleSystem holds emitters; an emitter its LOD levels (the demo uses the first,
the most detailed). A LOD level's settings are its ParticleModuleRequired (tagged
properties: material, alignment, spawn rate, bursts, sub-images...), the most
particles it has out at once (PeakActiveParticles), its TypeDataModule if it draws
something other than sprites (trails, beams, meshes and High Moon's decals are read)
and, after its tagged properties, High Moon's compiled form of its modules:

  u32 runtime size, u32 hash, 16 zero bytes, a byte, u32 module count
  per module 17 bytes: u16 parameter offset, u16 first float distribution,
      u16 first vector distribution, u16, u16 particle payload offset, u8 type,
      u8, u8, i32 the module object (type 0: a module kept as an object)
  u32 count + the spawn modules' indices (bytes), u32 count + the update modules'
  u32 0, u32 count + the spawning modules' indices (bytes: the modules that have a
      say in how many particles spawn, UE3's SpawningModules: SpawnPerUnit and
      LocationEmitter), u32 0
  u32 size + the modules' in-place parameters (a module's at its offset),
      u32 count + names (u32 name index, u32 number)
  u32 count + float distributions, u32 count + vector distributions
  u32 particle size, i32 -1

A distribution is u8 type, u8 operation, u8 values per entry, u8 entry size, u32
count + floats (Dist below). The module types, with their distributions in order
and their in-place parameters, are in MODULES.

    lib = fxmat.Library(cooked, packages)
    system = read_system(lib, pkg, index)   # -> System (or raises ParticleError)
"""
import re
import struct

from fxmat import props_of, sub, struct_array, obj_array
from ue3pkg import Reader


class ParticleError(Exception):
    pass


# ---------------------------------------------------------------------------
# Distributions
# ---------------------------------------------------------------------------

# Type: 1 constant, 2 curve, 3 uniform (two values), 4 uniform curve; 7-10 the same
# for vectors. Op: 1 plain, 2 random (between two values), 3 extreme (one or the
# other). Curves start with min, max, start time, time scale; then their entries.
FLOAT_TYPES = (1, 2, 3, 4)
VECTOR_TYPES = (7, 8, 9, 10)


class Dist:
    def __init__(self, kind, op, table):
        self.kind = kind
        self.op = op
        self.table = [float(x) for x in table]

    def text(self):
        return f"{self.kind} {self.op} {len(self.table)} " + " ".join(_num(x) for x in self.table)

    def __repr__(self):
        return f"Dist({self.kind}, {self.op}, {self.table[:8]})"


def _num(x):
    if x != x or x in (float("inf"), float("-inf")):
        x = 0.0
    s = f"{x:.6g}"
    return "0" if s == "-0" else s


def const_float(v):
    return Dist(1, 1, [v])


def const_vector(v):
    return Dist(7, 1, list(v))


def _read_dist(b, o):
    kind, op, _n, _chunk = b[o], b[o + 1], b[o + 2], b[o + 3]
    count = struct.unpack_from("<i", b, o + 4)[0]
    if not 0 <= count <= 65536 or o + 8 + 4 * count > len(b):
        raise ParticleError("bad distribution")
    return Dist(kind, op, struct.unpack_from(f"<{count}f", b, o + 8)), o + 8 + 4 * count


def raw_dist(pkg, v, vector):
    """A tagged RawDistributionFloat / RawDistributionVector property (cooked to a
    table in the same form). Missing fields take the defaults; a missing type is
    worked out from the table."""
    s = sub(pkg, v)
    if s is None:
        return None
    table = s.get("LookupTable")
    vals = []
    if isinstance(table, tuple) and table[0] == "array" and len(table[1]) >= 4:
        n = struct.unpack_from("<i", table[1])[0]
        vals = list(struct.unpack_from(f"<{n}f", table[1], 4))
    kind = s.get("Type")
    op = s.get("Op", 1)
    if kind is None:
        if vector:
            kind = 7 if len(vals) <= 3 else 8
        else:
            kind = 1 if len(vals) <= 1 else 2
    if not vals:
        vals = [0.0] * (3 if vector else 1)
    if kind in (3, 4, 9, 10) and op == 1:
        op = 2
    return Dist(kind, op, vals)


# ---------------------------------------------------------------------------
# Modules
# ---------------------------------------------------------------------------

# type: (UE3 module, float distributions, vector distributions, in-place words)
MODULES = {
    0x01: ("Lifetime", ["Lifetime"], [], 0),
    0x02: ("Color", ["StartAlpha"], ["StartColor"], 0),
    0x03: ("ColorByParameter", [], [], 2),          # name (in the name table), DefaultColor (BGRA)
    0x04: ("ColorOverLife", ["AlphaOverLife"], ["ColorOverLife"], 0),
    0x05: ("ColorScaleOverLife", ["AlphaScaleOverLife"], ["ColorScaleOverLife"], 1),  # bEmitterTime
    0x06: ("Size", [], ["StartSize"], 0),
    # (The engine's EParticleModuleId: 0x07 is SizeMultiplyVelocity, 0x09 SizeScale.)
    0x07: ("SizeMultiplyVelocity", [], ["VelocityMultiplier"], 3),  # MultiplyX, MultiplyY, MultiplyZ
    0x08: ("SizeMultiplyLife", [], ["LifeMultiplier"], 3),  # MultiplyX, MultiplyY, MultiplyZ
    0x09: ("SizeScale", [], ["SizeScale"], 3),      # EnableX, EnableY, EnableZ
    # A mesh particle's turn (X roll, Y pitch, Z yaw; 1 = a full turn), bInheritParent.
    0x0a: ("MeshRotation", [], ["StartRotation"], 1),
    # Scales a mesh particle's turn rate by its LifeMultiplier at the particle's age,
    # as it spawns and every frame after (its MeshRotationRate, reset to the rate
    # it spawned with each frame, times the curve).
    0x0c: ("MeshRotationRateMultiplyLife", [], ["LifeMultiplier"], 0),
    0x0d: ("Rotation", ["StartRotation"], [], 0),
    0x0f: ("RotationRate", ["StartRotationRate"], [], 0),
    0x10: ("RotationRateMultiplyLife", ["LifeMultiplier"], [], 0),
    # A mesh particle's turn rate (turns a second, X roll, Y pitch, Z yaw).
    0x0b: ("MeshRotationRate", [], ["StartRotationRate"], 0),
    0x11: ("Location", [], ["StartLocation"], 0),
    # Spawns each particle where one of another emitter's particles is (High Moon's
    # compiled form: the source emitter's name (in the name table),
    # InheritSourceVelocityScale, InheritSourceRotationScale (floats),
    # SelectionMethod (0 random, 1 in turn), InheritSourceVelocity,
    # bInheritSourceRotation, and a beam setting; its distribution is the beams'
    # BeamSpawnCoordinate).
    0x13: ("LocationEmitter", ["BeamSpawnCoordinate"], [], 7),
    # Each particle where the one of another emitter's particles of the same place
    # in its list is, from spawning and every frame after: there, as fast and as old
    # (UE3's LocationEmitterDirect; High Moon's compiled form: the source emitter's
    # name (in the name table), its place among the system's emitters and a word
    # more, its distribution LocationEmitter's).
    0x14: ("LocationEmitterDirect", ["BeamSpawnCoordinate"], [], 3),
    # Positive_X/Y/Z, Negative_X/Y/Z, SurfaceOnly, Velocity, RadialVelocity, HeightAxis
    0x15: ("LocationPrimitiveCylinder", ["VelocityScale", "StartRadius", "StartHeight"], ["StartLocation"], 10),
    # Positive_X/Y/Z, Negative_X/Y/Z, SurfaceOnly, Velocity
    0x16: ("LocationPrimitiveSphere", ["VelocityScale", "StartRadius"], ["StartLocation"], 8),
    0x17: ("Velocity", ["StartVelocityRadial"], ["StartVelocity"], 0),
    # New particles take on this much of the emitter's own velocity.
    0x18: ("VelocityInheritParent", [], ["Scale"], 0),
    0x19: ("VelocityOverLifetime", [], ["VelOverLife"], 1),  # Absolute
    0x1a: ("Acceleration", [], ["Acceleration"], 0),
    0x1b: ("AccelerationOverLifetime", [], ["AccelOverLife"], 0),
    0x1c: ("HmGravity", [], [], 1),                 # the gravity's multiplier (a float)
    # bUseEmitterTime x 4, bSpawnTimeOnly x 4
    0x1d: ("DynamicParameter", ["Param0", "Param1", "Param2", "Param3"], [], 8),
    0x1f: ("SubUV", ["SubImageIndex"], [], 0),
    0x22: ("HmWind", [], [], 3),
}
# Module objects (type 0) the demo runs, given types of their own.
ATTRACTOR_POINT = 0x80
# ParticleModuleSpawnPerUnit: particles spawned by the distance the emitter moves.
# Its distribution is SpawnPerUnit; its words UnitScalar and MovementTolerance (as
# floats' bits), bIgnoreSpawnRateWhenMoving and bProcessSpawnRate.
SPAWN_PER_UNIT = 0x81
# ParticleModuleSizeMultiplyLife whose LifeMultiplier is a vector parameter the game
# sets on the effect (DistributionVectorParticleParameter: a jet's hover jets'
# Size, set by how hard each pushes, TnPlaneForm.Hovering.UpdateFx). Its name is the
# parameter's; its words MultiplyX/Y/Z, the three ParamModes (0 normal, 1 its size,
# 2 direct), then as floats' bits MinInput, MaxInput, MinOutput, MaxOutput and
# Constant (what it is when the game hasn't set it), three each.
SIZE_BY_PARAMETER = 0x82
# ParticleModuleSwitchableColorScaleOverLife (High Moon's): ColorScaleOverLife with
# two sets of curves, A and B, the effect's module channel picking which
# (ParticleSystemComponent.SwitchModuleChannel: TnParticleSystemParameterTeamIndex
# switches it to A, 1, for team 0's effects and to B, 2, for team 1's; A until it
# is switched, the module's ChannelSwitch default). Its distributions are
# AlphaScaleOverLifeA, AlphaScaleOverLifeB, ColorScaleOverLifeA and
# ColorScaleOverLifeB, over each particle's life.
SWITCH_COLOR_SCALE = 0x83
# The modules whose first word names something in the emitter's name table
# (ColorByParameter its parameter, LocationEmitter and LocationEmitterDirect their
# source emitter).
NAMED = (0x03, 0x13, 0x14)

# The required module's settings, with UE3's defaults.
SCREEN_ALIGN = {"PSA_Square": 0, "PSA_Rectangle": 1, "PSA_Velocity": 2, "PSA_TypeSpecific": 3,
                "PSA_FacingCameraPosition": 4}
SORT_MODES = {"ESM_None": 0, "ESM_Depth": 1, "ESM_DistanceToView": 2, "ESM_OldestFirst": 3, "ESM_NewestFirst": 4,
              "PSORTMODE_None": 0, "PSORTMODE_ViewProjDepth": 1, "PSORTMODE_DistanceToView": 2,
              "PSORTMODE_Age_OldestFirst": 3, "PSORTMODE_Age_NewestFirst": 4}
# The emitter's axis lock (EParticleAxisLock; High Moon's "_U" variants are taken as
# the plain ones).
LOCK_AXIS = {"EPAL_NONE": 0, "EPAL_X": 1, "EPAL_Y": 2, "EPAL_Z": 3, "EPAL_NEGATIVE_X": 4, "EPAL_NEGATIVE_Y": 5,
             "EPAL_NEGATIVE_Z": 6, "EPAL_ROTATE_X": 7, "EPAL_ROTATE_Y": 8, "EPAL_ROTATE_Z": 9}
INTERP = {"PSUVIM_None": 0, "PSUVIM_Linear": 1, "PSUVIM_Linear_Blend": 2, "PSUVIM_Random": 3,
          "PSUVIM_Random_Blend": 4}


class Module:
    def __init__(self, kind, floats, vectors, words):
        self.kind = kind
        self.floats = floats
        self.vectors = vectors
        self.words = words      # in-place parameters (u32 words)
        self.name = None        # ColorByParameter's parameter name

    def lines(self, index, spawn, update):
        out = [f"module {index} {self.kind} {int(spawn)} {int(update)}"]
        out += [f" f {d.text()}" for d in self.floats]
        out += [f" v {d.text()}" for d in self.vectors]
        if self.words:
            out.append(" w " + " ".join(str(w) for w in self.words))
        if self.name:
            out.append(f" name {self.name}")
        return out


class Emitter:
    def __init__(self):
        self.name = ""
        self.material = None    # (package, export) of its material
        self.material_name = ""
        # What it draws (its TypeDataModule): None for sprites, else a dict with
        # "kind" ("trail", "beam", "mesh" or "decal") and that kind's settings.
        self.typedata = None
        self.req = {}
        # The most particles the game has out of it at once (its LOD level's
        # PeakActiveParticles; src/levelfx.rs holds an emitter whose particles never
        # die to it).
        self.peak = 0
        # (ParticleModuleRequired's SpawnRate as the engine's class default has it:
        # none, its RequiredDistributionSpawnRate a constant 0.)
        self.rate = const_float(0.0)
        self.bursts = []        # (count, count low, time)
        self.modules = []
        self.spawn = []
        self.update = []
        # The modules with a say in how many particles spawn (SpawnPerUnit), and
        # where (LocationEmitter): UE3's SpawningModules.
        self.spawning = []


class System:
    def __init__(self, name):
        self.name = name
        self.warmup = 0.0
        self.lit = False
        self.emitters = []
        self.left_out = []      # emitters that can't be drawn, and why
        # The distances (cm from the camera) from which each of its levels of
        # detail is used (LODDistances: the first is 0), and the level read.
        self.lod_distances = [0.0]
        self.level = 0


def _words(b, o, n):
    return list(struct.unpack_from(f"<{n}I", b, o)) if n and o + 4 * n <= len(b) else [0] * n


def _lod_blob(pkg, lod_idx):
    """A LOD level's tagged properties and the bytes after them."""
    e = pkg.exports[lod_idx - 1]
    for st in (4, 0, 8, 12):
        try:
            from props import read_props
            r = Reader(pkg.data, e["offset"] + st)
            pr = read_props(pkg, r, e["offset"] + e["size"])
        except Exception:
            continue
        if pr and "RequiredModule" in pr:
            return pr, pkg.data[r.o:e["offset"] + e["size"]]
    raise ParticleError(f"{e['name']}: no LOD properties")


def _parse_blob(b):
    if len(b) < 29:
        raise ParticleError("short module data")
    n = struct.unpack_from("<i", b, 25)[0]
    if not 0 <= n <= 64:
        raise ParticleError("bad module count")
    o = 29
    recs = []
    for _ in range(n):
        pay, f0, v0, _c, _inst = struct.unpack_from("<HHHHH", b, o)
        kind = b[o + 10]
        obj = struct.unpack_from("<i", b, o + 13)[0]
        recs.append({"type": kind, "payload": pay, "f0": f0, "v0": v0, "obj": obj})
        o += 17
    cnt = struct.unpack_from("<i", b, o)[0]
    spawn = list(b[o + 4:o + 4 + cnt])
    o += 4 + cnt
    cnt = struct.unpack_from("<i", b, o)[0]
    update = list(b[o + 4:o + 4 + cnt])
    o += 4 + cnt
    # The spawning modules (UE3's SpawningModules), between two zero words.
    if struct.unpack_from("<i", b, o)[0] != 0:
        raise ParticleError("unexpected parameter header")
    cnt = struct.unpack_from("<i", b, o + 4)[0]
    if not 0 <= cnt <= 64:
        raise ParticleError("unexpected parameter header")
    spawning = list(b[o + 8:o + 8 + cnt])
    o += 8 + cnt
    # Another list of module indices, mostly empty (a ParticleModule of type 0x1e
    # is in it in Harvester_PlayerHeal_FX's Sparkler): skipped.
    cnt = struct.unpack_from("<i", b, o)[0]
    if not 0 <= cnt <= 64:
        raise ParticleError("unexpected parameter header")
    o += 4 + cnt
    psize = struct.unpack_from("<i", b, o)[0]
    payload = b[o + 4:o + 4 + psize]
    o += 4 + psize
    nn = struct.unpack_from("<i", b, o)[0]
    names = [struct.unpack_from("<ii", b, o + 4 + 8 * k) for k in range(nn)]
    o += 4 + 8 * nn
    floats, vectors = [], []
    for out in (floats, vectors):
        cnt = struct.unpack_from("<i", b, o)[0]
        o += 4
        if not 0 <= cnt <= 256:
            raise ParticleError("bad distribution count")
        for _ in range(cnt):
            d, o = _read_dist(b, o)
            out.append(d)
    if o + 8 != len(b):
        raise ParticleError("module data doesn't end where expected")
    return recs, spawn, update, spawning, payload, names, floats, vectors


def _attractor(lib, pkg, idx):
    pr = lib.props(pkg, idx)
    pos = raw_dist(pkg, pr.get("Position"), True) or const_vector((0, 0, 0))
    rng = raw_dist(pkg, pr.get("Range"), False) or const_float(0.0)
    strength = raw_dist(pkg, pr.get("Strength"), False) or const_float(0.0)
    words = [int(pr.get("StrengthByDistance", True)), int(bool(pr.get("bAffectBaseVelocity", False))),
             int(bool(pr.get("bOverrideVelocity", False)))]
    return Module(ATTRACTOR_POINT, [rng, strength], [pos], words)


# EDistributionParamMode.
PARAM_MODES = {"DPM_Normal": 0, "DPM_Abs": 1, "DPM_Direct": 2}


def _size_by_parameter(lib, pkg, idx):
    """A ParticleModuleSizeMultiplyLife kept as an object because its LifeMultiplier
    is a DistributionVectorParticleParameter (SIZE_BY_PARAMETER), with UE3's
    defaults for what it leaves unset; None for any other distribution."""
    pr = lib.props(pkg, idx)
    lm = sub(pkg, pr.get("LifeMultiplier")) if "LifeMultiplier" in pr else None
    d = lm.get("Distribution") if lm else None
    if not (isinstance(d, tuple) and d[0] == "obj" and d[1] and d[1] > 0):
        return None
    if pkg.class_name(pkg.exports[d[1] - 1]) != "DistributionVectorParticleParameter":
        return None
    dp = lib.props(pkg, d[1])

    def vec(key, default):
        v = dp.get(key)
        if isinstance(v, tuple) and v[0] == "struct" and len(v[2]) >= 12:
            return struct.unpack_from("<3f", v[2])
        return default

    bits = lambda f: struct.unpack("<I", struct.pack("<f", float(f)))[0]
    modes = dp.get("ParamModes")
    mode = PARAM_MODES.get(str(modes), 0) if modes is not None else 0
    words = [int(bool(pr.get(k, True))) for k in ("MultiplyX", "MultiplyY", "MultiplyZ")] + [mode] * 3
    for v in (vec("MinInput", (0.0, 0.0, 0.0)), vec("MaxInput", (1.0, 1.0, 1.0)),
              vec("MinOutput", (0.0, 0.0, 0.0)), vec("MaxOutput", (1.0, 1.0, 1.0)),
              vec("Constant", (0.0, 0.0, 0.0))):
        words += [bits(x) for x in v]
    m = Module(SIZE_BY_PARAMETER, [], [], words)
    m.name = str(dp.get("ParameterName", "None"))
    return m


def _switchable_color_scale(lib, pkg, idx):
    """A ParticleModuleSwitchableColorScaleOverLife (SWITCH_COLOR_SCALE), with its
    class's defaults for what it leaves unset (alpha 1, colour 0)."""
    pr = lib.props(pkg, idx)
    alpha = [_set_dist(pkg, pr.get(k), False, const_float(1.0)) for k in ("AlphaScaleOverLifeA", "AlphaScaleOverLifeB")]
    color = [_set_dist(pkg, pr.get(k), True, const_vector((0.0, 0.0, 0.0)))
             for k in ("ColorScaleOverLifeA", "ColorScaleOverLifeB")]
    return Module(SWITCH_COLOR_SCALE, alpha, color, [])


def _spawn_per_unit(lib, pkg, idx):
    """A ParticleModuleSpawnPerUnit kept as an object: its tagged properties, with
    UE3's defaults (UnitScalar 50, MovementTolerance 0.1, bProcessSpawnRate)."""
    pr = lib.props(pkg, idx)
    per = raw_dist(pkg, pr.get("SpawnPerUnit"), False) or const_float(0.0)
    bits = lambda f: struct.unpack("<I", struct.pack("<f", float(f)))[0]
    words = [bits(pr.get("UnitScalar", 50.0)), bits(pr.get("MovementTolerance", 0.1)),
             int(bool(pr.get("bIgnoreSpawnRateWhenMoving", False))),
             int(bool(pr.get("bProcessSpawnRate", True)))]
    return Module(SPAWN_PER_UNIT, [per], [], words)


# A beam's taper (TaperMethod): none, along the whole beam, or along the part of it
# drawn so far.
TAPER_METHODS = {"PEBTM_None": 0, "PEBTM_Full": 1, "PEBTM_Partial": 2}


def _set_dist(pkg, v, vector, default):
    """A RawDistribution property that holds a value, else `default` (a cooked one
    with neither a table nor a distribution is unset: the class default stands)."""
    s = sub(pkg, v) if v is not None else None
    if s is None or not (isinstance(s.get("LookupTable"), tuple) and len(s["LookupTable"][1]) >= 8):
        return default
    return raw_dist(pkg, v, vector)


def _beam(pkg, pr, name):
    """A ParticleModuleTypeDataBeam2: a beam from the emitter (the source) to a target
    for each particle, drawn as a ribbon facing the camera (UE3's defaults for what
    it leaves unset). Its source and target come from the emitter's BeamSource and
    BeamTarget modules (read_emitter). A Distance beam runs Distance units along the
    emitter's X instead (its target, unless the game sets where it ends)."""
    method = str(pr.get("BeamMethod", "PEB2M_Target"))
    if method == "None":
        # (SentryLaser_FX's is saved as None, a value the enum hasn't got: loaded as
        # the enum's first, PEB2M_Distance - assumed.)
        method = "PEB2M_Distance"
    if method not in ("PEB2M_Target", "PEB2M_Distance"):
        raise ParticleError(f"{name}: a {method} beam can't be drawn")
    speed = float(pr.get("Speed", 10.0))
    if 0.0 < speed < 1e6:
        # (A beam reaching out to its target over time: not drawn yet. A million
        # units a second or more is there at once.)
        raise ParticleError(f"{name}: a beam that grows to its target can't be drawn")
    distance = None
    if method == "PEB2M_Distance":
        d = _set_dist(pkg, pr.get("Distance"), False, None)
        distance = d.table[0] if d is not None and d.table else 25.0
    return {"kind": "beam",
            "max": int(pr.get("MaxBeamCount", 0)) or MAX_BEAMS,
            "tile": float(pr.get("TextureTile", 1)),
            "taper": TAPER_METHODS.get(str(pr.get("TaperMethod", "PEBTM_None")), 0),
            "factor": _set_dist(pkg, pr.get("TaperFactor"), False, const_float(1.0)),
            "scale": _set_dist(pkg, pr.get("TaperScale"), False, const_float(1.0)),
            # Where the beam starts and ends in the emitter's frame (BeamSource's
            # Source, BeamTarget's Target): the emitter itself, and UE3's default
            # target 50 units out along its X, unless the modules say otherwise.
            "source": (0.0, 0.0, 0.0),
            "target": (50.0, 0.0, 0.0) if distance is None else (float(distance), 0.0, 0.0),
            "distance": distance is not None,
            "target_param": "",
            # High Moon's BeamSineWave modules' waves (read_emitter).
            "waves": []}


# A beam emitter with no MaxBeamCount keeps at most this many beams.
MAX_BEAMS = 10


def _beam_module(lib, pkg, idx, td):
    """A BeamSource or BeamTarget module of a beam emitter, into its type data: the
    beam's start (its Source offset) or end (Target, and the instance parameter
    naming it when the game sets it: TargetMethod PEB2STM_UserSet's TargetName)."""
    cls = pkg.class_name(pkg.exports[idx - 1])
    pr = lib.props(pkg, idx)
    if cls == "ParticleModuleBeamSource":
        d = raw_dist(pkg, pr.get("Source"), True) if "Source" in pr else None
        if d is not None:
            td["source"] = tuple(d.table[:3])
    elif not td.get("distance"):
        # (A Distance beam has no target: its BeamTarget module is unused.)
        d = raw_dist(pkg, pr.get("Target"), True) if "Target" in pr else None
        if d is not None:
            td["target"] = tuple(d.table[:3])
        if str(pr.get("TargetMethod", "")) == "PEB2STM_UserSet":
            td["target_param"] = str(pr.get("TargetName", ""))


def _sine_waves(lib, pkg, idx, td):
    """A ParticleModuleBeamSineWave (High Moon's) of a beam emitter, into its type
    data: each of its SineWaves bends the beam by Amplitude units along Direction
    (the beam's frame: Z along it, so X and Y across it - read so, as the heal beam's
    two waves a quarter turn apart then wind it into a helix), Period units a turn
    along the beam, the waves running along it at Speed units a second, PhaseOffset
    turns in."""
    pr = lib.props(pkg, idx)
    for w in struct_array(pkg, pr.get("SineWaves")) if "SineWaves" in pr else []:
        dv = w.get("Direction")
        direction = struct.unpack("<3f", dv[2][:12]) if isinstance(dv, tuple) and len(dv) == 3 else (1.0, 0.0, 0.0)
        period = float(w.get("Period", 0.0))
        amp = float(w.get("Amplitude", 0.0))
        if period > 0.0 and amp != 0.0:
            td["waves"].append((amp, period, float(w.get("Speed", 0.0)), float(w.get("PhaseOffset", 0.0)),
                                direction))


def _typedata(lib, pkg, idx, name):
    """An emitter's TypeDataModule: what it draws instead of sprites (see
    Emitter.typedata). Anything else can't be drawn."""
    cls = pkg.class_name(pkg.exports[idx - 1]) if idx > 0 else None
    pr = lib.props(pkg, idx) if idx > 0 else {}
    if cls == "ParticleModuleTypeDataBeam2":
        return _beam(pkg, pr, name)
    if cls == "ParticleModuleTypeDataTrail2":
        # UE3's defaults for those left unset.
        return {"kind": "trail",
                "tess": int(pr.get("TessellationFactor", 1)),
                "strength": float(pr.get("TessellationStrength", 25.0)),
                "tile": int(pr.get("TextureTile", 1)),
                "sheets": max(1, int(pr.get("Sheets", 1))),
                "max": int(pr.get("MaxParticleInTrailCount", 500)),
                "moving": int(bool(pr.get("bEmitOnlyWhenMoving", False)))}
    if cls == "ParticleModuleTypeDataMesh":
        mesh = pr.get("Mesh")
        if not (isinstance(mesh, tuple) and mesh[1]):
            raise ParticleError(f"{name}: mesh emitter without a mesh")
        return {"kind": "mesh", "mesh": (pkg, mesh[1]), "mesh_name": pkg.obj_name(mesh[1]),
                "camera_facing": int(bool(pr.get("bCameraFacing", False)))}
    if cls == "ParticleModuleTypeDataDecal":
        d = raw_dist(pkg, pr.get("ProjectionDirection"), True) if "ProjectionDirection" in pr else None
        return {"kind": "decal", "dir": d or const_vector((0.0, 0.0, -1.0))}
    raise ParticleError(f"{name}: {cls} emitters can't be drawn")


def read_emitter(lib, pkg, idx, level=0):
    """Emitter `idx` at level of detail `level` (or the most detailed one under it
    that it has)."""
    epr = lib.props(pkg, idx)
    em = Emitter()
    em.name = str(epr.get("EmitterName", pkg.exports[idx - 1]["name"]))
    if epr.get("bEnabled") is False:
        return None
    lods = obj_array(epr.get("LODLevels"))
    lod0 = None
    best = -1
    for lod in lods:
        if lod <= 0:
            continue
        lv = int(lib.props(pkg, lod).get("Level", 0))
        if best < lv <= level:
            lod0, best = lod, lv
    if lod0 is None:
        raise ParticleError(f"{em.name}: no LOD {level}")
    lpr, blob = _lod_blob(pkg, lod0)
    if lpr.get("bEnabled") is False:
        return None
    em.peak = int(lpr.get("PeakActiveParticles", 0))
    td = lpr.get("TypeDataModule")
    if isinstance(td, tuple) and td[0] == "obj" and td[1]:
        em.typedata = _typedata(lib, pkg, td[1], em.name)
    req_ref = lpr["RequiredModule"][1]
    rpr = lib.props(pkg, req_ref)
    mat = rpr.get("Material")
    if mat and mat[1]:
        em.material = (pkg, mat[1])
        em.material_name = pkg.obj_name(mat[1])
    em.req = {
        "align": SCREEN_ALIGN.get(rpr.get("ScreenAlignment", "PSA_Square"), 0),
        "local": int(bool(rpr.get("bUseLocalSpace", False))),
        "duration": float(rpr.get("EmitterDuration", 1.0)),
        "durlow": float(rpr.get("EmitterDurationLow", 0.0)),
        "durrange": int(bool(rpr.get("bEmitterDurationUseRange", False))),
        "durrecalc": int(bool(rpr.get("bDurationRecalcEachLoop", False))),
        "loops": int(rpr.get("EmitterLoops", 0)),
        "delay": float(rpr.get("EmitterDelay", 0.0)),
        "delayfirst": int(bool(rpr.get("bDelayFirstLoopOnly", False))),
        "subh": int(rpr.get("SubImages_Horizontal", 1)),
        "subv": int(rpr.get("SubImages_Vertical", 1)),
        "interp": INTERP.get(rpr.get("InterpolationMethod", "PSUVIM_None"), 0),
        "randtime": float(rpr.get("RandomImageTime", 1.0)),
        "sort": SORT_MODES.get(rpr.get("SortMode", "ESM_None"), 0),
        "camoffset": float(rpr.get("CameraZOffset", 0.0)),
        "maxdraw": int(rpr.get("MaxDrawCount", 500)) if rpr.get("bUseMaxDrawCount", True) else 0,
        "probability": float(rpr.get("SpawnProbability", 1.0)),
        "scaleuv": int(bool(rpr.get("bScaleUV", False))),
        "lock": LOCK_AXIS.get(re.sub(r"_U$", "", str(epr.get("LockAxisFlags", "EPAL_NONE"))), 0),
        # Its particles go at once when the system is turned off (DeactivateSystem).
        "killdeact": int(bool(rpr.get("bKillOnDeactivate", False))),
    }
    # Its particles go at once when its loops are done (bKillOnCompleted). Kept
    # aside: written ("killdone=1") only for the systems that ask for it (see
    # build_weapon_fx.py's SYSTEMS), where the game's videos show it so.
    em.kill_on_completed = bool(rpr.get("bKillOnCompleted", False))
    # (An emitter with no SpawnRate of its own has the class default's, Engine's
    # Default__ParticleModuleRequired: a constant 0, spawning only its bursts. Taken
    # for 10 a second until now, which drew a burst-only emitter's particles all
    # through its duration: the Chaos-Rift Combustor's muzzle cap, one 0.17 s flash in
    # the game, lit up a whole second.)
    rate = raw_dist(pkg, rpr.get("SpawnRate"), False) if "SpawnRate" in rpr else None
    em.rate = rate if rate is not None else const_float(0.0)
    for b in struct_array(pkg, rpr.get("BurstList")) if "BurstList" in rpr else []:
        em.bursts.append((int(b.get("Count", 0)), int(b.get("CountLow", -1)), float(b.get("Time", 0.0))))

    recs, spawn, update, spawning, payload, names, floats, vectors = _parse_blob(blob)
    nf, nv = len(floats), len(vectors)
    for k, rec in enumerate(recs):
        f1 = recs[k + 1]["f0"] if k + 1 < len(recs) else nf
        v1 = recs[k + 1]["v0"] if k + 1 < len(recs) else nv
        kind = rec["type"]
        if kind == 0:
            obj = rec["obj"]
            cls = pkg.class_name(pkg.exports[obj - 1]) if obj > 0 else None
            if cls == "ParticleModuleAttractorPoint":
                em.modules.append(_attractor(lib, pkg, obj))
            elif cls == "ParticleModuleSpawnPerUnit":
                em.modules.append(_spawn_per_unit(lib, pkg, obj))
            elif cls == "ParticleModuleSwitchableColorScaleOverLife":
                em.modules.append(_switchable_color_scale(lib, pkg, obj))
            elif cls == "ParticleModuleSizeMultiplyLife" and _size_by_parameter(lib, pkg, obj):
                em.modules.append(_size_by_parameter(lib, pkg, obj))
            elif cls in ("ParticleModuleBeamSource", "ParticleModuleBeamTarget") and \
                    em.typedata and em.typedata["kind"] == "beam":
                # (A beam's ends: its type data holds them.)
                _beam_module(lib, pkg, obj, em.typedata)
                em.modules.append(Module(-1, [], [], []))
            elif cls and cls.startswith("ParticleModuleBeam") and lib.props(pkg, obj).get("bEnabled") is False:
                # (A beam modifier turned off: nothing to do.)
                em.modules.append(Module(-1, [], [], []))
            elif cls == "ParticleModuleBeamSineWave" and em.typedata and em.typedata["kind"] == "beam":
                _sine_waves(lib, pkg, obj, em.typedata)
                em.modules.append(Module(-1, [], [], []))
            else:
                em.modules.append(Module(-1, [], [], []))
                em.unsupported = getattr(em, "unsupported", []) + [str(cls)]
            continue
        spec = MODULES.get(kind)
        if spec is None:
            em.modules.append(Module(-1, [], [], []))
            em.unsupported = getattr(em, "unsupported", []) + [f"type {kind:#x}"]
            continue
        _name, fnames, vnames, nwords = spec
        fl, vl = floats[rec["f0"]:f1], vectors[rec["v0"]:v1]
        if len(fl) != len(fnames) or len(vl) != len(vnames):
            raise ParticleError(f"{em.name}: module {kind:#x} has {len(fl)}/{len(vl)} distributions")
        m = Module(kind, fl, vl, _words(payload, rec["payload"], nwords))
        if kind in NAMED:
            ni = m.words[0]
            if ni < len(names):
                m.name = pkg.names[names[ni][0]]
                if kind in (0x13, 0x14):
                    # (As the emitters are named in the effects file.)
                    m.name = m.name.replace(" ", "_")
        em.modules.append(m)
    if em.typedata and em.typedata["kind"] == "beam":
        # (A beam shaped by a module the demo hasn't got - connected to a skeleton's
        # bones, say - would be drawn as a plain line: left out.)
        beam = [c for c in getattr(em, "unsupported", []) if c.startswith("ParticleModuleBeam")]
        if beam:
            raise ParticleError(f"{em.name}: its beam's {', '.join(beam)} can't be drawn")
    em.spawn = [i for i in spawn if i < len(em.modules) and em.modules[i].kind >= 0]
    em.update = [i for i in update if i < len(em.modules) and em.modules[i].kind >= 0]
    em.spawning = [i for i in spawning if i < len(em.modules) and em.modules[i].kind >= 0]
    return em


def lod_distances(pkg, pr):
    """A system's LODDistances (cm), [0.0] when it has none."""
    v = pr.get("LODDistances")
    if not (isinstance(v, tuple) and v[0] == "array"):
        return [0.0]
    d = v[1]
    n = struct.unpack_from("<i", d, 0)[0]
    out = list(struct.unpack_from(f"<{n}f", d, 4)) if n > 0 and len(d) >= 4 + 4 * n else []
    return out or [0.0]


def read_system(lib, pkg, idx, level=0):
    """Particle system `idx` at level of detail `level`."""
    pr = lib.props(pkg, idx)
    s = System(pkg.exports[idx - 1]["name"])
    s.warmup = float(pr.get("WarmupTime", 0.0))
    s.lit = bool(pr.get("bLit", False))
    s.lod_distances = lod_distances(pkg, pr)
    s.level = level
    for e in obj_array(pr.get("Emitters")):
        if e <= 0:
            continue
        try:
            em = read_emitter(lib, pkg, e, level)
        except ParticleError as ex:
            # (A beam, say: the rest of the system is still drawn.)
            s.left_out.append(str(ex))
            continue
        if em is not None:
            s.emitters.append(em)
    return s


def emitter_lines(em, material_index):
    r = em.req
    out = [f"emitter {em.name.replace(' ', '_')}",
           f" material {material_index}",
           " required " + " ".join(f"{k}={_num(v) if isinstance(v, float) else v}" for k, v in r.items()),
           f" rate {em.rate.text()}"]
    if em.peak > 0:
        out.append(f" peak {em.peak}")
    td = em.typedata
    if td and td["kind"] == "trail":
        out.append(f" typedata trail tess={td['tess']} strength={_num(td['strength'])} tile={td['tile']} "
                   f"sheets={td['sheets']} max={td['max']} moving={td['moving']}")
    elif td and td["kind"] == "mesh":
        out.append(f" typedata mesh file={td['file']} camera={td['camera_facing']}")
    elif td and td["kind"] == "decal":
        out.append(f" typedata decal {td['dir'].text()}")
    elif td and td["kind"] == "beam":
        src, dst = td["source"], td["target"]
        out.append(f" typedata beam max={td['max']} tile={_num(td['tile'])} taper={td['taper']} "
                   f"source={','.join(_num(x) for x in src)} target={','.join(_num(x) for x in dst)}"
                   + (f" param={td['target_param']}" if td["target_param"] else ""))
        out.append(f" beamfactor {td['factor'].text()}")
        out.append(f" beamscale {td['scale'].text()}")
        for amp, period, speed, phase, d in td.get("waves", []):
            out.append(f" beamwave {_num(amp)} {_num(period)} {_num(speed)} {_num(phase)} "
                       f"{','.join(_num(x) for x in d)}")
    for count, low, time in em.bursts:
        if count > 0 or low > 0:
            out.append(f" burst {count} {low} {_num(time)}")
    for i, m in enumerate(em.modules):
        if m.kind >= 0:
            out += m.lines(i, i in em.spawn, i in em.update)
    out.append(" spawn " + " ".join(str(i) for i in em.spawn))
    out.append(" update " + " ".join(str(i) for i in em.update))
    if em.spawning:
        out.append(" spawning " + " ".join(str(i) for i in em.spawning))
    out.append("end")
    return out
