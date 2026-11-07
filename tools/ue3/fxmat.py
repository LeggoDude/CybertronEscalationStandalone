"""The game's effect materials, compiled to WGSL for the demo's effect shaders.

FoC's effects (light shafts, glow spheres, smoke cards, particles, lens flares) are
drawn with Unreal 3 materials: graphs of material expressions, cooked into the
packages with their graphs (and High Moon's material functions and per-vertex
outputs). This module reads a material - an instance (MaterialInstanceConstant) is
followed to its base material, its parameters overriding the base's - and writes
its graph as a WGSL function the demo's effect shaders call:

    fn <name>(i: FxIn) -> FxOut

FxIn (assets/shaders/levelfx_types.wgsl) holds what the graph reads: texture
coordinates, vertex (particle) colour, the particle's dynamic parameters, positions
and vectors in Unreal's space and units, depth, time, the lens flare's values;
FxOut what it gives: emissive and diffuse colour, opacity, the tangent-space
normal, and a see-through material's distortion (how far it shifts the picture
behind it: Compiled.distorts). Textures are sampled from the material's own slots
(fx_tex0..3).

    lib = Library(cooked_dir, packages)    # where materials are looked up
    mat = lib.compile(name, fn_name)       # -> Compiled (or raises CompileError)
    mat.wgsl, mat.textures, mat.cube, mat.blend, mat.lit, mat.two_sided, mat.uses_scene,
    mat.distorts, ...

Unreal's types are float1-4; values are WGSL f32 / vec2-4<f32>, with Unreal's
rules: a float1 meets any size, other sizes are cut to the smaller.
"""
import os
import re
import struct

from props import read_props
from ue3pkg import Package, Reader

# Blend modes the demo draws (UE3's BLEND_*).
BLENDS = ("BLEND_Opaque", "BLEND_Masked", "BLEND_Translucent", "BLEND_Additive", "BLEND_Modulate")
LIT_MODELS = ("MLM_Phong", "MLM_ParticleWrapTransmissive", "MLM_ParticleModulateTransmissive", "MLM_NonDirectional",
              "MLM_SHPRT", "MLM_Custom")

# Texture slots a material may use (fx_tex0..).
MAX_TEXTURES = 6

MATERIAL_CLASSES = ("Material", "MaterialInstanceConstant", "MaterialInstanceTimeVarying")


class CompileError(Exception):
    pass


def _clean(k):
    return all(c.isalnum() or c in "_[]" for c in k)


def props_of(pkg, idx):
    """An object's tagged properties (the longest clean parse near its start)."""
    e = pkg.exports[idx - 1]
    best = {}
    for st in (4, 0, 8, 12, 16):
        try:
            r = Reader(pkg.data, e["offset"] + st)
            pr = read_props(pkg, r, e["offset"] + e["size"])
        except Exception:
            continue
        if pr and all(_clean(k) for k in pr) and len(pr) > len(best):
            best = pr
    return best


def sub(pkg, v):
    """A struct property's own properties (or None)."""
    if isinstance(v, tuple) and v[0] == "struct":
        try:
            return read_props(pkg, Reader(v[2]), len(v[2]))
        except Exception:
            return None
    return None


def struct_array(pkg, v):
    if not (isinstance(v, tuple) and v[0] == "array") or len(v[1]) < 4:
        return []
    r = Reader(v[1])
    n = r.i32()
    out = []
    try:
        for _ in range(n):
            out.append(read_props(pkg, r, len(v[1])))
    except Exception:
        return []
    return out


def obj_array(v):
    if not (isinstance(v, tuple) and v[0] == "array") or len(v[1]) < 4:
        return []
    n = struct.unpack_from("<i", v[1])[0]
    return list(struct.unpack_from(f"<{n}i", v[1], 4))


def fstr(v):
    """A StrProperty's text."""
    if isinstance(v, tuple) and isinstance(v[1], (bytes, bytearray)):
        d = v[1]
        n = struct.unpack_from("<i", d)[0] if len(d) >= 4 else 0
        return d[4:4 + max(n, 0)].decode("latin-1").rstrip("\x00")
    return str(v)


# ---------------------------------------------------------------------------
# Finding materials and their parameters
# ---------------------------------------------------------------------------

class Library:
    """Materials, textures and material functions of a set of packages (the map's
    and the shared ones it loads), looked up by name."""

    def __init__(self, cooked, files):
        self.cooked = cooked
        self.pkgs = []
        self.by_name = {}
        for f in files:
            path = os.path.join(cooked, f)
            if not os.path.exists(path):
                continue
            pkg = Package(path)
            pkg.file = f
            pkg.children = {}
            for i, e in enumerate(pkg.exports, 1):
                pkg.children.setdefault(e["outer"], []).append(i)
            self.pkgs.append(pkg)
            for i, e in enumerate(pkg.exports, 1):
                self.by_name.setdefault(e["name"], []).append((pkg, i))
        self._props = {}

    def props(self, pkg, idx):
        key = (id(pkg), idx)
        if key not in self._props:
            self._props[key] = props_of(pkg, idx)
        return self._props[key]

    def find(self, name, classes):
        """The first export named `name` of one of `classes` (package order wins)."""
        for pkg, i in self.by_name.get(name, []):
            if pkg.class_name(pkg.exports[i - 1]) in classes:
                return pkg, i
        return None

    def find_full(self, full_name, classes):
        """The export with the full name `full_name` (its outers', dotted) of one of
        `classes`."""
        for pkg, i in self.by_name.get(full_name.split(".")[-1], []):
            if pkg.class_name(pkg.exports[i - 1]) in classes and pkg.full_name(i) == full_name:
                return pkg, i
        return None

    def resolve_ref(self, pkg, idx, classes):
        """An object reference (export or import) -> (package, export) holding it."""
        if idx > 0:
            return pkg, idx
        if idx < 0:
            return self.find(pkg.obj_name(idx), classes)
        return None

    def material(self, name):
        """Follows material `name` to its base material: (base (pkg, idx), params)
        where params holds the scalars, vectors, textures and static switches the
        instances set (the most derived wins) and the chain's names."""
        found = self.find(name, MATERIAL_CLASSES)
        if not found:
            raise CompileError(f"material {name} not found")
        params = {"scalar": {}, "vector": {}, "texture": {}, "textureset": {}, "switch": {}, "chain": [],
                  "mask": {}}
        pkg, idx = found
        seen = set()
        while True:
            e = pkg.exports[idx - 1]
            cls = pkg.class_name(e)
            params["chain"].append(e["name"])
            if cls == "Material":
                for sw, val in self.compiled_switches(cls, e["name"]).items():
                    params["switch"].setdefault(sw, val)
                return (pkg, idx), params
            if (pkg.file, idx) in seen:
                raise CompileError(f"material chain loops at {e['name']}")
            seen.add((pkg.file, idx))
            pr = self.props(pkg, idx)
            for key, kind in (("ScalarParameterValues", "scalar"), ("VectorParameterValues", "vector"),
                              ("TextureParameterValues", "texture"), ("TextureSetParameterValues", "textureset")):
                for el in struct_array(pkg, pr.get(key)):
                    pname, value = el.get("ParameterName"), el.get("ParameterValue")
                    if pname is None or value is None:
                        continue
                    if kind == "vector":
                        value = struct.unpack("<4f", value[2][:16])
                    elif kind in ("texture", "textureset"):
                        value = (pkg, value[1]) if isinstance(value, tuple) and value[1] else None
                    params[kind].setdefault(str(pname), value)
            data = pkg.data[e["offset"]:e["offset"] + e["size"]]
            for sw, val in _static_switches(pkg, data).items():
                params["switch"].setdefault(sw, val)
            # (The switches it was compiled with: its shader map's, where the
            # packages' shader caches have one.)
            for sw, val in self.compiled_switches(cls, e["name"]).items():
                params["switch"].setdefault(sw, val)
            parent = pr.get("Parent")
            if not parent or not parent[1]:
                raise CompileError(f"{e['name']} has no parent")
            nxt = self.resolve_ref(pkg, parent[1], MATERIAL_CLASSES)
            if not nxt:
                raise CompileError(f"parent {pkg.obj_name(parent[1])} of {e['name']} not found")
            pkg, idx = nxt

    def compiled_switches(self, cls, name):
        """The static switch values material `name` (class `cls`) was compiled with,
        from the packages' shader caches ({} if none has its shader map)."""
        if getattr(self, "_switches", None) is None:
            import shadercache
            self._switches = {}
            for pkg in self.pkgs:
                shadercache.shader_maps(pkg, self._switches)
        return self._switches.get((cls, name), {})

    def compile(self, name, fn_name, overrides=None):
        """Material `name` compiled to WGSL function `fn_name`; `overrides` ({"scalar":
        {name: value}, "vector": {name: (r, g, b, a)}}) sets parameters over what its
        instances set, as a material instance's runtime parameters would (the
        Diffraction Barrier's edge in each side's colour, say)."""
        base, params = self.material(name)
        for kind, values in (overrides or {}).items():
            params[kind].update(values)
        return Compiler(self, base, params, fn_name, name).run()


def _pin_index(pins, oi):
    """The place among a function's input or output pins of the one an expression's
    output index names: the pins carry ids (High Moon's material functions number
    them on from other blocks' ids: 8, 9, 10...), and the index is the pin's id;
    where no pin has that id, it is the pin's place."""
    for n, pin in enumerate(pins):
        if pin.get("Id") == oi:
            return n
    return oi


def _static_switches(pkg, data):
    """{switch name: chosen input (0: the first, "Enabled")} cooked into an
    instance: records of ParameterName, ParameterCategory, the input chosen, the
    inputs' names, bOverride and a GUID (see matparams.py)."""
    out = {}
    o = 0
    while o + 32 <= len(data):
        k, kn, cat, catn, value, labels, labn, over = struct.unpack_from("<iiiiiiii", data, o)
        if (0 < k < len(pkg.names) and kn == 0 and 0 <= cat < len(pkg.names) and catn == 0 and value in (0, 1)
                and 0 <= labels < len(pkg.names) and labn == 0 and over in (0, 1) and labels > 0):
            name = pkg.names[k]
            if _clean(name):
                out.setdefault(name, value)
                o += 32
                continue
        o += 4
    return out


# ---------------------------------------------------------------------------
# Values
# ---------------------------------------------------------------------------

def vtype(n):
    return "f32" if n == 1 else f"vec{n}<f32>"


class V:
    """A typed WGSL value: code and component count."""

    def __init__(self, code, n):
        self.code = code
        self.n = n

    def __repr__(self):
        return f"V({self.code}, {self.n})"


def fnum(x):
    x = float(x)
    if x != x or x in (float("inf"), float("-inf")):
        x = 0.0
    s = repr(x)
    if "e" in s or "E" in s:
        s = f"{x:.9g}"
        if "e" in s and "." not in s.split("e")[0]:
            m, e = s.split("e")
            s = f"{m}.0e{e}"
    if "." not in s and "e" not in s:
        s += ".0"
    return s


def const(vals):
    vals = [float(v) for v in vals]
    if len(vals) == 1:
        return V(fnum(vals[0]), 1)
    return V(f"vec{len(vals)}<f32>({', '.join(fnum(v) for v in vals)})", len(vals))


def swz(v, comps):
    """Components of v ("xyz" etc.); a float1 gives its value for each."""
    if v.n == 1:
        if len(comps) == 1:
            return V(v.code, 1)
        return V(f"vec{len(comps)}<f32>({v.code})", len(comps))
    comps = "".join(c if "xyzw".index(c) < v.n else "xyzw"[v.n - 1] for c in comps)
    if comps == "xyzw"[:v.n] and len(comps) == v.n:
        return v
    return V(f"({v.code}).{comps}", len(comps))


def to_n(v, n):
    """v as n components: a float1 spread, a longer value cut, a shorter one padded
    with zeros."""
    if v.n == n:
        return v
    if v.n == 1:
        return V(f"{vtype(n)}({v.code})", n)
    if v.n > n:
        return swz(v, "xyzw"[:n])
    pad = ", ".join(["0.0"] * (n - v.n))
    return V(f"{vtype(n)}({v.code}, {pad})", n)


def arith(a, b):
    """The size two operands meet at (Unreal: float1 with anything, else the same)."""
    if a.n == b.n:
        return a.n
    if a.n == 1:
        return b.n
    if b.n == 1:
        return a.n
    return min(a.n, b.n)


# ---------------------------------------------------------------------------
# HLSL snippets (MaterialExpressionShaderCode): each translated by hand, from the
# operands (%A..%F) to a value. HLSL cuts a vector to a float1 cast's first part.
# ---------------------------------------------------------------------------

def _x(v):
    return swz(v, "x").code


def _n(v, n):
    return to_n(v, n).code


def _safe_pow(base, exp, n):
    zero = "0.000001" if n == 1 else f"vec{n}<f32>(0.000001)"
    return f"pow(max(abs({base}), {zero}), {exp})"


def _mul_xy(a, b):
    n = arith(a, b)
    return swz(V(f"({to_n(a, n).code} * {to_n(b, n).code})", n), "xy")


def _mul3(a, b, c):
    n = arith(arith(a, b) and V("", arith(a, b)), c)
    return V(f"({to_n(a, n).code} * {to_n(b, n).code} * {to_n(c, n).code})", n)


def _nz(v):
    """A value's own size (float1 to 4)."""
    return v.code, v.n


def _comp(v, c):
    """Component c ("x".."w", or HLSL's "r".."a") of v."""
    c = {"r": "x", "g": "y", "b": "z", "a": "w"}.get(c, c)
    return swz(v, c).code


def _lerp3(a, b, c):
    return V(f"mix({_n(a, 3)}, {_n(b, 3)}, {_n(c, 3) if c.n > 1 else 'vec3<f32>(' + _x(c) + ')'})", 3)


# The environment materials' (ENV_BRDF_A_MAT's material functions).
BRDF_SNIPPETS = {
    # (MP_HealthPluses_MAT, the heal beam's crosses: a mask's fourth power, tinted
    # and brightened.)
    "half3(pow(%A,4) * %B * 25)":
        lambda a: V(f"(pow(max({_n(a['A'], 3)}, vec3<f32>(0.0)), vec3<f32>(4.0)) * {_n(a['B'], 3)} * 25.0)", 3),
    # (MP_HealDoubleDmg_MATINST: a colour's brightness, its parts summed over the
    # square root of three.)
    "half( saturate( dot( %A, normalize( half3(1,1,1)) ) ) )":
        lambda a: V(f"saturate(dot({_n(a['A'], 3)}, vec3<f32>(0.57735027)))", 1),
    # (FX_PickUps_MAT, MP_HealDoubleDmg_MATINST's parent: a mask's power, scaled.)
    "half(pow(%A,%B) * %D * %E)":
        lambda a: V(f"({_safe_pow(_x(a['A']), _x(a['B']), 1)} * {_x(a['D'])} * {_x(a['E'])})", 1),
    # (MP_HealDoubleDmg_MATINST: a colour times a mask.)
    "half3(%A* %B.xyz )":
        lambda a: V(f"({_n(a['A'], 3)} * {_n(a['B'], 3)})", 3),
    # (Tracer_ADD_MAT, the sentry's laser sight: faded in and out over the first and
    # last twentieth of its length.)
    "half1( saturate(%A.x*20)*saturate((1-%A.x)*20) )":
        lambda a: V(f"(saturate({_x(a['A'])} * 20.0) * saturate((1.0 - {_x(a['A'])}) * 20.0))", 1),
    # (MegatronHoverMelee_MAT, the hover slam's landing rings: High Moon's shader
    # file, unseen, giving the DepthBiasedAlpha's BiasScale - how deep a band around
    # where the rings' spheres meet the ground is lit. Taken as a constant 40 units:
    # a band about 20 cm deep.)
    "HoverSlamAbilityDepthRange.usf":
        lambda a: const([40.0]),
    "half(saturate( %A-.5) * 2.0)":
        lambda a: V(f"(saturate({_x(a['A'])} - 0.5) * 2.0)", 1),
    "half( SafePow(%A.z, 1/2.2) )":
        lambda a: V(f"pow(max({_comp(a['A'], 'z')}, 0.0), 1.0 / 2.2)", 1),
    "half3( lerp(%A, %B, %C ) )":
        lambda a: _lerp3(a["A"], a["B"], a["C"]),
    "half3( lerp(%A, %A*%B, %C ) )":
        lambda a: _lerp3(a["A"], V(f"({_n(a['A'], 3)} * {_n(a['B'], 3)})", 3), a["C"]),
    "half(max(%A, 1))":
        lambda a: V(f"max({_x(a['A'])}, 1.0)", 1),
    "half(smoothstep(%A,%B,%C))":
        lambda a: V(f"smoothstep({_x(a['A'])}, {_x(a['B'])}, {_x(a['C'])})", 1),
    "half(smoothstep(%B,%C,%A))":
        lambda a: V(f"smoothstep({_x(a['B'])}, {_x(a['C'])}, {_x(a['A'])})", 1),
    "half(%A-.05)":
        lambda a: V(f"({_x(a['A'])} - 0.05)", 1),
    "float(saturate(%A))":
        lambda a: V(f"saturate({_x(a['A'])})", 1),
    "half(saturate(dot(%A,%B)) )":
        lambda a: V(f"saturate({_dot(a['A'], a['B'])})", 1),
    "half(saturate(dot(%A,%B)) * %C + %D)":
        lambda a: V(f"(saturate({_dot(a['A'], a['B'])}) * {_x(a['C'])} + {_x(a['D'])})", 1),
    "half3(%A.x,%B,%A.z + %C)":
        lambda a: V(f"vec3<f32>({_comp(a['A'], 'x')}, {_x(a['B'])}, {_comp(a['A'], 'z')} + {_x(a['C'])})", 3),
    "half(1-%A)":
        lambda a: V(f"(1.0 - {_x(a['A'])})", 1),
    "half( saturate(length( half3(%A-%B) )/ %C)*.75 )":
        lambda a: V(f"(saturate(length({_n(a['A'], 3)} - {_n(a['B'], 3)}) / {_x(a['C'])}) * 0.75)", 1),
    "half( pow(1-%A,%B)* %C )":
        lambda a: V(f"(pow(max(1.0 - {_x(a['A'])}, 0.0), {_x(a['B'])}) * {_x(a['C'])})", 1),
    "half3(%B + min(min(%A.x, %A.y),%A.z))":
        lambda a: V(f"({_n(a['B'], 3)} + vec3<f32>(min(min({_comp(a['A'], 'x')}, {_comp(a['A'], 'y')}), "
                    f"{_comp(a['A'], 'z')})))", 3),
    "half(dot(%A,float3(.3,.59,.11)) )":
        lambda a: V(f"dot({_n(a['A'], 3)}, vec3<f32>(0.3, 0.59, 0.11))", 1),
    "half3(0.0f, %A.r,0.0f)":
        lambda a: V(f"vec3<f32>(0.0, {_comp(a['A'], 'x')}, 0.0)", 3),
    "half((-1/(1+ pow(1.497,((%A*%B)-15) ))) + 1 )":
        lambda a: V(f"(-1.0 / (1.0 + pow(1.497, {_x(a['A'])} * {_x(a['B'])} - 15.0)) + 1.0)", 1),
    "half(clamp((%A+1),0,1.5))":
        lambda a: V(f"clamp({_x(a['A'])} + 1.0, 0.0, 1.5)", 1),
    "half(pow(%A.x, 1/2.2) )":
        lambda a: V(f"pow(max({_comp(a['A'], 'x')}, 0.0), 1.0 / 2.2)", 1),
    "half3( %A.x, 0, lerp(%A.x, %A.z, %B) * %C )":
        lambda a: V(f"vec3<f32>({_comp(a['A'], 'x')}, 0.0, mix({_comp(a['A'], 'x')}, {_comp(a['A'], 'z')}, "
                    f"{_x(a['B'])}) * {_x(a['C'])})", 3),
    "half3(%A,0, %B)":
        lambda a: V(f"vec3<f32>({_x(a['A'])}, 0.0, {_x(a['B'])})", 3),
    "half(saturate(%A + (%F - %B - %C + (%A - 0.5) * %D) * %E * saturate(%A * (1.0 - %A))))":
        lambda a: V(f"saturate({_x(a['A'])} + ({_x(a['F'])} - {_x(a['B'])} - {_x(a['C'])} + ({_x(a['A'])} - 0.5)"
                    f" * {_x(a['D'])}) * {_x(a['E'])} * saturate({_x(a['A'])} * (1.0 - {_x(a['A'])})))", 1),
    "half4(%A,%B,%C, 1)":
        lambda a: _pack4(a),
    "half3(%A,%A,%A)":
        lambda a: V(f"vec3<f32>({_x(a['A'])})", 3),
    # SmokeThick_LIT_Mat's (the Leapers' and the Hover Slam's ground smash: its
    # GroundDust and its smoke trails).
    "half3(%A,%A,%B)":
        lambda a: V(f"vec3<f32>({_x(a['A'])}, {_x(a['A'])}, {_x(a['B'])})", 3),
    "half1( saturate(%A + (%F - %B - %C + (%A - 0.5f) * %D) * %E * saturate(%A * (1.0f - %A))) )":
        lambda a: SNIPPETS["half(saturate(%A + (%F - %B - %C + (%A - 0.5) * %D) * %E * saturate(%A * (1.0 - %A))))"](a),
    "half3(normalize(%A))":
        lambda a: V(f"fx_normalize3({_n(a['A'], 3)})", 3),
    "half3(pow(saturate(abs(dot(%A,%B))),%C) *%D *%E)":
        lambda a: V(f"(vec3<f32>(pow(saturate(abs({_dot(a['A'], a['B'])})), {_x(a['C'])})) * {_n(a['D'], 3)} * "
                    f"{_n(a['E'], 3)})", 3),
    "pow(saturate(abs(dot(normalize(%A),%B))),%C) *%D *%E*%F":
        lambda a: V(f"(pow(saturate(abs(dot(fx_normalize3({_n(a['A'], 3)}), {_n(a['B'], 3)}))), {_x(a['C'])}) * "
                    f"{_x(a['D'])} * {_x(a['E'])} * {_x(a['F'])})", 1),
    "((1/dot(%A,float3(0,0,1)))+%D)*%B*%C":
        lambda a: V(f"((1.0 / {_n(a['A'], 3)}.z + {_x(a['D'])}) * {_x(a['B'])} * {_x(a['C'])})", 1),
}


def _pack4(a):
    """HLSL's half4(A, B, C, 1): the operands' components in turn, then 1 (cut to
    four)."""
    parts = []
    for k in "ABC":
        v = a[k]
        parts += [swz(v, c).code for c in "xyzw"[:v.n]]
    parts = (parts + ["1.0"])[:4]
    while len(parts) < 4:
        parts.append("1.0")
    return V(f"vec4<f32>({', '.join(parts)})", 4)


SNIPPETS = {
    # (Distortions: DeathDecal_MAT's, only inside its texture's square, and
    # BlastSphere_Death_MAT's, refracting.)
    "half(saturate(sign(%A.x))*saturate(sign(%A.y))*saturate(sign(1-%A.x))*saturate(sign(1-%A.y)))":
        lambda a: V(f"(saturate(sign({_n(a['A'], 2)}.x)) * saturate(sign({_n(a['A'], 2)}.y))"
                    f" * saturate(sign(1.0 - {_n(a['A'], 2)}.x)) * saturate(sign(1.0 - {_n(a['A'], 2)}.y)))", 1),
    "float3( refract(%A, %B, %C) )":
        lambda a: V(f"refract({_n(a['A'], 3)}, {_n(a['B'], 3)}, {_x(a['C'])})", 3),
    "half1( saturate(%A-%B + 0.5*%C)/%C )":
        lambda a: V(f"(saturate({_x(a['A'])} - {_x(a['B'])} + 0.5 * {_x(a['C'])}) / {_x(a['C'])})", 1),
    "saturate(%A-%B + 0.5*%C)/%C":
        lambda a: V(f"(saturate({_x(a['A'])} - {_x(a['B'])} + 0.5 * {_x(a['C'])}) / {_x(a['C'])})", 1),
    "float1(saturate(%A-%B + 0.5*%C)/%C)":
        lambda a: V(f"(saturate({_x(a['A'])} - {_x(a['B'])} + 0.5 * {_x(a['C'])}) / {_x(a['C'])})", 1),
    "float3(frac(%A.xy),0)":
        lambda a: V(f"vec3<f32>(fract({_n(a['A'], 2)}), 0.0)", 3),
    # (Scanner_Beam_MAT, the Guard's scanner beam: a triangle wave, 0 at the
    # edges, 1 in the middle.)
    "float2(1-abs((%A*2)-1))":
        lambda a: V(f"(vec2<f32>(1.0) - abs({_n(a['A'], 2)} * 2.0 - vec2<f32>(1.0)))", 2),
    "half1( floor(%A.y*1.01)*.5 )":
        lambda a: V(f"(floor({_n(a['A'], 2)}.y * 1.01) * 0.5)", 1),
    "half1( (pow(saturate(%A.y),1.75)*.4)+%B )":
        lambda a: V(f"(pow(saturate({_n(a['A'], 2)}.y), 1.75) * 0.4 + {_x(a['B'])})", 1),
    "float2( (%A * .8) + .1 )":
        lambda a: V(f"({_n(a['A'], 2)} * 0.8 + 0.1)", 2),
    "float3(%A,%A,%A)":
        lambda a: V(f"vec3<f32>({_x(a['A'])})", 3),
    # (ShieldHealer_MAT, the Titan's drones' shield on him: a hot fall-off.)
    "float3(pow(%A,4) * %B * 15)":
        lambda a: V(f"(pow({_n(a['A'], 3)}, vec3<f32>(4.0)) * {_n(a['B'], 3)} * 15.0)", 3),
    "float3(%A,%B,%B)":
        lambda a: V(f"vec3<f32>({_x(a['A'])}, {_x(a['B'])}, {_x(a['B'])})", 3),
    # (Clouds_Anim_MAT, the smoke the Riot Cannon's round trails, and
    # Sparks_02_DB_MAT_INST: colours built from a mask and a fade.)
    "float3(%A,%A+((1-%A)*.05),%B)":
        lambda a: V(f"vec3<f32>({_x(a['A'])}, {_x(a['A'])} + (1.0 - {_x(a['A'])}) * 0.05, {_x(a['B'])})", 3),
    "float3(%A,%B,%B*%B)":
        lambda a: V(f"vec3<f32>({_x(a['A'])}, {_x(a['B'])}, {_x(a['B'])} * {_x(a['B'])})", 3),
    "float2(%A.x, %A.y -.1)":
        lambda a: V(f"({_n(a['A'], 2)} - vec2<f32>(0.0, 0.1))", 2),
    "half2( saturate((%A.x - %B)/%C) , saturate((%A.y - %B)/%C) )":
        lambda a: V(f"saturate(({_n(a['A'], 2)} - {_x(a['B'])}) / {_x(a['C'])})", 2),
    "sqrt(%A)":
        lambda a: V(f"sqrt(max({a['A'].code}, {_n(const([0.0]), a['A'].n)}))", a['A'].n),
    "half1( saturate(5*%A) )":
        lambda a: V(f"saturate(5.0 * {_x(a['A'])})", 1),
    "half1( saturate(%A*0.5 + 0.5) )":
        lambda a: V(f"saturate({_x(a['A'])} * 0.5 + 0.5)", 1),
    "float1( 0.5 + (%A.x-0.5)*lerp(1, 3, abs(0.5-%A.y)) )":
        lambda a: V(f"(0.5 + ({_n(a['A'], 2)}.x - 0.5) * mix(1.0, 3.0, abs(0.5 - {_n(a['A'], 2)}.y)))", 1),
    "half3( %A.x*(1-%B) + %B, %A.y*(1-%B) + %B, %A.z*(1-%B) + %B )":
        lambda a: V(f"({_n(a['A'], 3)} * (1.0 - {_x(a['B'])}) + {_x(a['B'])})", 3),
    "float3(%A*1.4,%B*.5,pow(%B*.25,2)*2)":
        lambda a: V(f"vec3<f32>({_x(a['A'])} * 1.4, {_x(a['B'])} * 0.5, pow({_x(a['B'])} * 0.25, 2.0) * 2.0)", 3),
    "half3(( SafePow(((%A*%B)+(%C*%D)),%E))+(%D*%F))":
        lambda a: V("(" + _safe_pow("(%s * %s + %s * %s)" % (_n(a['A'], 3), _x(a['B']), _n(a['C'], 3), _n(a['D'], 3)),
                                    "vec3<f32>(%s)" % _x(a['E']), 3)
                    + f" + {_n(a['D'], 3)} * {_x(a['F'])})", 3),
    "half3((%B+( SafePow(%A,%C) *%D *%E))+%F)":
        lambda a: V(f"({_n(a['B'], 3)} + {_safe_pow(_x(a['A']), _x(a['C']), 1)} * {_n(a['D'], 3)} * {_x(a['E'])}"
                    f" + {_n(a['F'], 3)})", 3),
    "half( SafePow((1-%A),%B)*%C)":
        lambda a: V(f"({_safe_pow(f'(1.0 - {_x(a[chr(65)])})', _x(a['B']), 1)} * {_x(a['C'])})", 1),
    "half(saturate(abs(dot(normalize(%A),%B))))":
        lambda a: V(f"saturate(abs(dot(fx_normalize3({_n(a['A'], 3)}), {_n(a['B'], 3)})))", 1),
    "half((%A*%B)+(%C*%D)+%E)":
        lambda a: V(f"({_x(a['A'])} * {_x(a['B'])} + {_x(a['C'])} * {_x(a['D'])} + {_x(a['E'])})", 1),
    "saturate(%A.x*20)":
        lambda a: V(f"saturate({_x(a['A'])} * 20.0)", 1),
    "saturate((%A.x*%A.x)+(%A.y*%A.y)-0.995)*250":
        lambda a: V(f"(saturate(dot({_n(a['A'], 2)}, {_n(a['A'], 2)}) - 0.995) * 250.0)", 1),
    "float3(saturate(float3((%B*.3)*(%B*.7)+%B*.1, %B*.2, %C*.99)))":
        lambda a: V(f"saturate(vec3<f32>(({_x(a['B'])} * 0.3) * ({_x(a['B'])} * 0.7) + {_x(a['B'])} * 0.1,"
                    f" {_x(a['B'])} * 0.2, {_x(a['C'])} * 0.99))", 3),
    "float(1-SafePow( (%A.x*2)-1,16))":
        lambda a: V(f"(1.0 - {_safe_pow(f'({_x(a[chr(65)])} * 2.0 - 1.0)', '16.0', 1)})", 1),
    "half(saturate(length(half2((%A*2)-1))))":
        lambda a: V(f"saturate(length({_n(a['A'], 2)} * 2.0 - 1.0))", 1),
    "half(SafePow(%A, %B) * %C )":
        lambda a: V(f"({_safe_pow(_x(a['A']), _x(a['B']), 1)} * {_x(a['C'])})", 1),
    "half(saturate((1- SafePow(saturate((%A-.498))*2 , 36)) * ( SafePow(%A,4)*1.5) + (%A/10)))":
        lambda a: V(f"saturate((1.0 - {_safe_pow(f'(saturate({_x(a[chr(65)])} - 0.498) * 2.0)', '36.0', 1)})"
                    f" * ({_safe_pow(_x(a['A']), '4.0', 1)} * 1.5) + {_x(a['A'])} / 10.0)", 1),
    "float1( SafePow(SafePow(saturate(1-length(%A) ),0.5),%C) )":
        lambda a: V(f"{_safe_pow(_safe_pow(f'saturate(1.0 - length({a[chr(65)].code}))', '0.5', 1), _x(a['C']), 1)}", 1),
    "float2(1/%A,1/%B)*floor(float2(%C*(1/%B), %C))":
        lambda a: V(f"(vec2<f32>(1.0 / {_x(a['A'])}, 1.0 / {_x(a['B'])}) * floor(vec2<f32>({_x(a['C'])} * (1.0 / {_x(a['B'])}),"
                    f" {_x(a['C'])})))", 2),
    "%A*%B*%C":
        lambda a: _mul3(a['A'], a['B'], a['C']),
    "float2((%A*%B).xy)":
        lambda a: _mul_xy(a['A'], a['B']),
    "float2((%A*2) - 1)":
        lambda a: V(f"({_n(a['A'], 2)} * 2.0 - 1.0)", 2),
    "float3(1/%A,1/%B,1/(%A*%B))":
        lambda a: V(f"vec3<f32>(1.0 / {_x(a['A'])}, 1.0 / {_x(a['B'])}, 1.0 / ({_x(a['A'])} * {_x(a['B'])}))", 3),
    "half( saturate(pow(2.72,- ( pow(%A,2 ) /.15 ) )-.002) )":
        lambda a: V(f"saturate(pow(2.72, -(pow(max({_x(a['A'])}, 0.0), 2.0) / 0.15)) - 0.002)", 1),
    # The EMP grenade's effects (tools/build_weapon_fx.py).
    "float2(%A.y,%A.x)":
        lambda a: V(f"{_n(a['A'], 2)}.yx", 2),
    "float1( saturate(%A*20) )":
        lambda a: V(f"saturate({_x(a['A'])} * 20.0)", 1),
    "float1( %A.x +( 2.5 * %B) )":
        lambda a: V(f"({_x(a['A'])} + 2.5 * {_x(a['B'])})", 1),
    "float3(%A*.7,%A*%A,%A*%A*%A)":
        lambda a: V(f"vec3<f32>({_x(a['A'])} * 0.7, {_x(a['A'])} * {_x(a['A'])},"
                    f" {_x(a['A'])} * {_x(a['A'])} * {_x(a['A'])})", 3),
    "float3(%A,%B,saturate(%B*%B) )":
        lambda a: V(f"vec3<f32>({_x(a['A'])}, {_x(a['B'])}, saturate({_x(a['B'])} * {_x(a['B'])}))", 3),
    "float2( %A.x , abs((%A.y*2)-1))":
        lambda a: V(f"vec2<f32>({_n(a['A'], 2)}.x, abs({_n(a['A'], 2)}.y * 2.0 - 1.0))", 2),
    "float2( %A.x, %A.y - (%B*2) )":
        lambda a: V(f"vec2<f32>({_n(a['A'], 2)}.x, {_n(a['A'], 2)}.y - {_x(a['B'])} * 2.0)", 2),
    "half3( ((%A+1)*1.5) )":
        lambda a: V(f"(({_n(a['A'], 3)} + 1.0) * 1.5)", 3),
    "float2( %A , %A * 0.5 )":
        lambda a: V(f"vec2<f32>({_x(a['A'])}, {_x(a['A'])} * 0.5)", 2),
    # (The shield's restore and break, Shield_Break_Mat: a gradient's coordinate
    # from a scalar sweep.)
    "float2( %A-0.5, %A-1 )":
        lambda a: V(f"vec2<f32>({_x(a['A'])} - 0.5, {_x(a['A'])} - 1.0)", 2),
    "half1( ceil(%A-0.498)-ceil(%A-0.501) )":
        lambda a: V(f"(ceil({_x(a['A'])} - 0.498) - ceil({_x(a['A'])} - 0.501))", 1),
    "float3( SafePow(abs(dot(%A,%B)),6) *5 * %C.x, SafePow(abs(dot(%A,%B)),6) *5 * %C.y,"
    " SafePow(abs(dot(%A,%B)),6) *5 * %C.z)":
        lambda a: V(f"({_safe_pow(_dot(a['A'], a['B']), '6.0', 1)} * 5.0 * {_n(a['C'], 3)})", 3),
    "float1( ( saturate(%A.x*20) ) * ( saturate( (1-%A.x)*15 ) ) )":
        lambda a: V(f"(saturate({_x(a['A'])} * 20.0) * saturate((1.0 - {_x(a['A'])}) * 15.0))", 1),
    # (The same unwrapped: Tracer_Sniper_AI_MOD_MAT, the snipers' tracers and
    # laser sights, faded in and out at the ends of the beam.)
    "( saturate(%A.x*20) ) * ( saturate( (1-%A.x)*15 ) )":
        lambda a: V(f"(saturate({_x(a['A'])} * 20.0) * saturate((1.0 - {_x(a['A'])}) * 15.0))", 1),
    "saturate( (1-%A)*%A*4 )":
        lambda a: V(f"saturate((1.0 - {a['A'].code}) * {a['A'].code} * 4.0)", a['A'].n),
    "pow(%A,2)":
        lambda a: V(f"({a['A'].code} * {a['A'].code})", a['A'].n),
    "clamp((1-%A)*%A*4,0,1)":
        lambda a: V(f"saturate((1.0 - {a['A'].code}) * {a['A'].code} * 4.0)", a['A'].n),
    "clamp((%A.x*1.5)-0.98,0,2)*clamp((1-%A.x)*20,0,1)":
        lambda a: V(f"(clamp({_x(a['A'])} * 1.5 - 0.98, 0.0, 2.0) * saturate((1.0 - {_x(a['A'])}) * 20.0))", 1),
    "half1( saturate(dot(%A,%B)*1.5) )":
        lambda a: V(f"saturate({_dot(a['A'], a['B'])} * 1.5)", 1),
    "float3( (1-%A) * saturate( 1-(saturate(1-%A -.9)*50*%B)) )":
        lambda a: V(f"vec3<f32>((1.0 - {_x(a['A'])}) * saturate(1.0 - saturate(1.0 - {_x(a['A'])} - 0.9) * 50.0"
                    f" * {_x(a['B'])}))", 3),
    "float3(%A.y,%A.y,%A.y)":
        lambda a: V(f"vec3<f32>({_n(a['A'], 2)}.y)", 3),
    "half3(pow((1 - %A) * 3, 2) * (%A + (%B.x * 3)) * 0.25 * %C , pow((1 - %A) * 3, 2) * (%A + (%B.y * 3)) * 0.25 * %C ,"
    " pow((1 - %A) * 3, 2) * (%A + (%B.z * 3)) * 0.25 * %C )":
        lambda a: V(f"(((1.0 - {_x(a['A'])}) * 3.0) * ((1.0 - {_x(a['A'])}) * 3.0) * ({_x(a['A'])} + {_n(a['B'], 3)} * 3.0)"
                    f" * 0.25 * {_x(a['C'])})", 3),
    "float3( ((-%B)*%A.x)+%B , ((-%B)*%A.y)+%B , ((-%B)*%A.z)+%B )":
        lambda a: V(f"(-{_x(a['B'])} * {_n(a['A'], 3)} + {_x(a['B'])})", 3),
    "float3( saturate( (1-%A)*%A*4 ) )":
        lambda a: V(f"saturate((1.0 - {_n(a['A'], 3)}) * {_n(a['A'], 3)} * 4.0)", 3),
    "float1( saturate(%A-%B + 0.5*%C)/%C )":
        lambda a: V(f"(saturate({_x(a['A'])} - {_x(a['B'])} + 0.5 * {_x(a['C'])}) / {_x(a['C'])})", 1),
    "float3( pow(%A,2) )":
        lambda a: V(f"({_n(a['A'], 3)} * {_n(a['A'], 3)})", 3),
    # The weapons added later (tools/build_weapon_fx.py): Glow_Add_AssRiffle_FX_MAT
    # (a round glow from the particle's UVs: the Energon Harvester's muzzle flash and
    # tracers), Explosion_MAT (a corner fade), Spark_MAT (a fade in and out over a
    # gradient), ElectroRing01_Mat, WEP_StandardRocket_MATINST (the rockets' rim
    # light: the Thermo Rocket Cannon's), Ram_model_MAT (a facing ratio: the Gear
    # Shredder's blade glow) and Water_Disturbance_MAT (the Slime Cannon's drops).
    "float(1-saturate(length(((%A*2)-1)) *1.5 ))":
        lambda a: V(f"(1.0 - saturate({_length(f'({a[chr(65)].code} * 2.0 - 1.0)', a['A'].n)} * 1.5))", 1),
    "float1( saturate(1-((%A.x+%A.y)*4) ) )":
        lambda a: V(f"saturate(1.0 - ({_n(a['A'], 2)}.x + {_n(a['A'], 2)}.y) * 4.0)", 1),
    "float1( saturate((%A.x*(1-%A.x))*4) )":
        lambda a: V(f"saturate({_comp(a['A'], 'x')} * (1.0 - {_comp(a['A'], 'x')}) * 4.0)", 1),
    "float3(%A,%A*%A,%B)":
        lambda a: V(f"vec3<f32>({_x(a['A'])}, {_x(a['A'])} * {_x(a['A'])}, {_x(a['B'])})", 3),
    "half3((pow(saturate(abs(dot(%A,%B))),%C)+%F) *%D *%E)":
        lambda a: V(f"((vec3<f32>(pow(saturate(abs({_dot(a['A'], a['B'])})), {_x(a['C'])})) + {_n(a['F'], 3)})"
                    f" * {_n(a['D'], 3)} * {_n(a['E'], 3)})", 3),
    "half1( saturate(pow(saturate(abs(dot(%A,%B))*%D),%C)) )":
        lambda a: V(f"saturate(pow(saturate(abs({_dot(a['A'], a['B'])}) * {_x(a['D'])}), {_x(a['C'])}))", 1),
    "half3(%A,%B, %C )":
        lambda a: V(f"vec3<f32>({_x(a['A'])}, {_x(a['B'])}, {_x(a['C'])})", 3),
    # Spark_MAT's fade across a spark's width, and Lightning_Anim_03_UP_MAT_INST's
    # flipbook cell (the lightning of the Pulsar Cannon's and the Gear Shredder's
    # blasts, the X18 Scrapmaker's muzzle flash and the EMP grenade's flight).
    "float1( saturate((%A.y*(1-%A.y))*10) )":
        lambda a: V(f"saturate({_n(a['A'], 2)}.y * (1.0 - {_n(a['A'], 2)}.y) * 10.0)", 1),
    "float2(1/%A,1/%B)*floor(float2(%C, %C*(1/%A)))":
        lambda a: V(f"(vec2<f32>(1.0 / {_x(a['A'])}, 1.0 / {_x(a['B'])}) * floor(vec2<f32>({_x(a['C'])},"
                    f" {_x(a['C'])} * (1.0 / {_x(a['A'])}))))", 2),
    # HealthRecharge_MAT (the Energon Harvester's heal sparkles): a mask sharpened
    # to its fourth power, times a colour, brightened 25 times.
    "pow(%A,4) * %B * 25":
        lambda a: _pow4_times(a["A"], a["B"]),
    # (The same as a half3: the Diffraction Barrier's hexagons, TR_Barrier_Hexigons_MAT.)
    "half3( pow(%A,4) * %B * 25 )":
        lambda a: _pow4_times(a["A"], a["B"]),
    # PROP_NEU_MineBall_MATINST and WEP_ClusterCannon_cans_MATINST (the Chaos-Rift
    # Combustor's bomb and its clusters, drawn by tools/build_weapon_fx.py): a unit
    # direction turned `%A` radians round from +X towards +Y, tipped up by 45 degrees.
    "half3(normalize(half3(cos(%A), sin(%A), 1)) )":
        lambda a: V(f"normalize(vec3<f32>(cos({_x(a['A'])}), sin({_x(a['A'])}), 1.0))", 3),
    # ...and their wrapped light: the dot product of two directions taken from -1..1
    # to 0..1, scaled a little, squared, times a colour.
    "half3(saturate(SafePow( ((dot(%A,%B)+1)*.5 )*1.127,2)) * %C)":
        lambda a: V(f"(vec3<f32>(saturate({_safe_pow(f'(({_dot(a[chr(65)], a[chr(66)])} + 1.0) * 0.5 * 1.127)', '2.0', 1)}))"
                    f" * {_n(a['C'], 3)})", 3),
    "half(abs(dot(%A,%B)))":
        lambda a: V(f"abs({_dot(a['A'], a['B'])})", 1),
    "half3(SafePow(%E, %C) * %D)":
        lambda a: V(f"({_safe_pow(_n(a['E'], 3), f'vec3<f32>({_x(a[chr(67)])})', 3)} * {_n(a['D'], 3)})", 3),
    "half3(%A * (%B + 8) / (8 * PI))":
        lambda a: V(f"({_n(a['A'], 3)} * ({_x(a['B'])} + 8.0) / (8.0 * 3.14159265))", 3),
    "half3( %C * %D * %E )":
        lambda a: V(f"({_n(a['C'], 3)} * {_n(a['D'], 3)} * {_n(a['E'], 3)})", 3),
    # Oblivion's vortex (ENV_RST_VortexClouds2_MAT, ENV_RST_VortexBlackSwril2_MAT): a
    # round fade from the middle of the UVs, and the UVs scaled about their middle.
    "saturate((1-saturate(length((%A*2)-1))) *3)":
        lambda a: V(f"saturate((1.0 - saturate({_length(f'({a[chr(65)].code} * 2.0 - 1.0)', a['A'].n)})) * 3.0)", 1),
    "float2((%A.x-0.5)*%B*2+0.5,(%A.y-0.5)*%B*2+0.5)":
        lambda a: V(f"vec2<f32>(({_comp(a['A'], 'x')} - 0.5) * {_x(a['B'])} * 2.0 + 0.5,"
                    f" ({_comp(a['A'], 'y')} - 0.5) * {_x(a['B'])} * 2.0 + 0.5)", 2),
    # The Diffraction Barrier's shield (TR_Barrier_Distortion_AA_MAT): its hexagons'
    # normal map flattened, x and y scaled by B.
    "float3(%A.x*%B, %A.y*%B, %A.z)":
        lambda a: V(f"vec3<f32>({_comp(a['A'], 'x')} * {_x(a['B'])}, {_comp(a['A'], 'y')} * {_x(a['B'])},"
                    f" {_comp(a['A'], 'z')})", 3),
    # The Cloak (tools/build_cloak.py; the demo draws these in paint.wgsl). Its
    # overlay (TR_Cloak_Overlay_Mat): the part's normal from its Normal_XY_Map's
    # alpha and green (the z as written, of their product), and its distance from
    # the joint over the overlay's radius...
    "half3(%A,%B, sqrt(1 - dot(%A,%B)))":
        lambda a: V(f"vec3<f32>({_x(a['A'])}, {_x(a['B'])}, sqrt(max(1.0 - {_x(a['A'])} * {_x(a['B'])}, 0.0)))", 3),
    # (The dead's dissolve, TR_DefragOverlay_AB_Mat, the same in full precision.)
    "float3(%A,%B, sqrt(1 - dot(%A,%B)))":
        lambda a: V(f"vec3<f32>({_x(a['A'])}, {_x(a['B'])}, sqrt(max(1.0 - {_x(a['A'])} * {_x(a['B'])}, 0.0)))", 3),
    "half( saturate(length( float3(%A-%B) )/ %C)*.75 )":
        lambda a: V(f"(saturate(length({_n(a['A'], 3)} - {_n(a['B'], 3)}) / {_x(a['C'])}) * 0.75)", 1),
    # ... and the cloak itself (TR_Cloak_MAT): its bubbles' normal flattened by the
    # distance and laid over a fine one, and its waves out from the joint as it runs
    # low (Fade_In).
    "half(1-saturate(%A*%B))":
        lambda a: V(f"(1.0 - saturate({_x(a['A'])} * {_x(a['B'])}))", 1),
    "half3(((%A.xy*.3*%C)+%B.xy),%B.z)":
        lambda a: V(f"vec3<f32>({_n(a['A'], 2)} * 0.3 * {_x(a['C'])} + {_n(a['B'], 2)}, {_comp(a['B'], 'z')})", 3),
    "float3(%A.xyz-%B.xyz)":
        lambda a: V(f"({_n(a['A'], 3)} - {_n(a['B'], 3)})", 3),
    "half( saturate(1-(length(%A)/%B))*%C )":
        lambda a: V(f"(saturate(1.0 - {_length(a['A'].code, a['A'].n)} / {_x(a['B'])}) * {_x(a['C'])})", 1),
    "half( %A*%A*%B )":
        lambda a: V(f"({_x(a['A'])} * {_x(a['A'])} * {_x(a['B'])})", 1),
}


def _pow4_times(a, b):
    """HLSL's pow(a, 4) * b * 25, at the larger of their sizes."""
    n = arith(a, b)
    exp = "4.0" if n == 1 else f"vec{n}<f32>(4.0)"
    return V(f"({_safe_pow(to_n(a, n).code, exp, n)} * {to_n(b, n).code} * 25.0)", n)


def _length(code, n):
    """HLSL's length() of a value of n components (a float's is its size)."""
    return f"abs({code})" if n == 1 else f"length({code})"


SNIPPETS.update(BRDF_SNIPPETS)


def _dot(a, b):
    n = min(arith(a, b), 4)
    if n == 1:
        return f"({_x(a)} * {_x(b)})"
    return f"dot({_n(a, n)}, {_n(b, n)})"


# ---------------------------------------------------------------------------
# The compiler
# ---------------------------------------------------------------------------

class Ctx:
    """Where an expression's inputs come from: the material, or a material
    function's body with its call's inputs."""

    def __init__(self, pkg, call=None, parent=None, inputs=None, block=None):
        self.pkg = pkg
        self.call = call
        self.parent = parent
        self.inputs = inputs or []
        self.block = block


class Compiler:
    def __init__(self, lib, base, params, fn_name, label):
        self.lib = lib
        self.pkg, self.mat = base
        self.params = params
        self.fn = fn_name
        self.label = label
        self.lines = []
        self.memo = {}
        self.n = 0
        self.textures = []      # [(texture pkg, export idx)] by slot
        self.cube = None        # (pkg, export idx) of the cube map it samples, if any
        self.flags = set()
        self.stage = "pixel"
        # Stage inputs (the vertex-stage material outputs override them).
        self.tc = ["i.uv0", "i.uv1"]
        self.vcolor = "i.color"
        self.dyn = "i.dparam"

    # -- bookkeeping --

    def var(self, v, hint=""):
        """Stores v in a new local; returns it as a value."""
        self.n += 1
        name = f"v{self.n}"
        self.lines.append(f"    let {name}: {vtype(v.n)} = {v.code};")
        return V(name, v.n)

    def props(self, ctx, idx):
        return self.lib.props(ctx.pkg, idx)

    def warn(self, msg):
        self.flags.add("warn:" + msg)

    # -- inputs --

    def input(self, ctx, pr, key):
        """The value an ExpressionInput property gives (None when unconnected)."""
        s = sub(ctx.pkg, pr.get(key)) if key in pr else None
        if not s:
            return None
        return self.input_struct(ctx, s)

    def input_struct(self, ctx, s):
        ex = s.get("Expression")
        if not (isinstance(ex, tuple) and ex[0] == "obj" and ex[1]):
            return None
        out_index = s.get("ExpressionOutputIndex", 0) or 0
        val = self.expr(ctx, ex[1], out_index)
        if val is None:
            return None
        if s.get("Mask"):
            comps = "".join(c for c, k in zip("xyzw", ("MaskR", "MaskG", "MaskB", "MaskA")) if s.get(k))
            if comps:
                val = swz(val, comps)
        return val

    def output_mask(self, ctx, idx, out_index, val):
        """An expression's output `out_index` (its Outputs entry's mask) of val."""
        pr = self.props(ctx, idx)
        outs = struct_array(ctx.pkg, pr.get("Outputs")) if "Outputs" in pr else []
        if out_index < len(outs):
            o = outs[out_index]
            if o.get("Mask"):
                comps = "".join(c for c, k in zip("xyzw", ("MaskR", "MaskG", "MaskB", "MaskA")) if o.get(k))
                if comps:
                    return swz(val, comps)
        return val

    # -- expressions --

    def expr(self, ctx, idx, out_index=0):
        key = (id(ctx), ctx.pkg.file, idx, out_index, self.stage, tuple(self.tc), self.vcolor, self.dyn)
        if key in self.memo:
            return self.memo[key]
        pkg = ctx.pkg
        if idx < 0:
            raise CompileError(f"expression import {pkg.obj_name(idx)}")
        e = pkg.exports[idx - 1]
        cls = pkg.class_name(e).replace("MaterialExpression", "")
        pr = self.props(ctx, idx)
        handler = getattr(self, "x_" + cls, None)
        if handler is None:
            raise CompileError(f"{self.label}: unsupported expression {cls}")
        val = handler(ctx, idx, pr, out_index)
        if val is None:
            self.memo[key] = None
            return None
        if cls not in ("Function", "IOBlock", "StaticSwitchParameter"):
            val = self.output_mask(ctx, idx, out_index, val)
        if not re.fullmatch(r"v\d+|i\.\w+|[-0-9.e]+", val.code):
            val = self.var(val)
        self.memo[key] = val
        return val

    def need(self, v, what, default):
        return v if v is not None else default

    # arithmetic
    def _bin(self, ctx, pr, op, da, db):
        a = self.input(ctx, pr, "A")
        b = self.input(ctx, pr, "B")
        if a is None:
            a = const([pr.get("ConstA", da)])
        if b is None:
            b = const([pr.get("ConstB", db)])
        n = arith(a, b)
        a, b = to_n(a, n), to_n(b, n)
        return V(f"({a.code} {op} {b.code})", n)

    def x_Add(self, ctx, idx, pr, oi):
        return self._bin(ctx, pr, "+", 0.0, 1.0)

    def x_Subtract(self, ctx, idx, pr, oi):
        return self._bin(ctx, pr, "-", 1.0, 1.0)

    def x_Multiply(self, ctx, idx, pr, oi):
        return self._bin(ctx, pr, "*", 0.0, 1.0)

    def x_Divide(self, ctx, idx, pr, oi):
        a = self.input(ctx, pr, "A") or const([pr.get("ConstA", 1.0)])
        b = self.input(ctx, pr, "B") or const([pr.get("ConstB", 2.0)])
        n = arith(a, b)
        a, b = to_n(a, n), to_n(b, n)
        return V(f"({a.code} / {b.code})", n)

    def x_Constant(self, ctx, idx, pr, oi):
        return const([pr.get("R", 0.0)])

    def x_Constant2Vector(self, ctx, idx, pr, oi):
        return const([pr.get("R", 0.0), pr.get("G", 0.0)])

    def x_Constant3Vector(self, ctx, idx, pr, oi):
        return const([pr.get("R", 0.0), pr.get("G", 0.0), pr.get("B", 0.0)])

    def x_Constant4Vector(self, ctx, idx, pr, oi):
        return const([pr.get("R", 0.0), pr.get("G", 0.0), pr.get("B", 0.0), pr.get("A", 0.0)])

    def x_ScalarParameter(self, ctx, idx, pr, oi):
        name = str(pr.get("ParameterName", ""))
        v = self.params["scalar"].get(name, pr.get("DefaultValue", 0.0))
        return const([v])

    def x_VectorParameter(self, ctx, idx, pr, oi):
        name = str(pr.get("ParameterName", ""))
        if name in self.params["vector"]:
            v = self.params["vector"][name]
        else:
            d = pr.get("DefaultValue")
            v = struct.unpack("<4f", d[2][:16]) if d else (0.0, 0.0, 0.0, 0.0)
        return const(v)

    def x_ComponentMask(self, ctx, idx, pr, oi):
        v = self.input(ctx, pr, "Input")
        if v is None:
            v = const([0.0])
        comps = "".join(c for c, k in zip("xyzw", "RGBA") if pr.get(k))
        if not comps:
            return v
        return swz(v, comps)

    def x_StaticComponentMaskParameter(self, ctx, idx, pr, oi):
        v = self.input(ctx, pr, "Input") or const([0.0])
        name = str(pr.get("ParameterName", ""))
        m = self.params["mask"].get(name)
        comps = "".join(c for c, k in zip("xyzw", "RGBA") if (m[k] if m else pr.get("Default" + k)))
        return swz(v, comps or "x")

    def x_AppendVector(self, ctx, idx, pr, oi):
        a = self.input(ctx, pr, "A") or const([0.0])
        b = self.input(ctx, pr, "B") or const([0.0])
        n = min(a.n + b.n, 4)
        if a.n >= 4:
            return a
        b = to_n(b, n - a.n) if n - a.n < b.n else b
        return V(f"{vtype(n)}({a.code}, {b.code})", n)

    def x_OneMinus(self, ctx, idx, pr, oi):
        v = self.input(ctx, pr, "Input") or const([0.0])
        return V(f"(1.0 - {v.code})", v.n)

    def x_Abs(self, ctx, idx, pr, oi):
        v = self.input(ctx, pr, "Input") or const([0.0])
        return V(f"abs({v.code})", v.n)

    def x_Floor(self, ctx, idx, pr, oi):
        v = self.input(ctx, pr, "Input") or const([0.0])
        return V(f"floor({v.code})", v.n)

    def x_Ceil(self, ctx, idx, pr, oi):
        v = self.input(ctx, pr, "Input") or const([0.0])
        return V(f"ceil({v.code})", v.n)

    def x_Frac(self, ctx, idx, pr, oi):
        v = self.input(ctx, pr, "Input") or const([0.0])
        return V(f"fract({v.code})", v.n)

    def x_SquareRoot(self, ctx, idx, pr, oi):
        v = self.input(ctx, pr, "Input") or const([0.0])
        return V(f"sqrt(max({v.code}, {to_n(const([0.0]), v.n).code}))", v.n)

    def x_Normalize(self, ctx, idx, pr, oi):
        v = self.input(ctx, pr, "VectorInput") or self.input(ctx, pr, "Input") or const([0.0, 0.0, 1.0])
        if v.n == 1:
            return const([1.0])
        return V(f"fx_normalize{v.n}({v.code})", v.n)

    def x_Sine(self, ctx, idx, pr, oi):
        v = self.input(ctx, pr, "Input") or const([0.0])
        period = pr.get("Period", 1.0)
        k = 2.0 * 3.141592653589793 / period if period > 0 else 1.0
        return V(f"sin({v.code} * {fnum(k)})", v.n)

    def x_Cosine(self, ctx, idx, pr, oi):
        v = self.input(ctx, pr, "Input") or const([0.0])
        period = pr.get("Period", 1.0)
        k = 2.0 * 3.141592653589793 / period if period > 0 else 1.0
        return V(f"cos({v.code} * {fnum(k)})", v.n)

    def x_Power(self, ctx, idx, pr, oi):
        b = self.input(ctx, pr, "Base") or const([0.0])
        e = self.input(ctx, pr, "Exponent") or const([pr.get("ConstExponent", 2.0)])
        e = to_n(e, b.n)
        return V(f"pow(max({b.code}, {to_n(const([0.0]), b.n).code}), {e.code})", b.n)

    def x_Clamp(self, ctx, idx, pr, oi):
        v = self.input(ctx, pr, "Input") or const([0.0])
        lo = self.input(ctx, pr, "Min") or const([pr.get("MinDefault", 0.0)])
        hi = self.input(ctx, pr, "Max") or const([pr.get("MaxDefault", 1.0)])
        return V(f"clamp({v.code}, {to_n(lo, v.n).code}, {to_n(hi, v.n).code})", v.n)

    def x_ConstantClamp(self, ctx, idx, pr, oi):
        v = self.input(ctx, pr, "Input") or const([0.0])
        lo, hi = pr.get("Min", 0.0), pr.get("Max", 1.0)
        return V(f"clamp({v.code}, {to_n(const([lo]), v.n).code}, {to_n(const([hi]), v.n).code})", v.n)

    def x_LinearInterpolate(self, ctx, idx, pr, oi):
        a = self.input(ctx, pr, "A") or const([pr.get("ConstA", 0.0)])
        b = self.input(ctx, pr, "B") or const([pr.get("ConstB", 1.0)])
        t = self.input(ctx, pr, "Alpha") or const([pr.get("ConstAlpha", 0.5)])
        n = arith(a, b)
        if t.n != 1:
            n = arith(V("", n), t)
        return V(f"mix({to_n(a, n).code}, {to_n(b, n).code}, {to_n(t, n).code})", n)

    def x_DotProduct(self, ctx, idx, pr, oi):
        a = self.input(ctx, pr, "A") or const([0.0])
        b = self.input(ctx, pr, "B") or const([0.0])
        n = arith(a, b)
        if n == 1:
            return V(f"({a.code} * {b.code})", 1)
        return V(f"dot({to_n(a, n).code}, {to_n(b, n).code})", 1)

    def x_CrossProduct(self, ctx, idx, pr, oi):
        a = to_n(self.input(ctx, pr, "A") or const([0.0]), 3)
        b = to_n(self.input(ctx, pr, "B") or const([0.0]), 3)
        return V(f"cross({a.code}, {b.code})", 3)

    def x_Desaturation(self, ctx, idx, pr, oi):
        v = self.input(ctx, pr, "Input") or const([0.0])
        p = self.input(ctx, pr, "Percent") or const([1.0])
        lum = pr.get("LuminanceFactors")
        lum = struct.unpack("<4f", lum[2][:16])[:3] if lum else (0.3, 0.59, 0.11)
        c = to_n(v, 3)
        g = V(f"dot({c.code}, {const(lum).code})", 1)
        return V(f"mix({c.code}, vec3<f32>({g.code}), {to_n(p, 3).code})", 3)

    def x_If(self, ctx, idx, pr, oi):
        a = self.input(ctx, pr, "A") or const([0.0])
        b = self.input(ctx, pr, "B") or const([0.0])
        gt = self.input(ctx, pr, "AGreaterThanB") or const([0.0])
        lt = self.input(ctx, pr, "ALessThanB") or const([0.0])
        eq = self.input(ctx, pr, "AEqualsB")
        n = max(gt.n, lt.n, eq.n if eq else 1)
        a1, b1 = swz(a, "x"), swz(b, "x")
        res = f"select({to_n(lt, n).code}, {to_n(gt, n).code}, {a1.code} > {b1.code})"
        if eq is not None:
            res = f"select({res}, {to_n(eq, n).code}, abs({a1.code} - {b1.code}) < 0.00001)"
        return V(res, n)

    # textures and coordinates
    def x_TextureCoordinate(self, ctx, idx, pr, oi):
        k = pr.get("CoordinateIndex", 0)
        base = V(self.tc[min(k, 1)], 2)
        ut, vt_ = pr.get("UTiling", 1.0), pr.get("VTiling", 1.0)
        utile = self.input(ctx, pr, "UTile")
        vtile = self.input(ctx, pr, "VTile")
        # (High Moon's UTile and VTile inputs stand in for UTiling and VTiling.)
        su = swz(utile, "x").code if utile else fnum(ut)
        sv = swz(vtile, "x").code if vtile else fnum(vt_)
        if su == "1.0" and sv == "1.0":
            return base
        return V(f"({base.code} * vec2<f32>({su}, {sv}))", 2)

    def x_Panner(self, ctx, idx, pr, oi):
        c = self.input(ctx, pr, "Coordinate") or V(self.tc[0], 2)
        t = self.input(ctx, pr, "Time") or V("i.time", 1)
        sx, sy = pr.get("SpeedX", 0.0), pr.get("SpeedY", 0.0)
        c = to_n(c, 2)
        if pr.get("bFractionalPart"):
            return V(f"({c.code} + fract({swz(t, 'x').code} * vec2<f32>({fnum(sx)}, {fnum(sy)})))", 2)
        return V(f"({c.code} + {swz(t, 'x').code} * vec2<f32>({fnum(sx)}, {fnum(sy)}))", 2)

    def x_Rotator(self, ctx, idx, pr, oi):
        c = to_n(self.input(ctx, pr, "Coordinate") or V(self.tc[0], 2), 2)
        t = self.input(ctx, pr, "Time") or V("i.time", 1)
        cx, cy = pr.get("CenterX", 0.5), pr.get("CenterY", 0.5)
        speed = pr.get("Speed", 0.25)
        return V(f"fx_rotate({c.code}, {swz(t, 'x').code} * {fnum(speed)}, vec2<f32>({fnum(cx)}, {fnum(cy)}))", 2)

    def texture_slot(self, ctx, ref):
        """The slot for texture object ref (pkg, idx)."""
        found = self.lib.resolve_ref(ref[0], ref[1], ("Texture2D", "TextureFlipBook", "TextureRenderTarget2D"))
        if not found:
            raise CompileError(f"{self.label}: texture {ref[0].obj_name(ref[1])} not found")
        key = (found[0].file, found[1])
        keys = [(p.file, i) for p, i in self.textures]
        if key in keys:
            return keys.index(key)
        if len(self.textures) >= MAX_TEXTURES:
            raise CompileError(f"{self.label}: more than {MAX_TEXTURES} textures")
        self.textures.append(found)
        return len(self.textures) - 1

    def _sample(self, ctx, pr, tex_ref, uv):
        slot = self.texture_slot(ctx, tex_ref)
        tpkg, tidx = self.textures[slot]
        tp = self.lib.props(tpkg, tidx)
        s = V(f"fx_sample{slot}({to_n(uv, 2).code})", 4)
        lo = [tp.get(k, 0.0) for k in ("UnpackMin", "UnpackMin[1]", "UnpackMin[2]", "UnpackMin[3]")]
        hi = [tp.get(k, 1.0) for k in ("UnpackMax", "UnpackMax[1]", "UnpackMax[2]", "UnpackMax[3]")]
        if lo != [0.0] * 4 or hi != [1.0] * 4:
            s = V(f"({s.code} * {const([h - l for h, l in zip(hi, lo)]).code} + {const(lo).code})", 4)
        return self.var(s)

    def x_TextureSample(self, ctx, idx, pr, oi):
        tex = pr.get("Texture")
        if not tex or not tex[1]:
            return const([0.0, 0.0, 0.0, 0.0])
        # (A cube map in a plain sample that has no 2D texture of its name to stand
        # in for it, its Coordinates a direction: the Whirlwind's faction trail's,
        # Faction_Whirlwind_MAT's ENV_Metals_CUBEMAP. Those that have one, as
        # Water_Disturbance_MAT's FluidCubemap_CLR, keep sampling it as before.)
        if self._is_cube(ctx.pkg, tex[1]) and not self.lib.resolve_ref(
                ctx.pkg, tex[1], ("Texture2D", "TextureFlipBook", "TextureRenderTarget2D")):
            return self.x_TextureSampleParameterCube(ctx, idx, pr, oi)
        uv = self.input(ctx, pr, "Coordinates") or V(self.tc[0], 2)
        return self._sample(ctx, pr, (ctx.pkg, tex[1]), uv)

    @staticmethod
    def _is_cube(pkg, ref):
        """Whether texture object ref is a cube map (its class, TextureCube: an
        export's, or what an import says it is)."""
        if ref > 0:
            return pkg.class_name(pkg.exports[ref - 1]) == "TextureCube"
        if ref < 0:
            return str(pkg.imports[-ref - 1]["class"]) == "TextureCube"
        return False

    def x_TextureSampleParameter2D(self, ctx, idx, pr, oi):
        name = str(pr.get("ParameterName", ""))
        tex = self.params["texture"].get(name)
        if tex is None:
            t = pr.get("Texture")
            tex = (ctx.pkg, t[1]) if t and t[1] else None
        if tex is None:
            return const([0.0, 0.0, 0.0, 0.0])
        uv = self.input(ctx, pr, "Coordinates") or V(self.tc[0], 2)
        return self._sample(ctx, pr, tex, uv)

    x_TextureSampleParameterFlipbook = x_TextureSampleParameter2D

    def x_FlipBookSample(self, ctx, idx, pr, oi):
        """A flip book's sample (MaterialExpressionFlipBookSample: Oblivion's animated
        energon signs). Its texture (TextureFlipBook) holds HorizontalImages x
        VerticalImages frames, shown FrameRate a second (4 unless it says),
        left to right and top to bottom (TFBM_UL_ROW), or a frame picked at random each
        time (TFBM_RANDOM). The game moves the UVs onto the frame from the CPU (the flip
        book's tick); here they are moved by the time."""
        tex = pr.get("Texture")
        if not tex or not tex[1]:
            return const([0.0, 0.0, 0.0, 0.0])
        found = self.lib.resolve_ref(ctx.pkg, tex[1], ("TextureFlipBook",))
        if not found:
            raise CompileError(f"{self.label}: flip book {ctx.pkg.obj_name(tex[1])} not found")
        tp = self.lib.props(*found)
        h, v = int(tp.get("HorizontalImages", 1)), int(tp.get("VerticalImages", 1))
        method = tp.get("FBMethod", "TFBM_UL_ROW")
        if method not in ("TFBM_UL_ROW", "TFBM_RANDOM"):
            raise CompileError(f"{self.label}: flip book order {method}")
        step = f"floor(i.time * {fnum(float(tp.get('FrameRate', 4.0)))})"
        if method == "TFBM_RANDOM":
            k = self.var(V(f"floor(fract(sin({step} * 12.9898) * 43758.5453) * {fnum(h * v)})", 1))
        else:
            k = self.var(V(f"({step} % {fnum(h * v)})", 1))
        uv = self.input(ctx, pr, "Coordinates") or V(self.tc[0], 2)
        cell = f"vec2<f32>({k.code} % {fnum(h)}, floor({k.code} / {fnum(h)}))"
        frame_uv = V(f"(({to_n(uv, 2).code} + {cell}) / vec2<f32>({fnum(h)}, {fnum(v)}))", 2)
        return self._sample(ctx, pr, (ctx.pkg, tex[1]), frame_uv)

    def x_TextureSampleParameterCube(self, ctx, idx, pr, oi):
        """A cube map's sample (Oblivion's windows, ENV_Refractive_Glass_Trans_Mat's
        refraction and reflection of ENV_Metals_CUBEMAP; a spent shell casing's
        reflection, WEP_shellcasings_CLR_Mat_FX's): the material's own cube map (one
        to a material: `cube`, which levelfx.py writes and levelfx.rs binds), in the
        direction its Coordinates give in Unreal's axes (the reflection vector without
        them). A second, different cube map in a material gets a mid grey, as the
        levels' captured ones average out (fx_cube_sample's own, where none is bound)."""
        name = str(pr.get("ParameterName", ""))
        tex = self.params["texture"].get(name) if name else None
        if tex is None:
            t = pr.get("Texture")
            tex = (ctx.pkg, t[1]) if t and t[1] else None
        if tex is None:
            return const([0.0, 0.0, 0.0, 0.0])
        found = self.lib.resolve_ref(tex[0], tex[1], ("TextureCube",))
        if not found:
            raise CompileError(f"{self.label}: cube map {tex[0].obj_name(tex[1])} not found")
        if self.cube is None:
            self.cube = found
        elif (self.cube[0].file, self.cube[1]) != (found[0].file, found[1]):
            self.warn(f"a second cube map, {found[0].exports[found[1] - 1]['name']}: mid grey")
            return const([0.5, 0.5, 0.5, 1.0])
        coords = self.input(ctx, pr, "Coordinates")
        if coords is None:
            self.flags.add("pixel_normal")
            coords = V("fx_reflect_ws(i)", 3)
        return self.var(V(f"fx_cube_sample({to_n(coords, 3).code})", 4))

    x_TextureSampleCube = x_TextureSampleParameterCube

    def x_ReflectionCubeMapSample(self, ctx, idx, pr, oi):
        """High Moon's reflection: the level's captured cube map (the nearest
        StaticSceneCaptureCubeMapActor's) in the pixel's reflection, blurred as the
        material's specular power says (RoughnessScale). The surface shader samples it
        (src/levelfx.rs's SurfaceMaterial): there it is 1, so the material's
        ReflectionColor is what the sample is multiplied by; an effect mesh of a map
        that reflects it samples the map's cube map here (fx_reflection_sample:
        Oblivion's tower window), and other effects take it as 1."""
        self.flags.add("reflection_cube")
        self.reflection_roughness_scale = float(pr.get("RoughnessScale", 0.0))
        return self.var(V("fx_reflection_sample(i)", 4))

    def x_TextureSetSampleParameter(self, ctx, idx, pr, oi):
        """High Moon's texture sets (TextureSet, made by a TextureSetBlueprint from
        source textures): ENV_MainTexSet_BluePrint packs Color (RGB) with the normal's
        X (A) in its Color_NormX texture and Masks (RGB) with the normal's Y (A) in
        Masks_NormY. The sampler's outputs: 0 Color, 1 Masks, 2 Normal (tangent
        space, Z made up from X and Y)."""
        name = str(pr.get("ParameterName", ""))
        ref = self.params["textureset"].get(name)
        if ref is None:
            d = pr.get("DefaultValue")
            ref = (ctx.pkg, d[1]) if d and d[1] else None
        if ref is None:
            return const([0.0, 0.0, 0.0, 0.0])
        found = self.lib.resolve_ref(ref[0], ref[1], ("TextureSet",))
        if not found:
            raise CompileError(f"{self.label}: texture set {ref[0].obj_name(ref[1])} not found")
        spkg, sidx = found
        sp = self.lib.props(spkg, sidx)
        inter = obj_array(sp.get("Intermediates"))
        if len(inter) < 2:
            raise CompileError(f"{self.label}: texture set {spkg.exports[sidx - 1]['name']} has no textures")
        uv = self.input(ctx, pr, "UVs") or V(self.tc[0], 2)
        key = ("texset", spkg.file, sidx, uv.code)
        if key not in self.memo:
            c = self._sample(ctx, pr, (spkg, inter[0]), uv)
            m = self._sample(ctx, pr, (spkg, inter[1]), uv)
            n = self.var(V(f"fx_texset_normal({c.code}.w, {m.code}.w)", 3))
            self.memo[key] = (c, m, n)
        c, m, n = self.memo[key]
        if oi == 0:
            return V(f"vec4<f32>({c.code}.xyz, 1.0)", 4)
        if oi == 1:
            return V(f"vec4<f32>({m.code}.xyz, 1.0)", 4)
        return n

    def x_ParticleSubUV(self, ctx, idx, pr, oi):
        tex = pr.get("Texture")
        if not tex or not tex[1]:
            return const([0.0, 0.0, 0.0, 0.0])
        a = self._sample(ctx, pr, (ctx.pkg, tex[1]), V(self.tc[0], 2))
        b = self._sample(ctx, pr, (ctx.pkg, tex[1]), V(self.tc[1], 2))
        return self.var(V(f"mix({a.code}, {b.code}, i.subuv_lerp)", 4))

    # vertex / particle data
    def x_VertexColor(self, ctx, idx, pr, oi):
        return V(self.vcolor, 4)

    def has_outputs(self, ctx, idx):
        pr = self.props(ctx, idx)
        return bool(struct_array(ctx.pkg, pr.get("Outputs"))) if "Outputs" in pr else False

    def x_MeshEmitterVertexColor(self, ctx, idx, pr, oi):
        return V("i.mesh_color", 4)

    def x_DynamicParameter(self, ctx, idx, pr, oi):
        return V(self.dyn, 4)

    def x_Time(self, ctx, idx, pr, oi):
        period = pr.get("Period", 0.0)
        if period and period > 0:
            return V(f"(i.time % {fnum(period)})", 1)
        return V("i.time", 1)

    def x_RealTime(self, ctx, idx, pr, oi):
        return V("i.time", 1)

    def x_BumpOffset(self, ctx, idx, pr, oi):
        """Unreal's parallax offset: Coordinate + CameraVector(tangent).xy *
        (HeightRatio * Height - ReferencePlane * HeightRatio)."""
        coord = self.input(ctx, pr, "Coordinate") or V(self.tc[0], 2)
        height = self.input(ctx, pr, "Height") or const([0.0])
        ratio = float(pr.get("HeightRatio", 0.05))
        plane = float(pr.get("ReferencePlane", 0.5))
        return V(f"({_n(coord, 2)} + i.camera_vector_ts.xy * ({fnum(ratio)} * {_x(height)}"
                 f" - {fnum(plane * ratio)}))", 2)

    def x_CameraVector(self, ctx, idx, pr, oi):
        if pr.get("CoordinateSpace", "CS_Tangent") == "CS_World":
            return V("i.camera_vector", 3)
        return V("i.camera_vector_ts", 3)

    def x_ReflectionVector(self, ctx, idx, pr, oi):
        self.flags.add("pixel_normal")
        if pr.get("CoordinateSpace", "CS_Tangent") == "CS_World":
            return V("fx_reflect_ws(i)", 3)
        return V("fx_reflect_ts(i)", 3)

    def x_Normal(self, ctx, idx, pr, oi):
        self.flags.add("pixel_normal")
        return V("i.pixel_normal", 3)

    x_PixelNormalWS = x_Normal

    def x_VertexNormalWS(self, ctx, idx, pr, oi):
        return V("i.normal", 3)

    def x_WorldPosition(self, ctx, idx, pr, oi):
        return V("i.world_pos", 3)

    def x_ObjectWorldPosition(self, ctx, idx, pr, oi):
        return V("i.object_pos", 3)

    def x_RandomSeed(self, ctx, idx, pr, oi):
        # (High Moon's per-primitive random number, 0 to 1: a different one for each
        # placed mesh or effect, here from where it is placed.)
        return V("fx_random_seed(i.object_pos)", 1)

    def x_ActorWorldPosition(self, ctx, idx, pr, oi):
        return V("i.object_pos", 3)

    def x_Transform(self, ctx, idx, pr, oi):
        v = to_n(self.input(ctx, pr, "Input") or const([0.0, 0.0, 1.0]), 3)
        src = pr.get("TransformSourceType", "TRANSFORMSOURCE_Tangent")
        dst = pr.get("TransformType", "TRANSFORM_World")
        if src.endswith("Tangent") and dst in ("TRANSFORM_World", "TRANSFORM_Local"):
            return V(f"fx_tangent_to_world(i, {v.code})", 3)
        if src.endswith(("World", "Local")) and dst == "TRANSFORM_Tangent":
            return V(f"fx_world_to_tangent(i, {v.code})", 3)
        if src.endswith("Tangent") and dst == "TRANSFORM_View":
            return V(f"fx_world_to_view(i, fx_tangent_to_world(i, {v.code}))", 3)
        if src.endswith(("World", "Local")) and dst == "TRANSFORM_View":
            return V(f"fx_world_to_view(i, {v.code})", 3)
        return v

    def x_Fresnel(self, ctx, idx, pr, oi):
        nrm = self.input(ctx, pr, "Normal")
        nrm = to_n(nrm, 3) if nrm is not None else const([0.0, 0.0, 1.0])
        e = self.input(ctx, pr, "Exp")
        e = swz(e, "x") if e is not None else const([pr.get("Exponent", 3.0)])
        return V(f"pow(max(1.0 - max(dot(fx_normalize3({nrm.code}), i.camera_vector_ts), 0.0), 0.0), {e.code})", 1)

    def x_ScreenPosition(self, ctx, idx, pr, oi):
        if pr.get("ScreenAlign"):
            return V("vec4<f32>(i.screen_uv, i.depth, i.depth)", 4)
        return V("i.clip", 4)

    def x_PixelDepth(self, ctx, idx, pr, oi):
        return V("i.depth", 1)

    def x_SceneDepth(self, ctx, idx, pr, oi):
        self.flags.add("depth")
        return V("i.scene_depth", 1)

    def x_DepthBiasedAlpha(self, ctx, idx, pr, oi):
        self.flags.add("depth")
        a = swz(self.input(ctx, pr, "Alpha") or const([1.0]), "x")
        b = swz(self.input(ctx, pr, "Bias") or const([0.5]), "x")
        s = self.input(ctx, pr, "BiasScaleInput")
        s = swz(s, "x") if s is not None else const([pr.get("BiasScale", 1.0)])
        if s.code == "1.0" and "BiasScaleInput" not in pr and "BiasScale" in pr:
            pass
        return V(f"fx_depth_biased_alpha(i, {a.code}, {b.code}, {s.code})", 1)

    def x_DepthBiasBlend(self, ctx, idx, pr, oi):
        self.flags.add("depth")
        v = self.input(ctx, pr, "RGB") or const([0.0, 0.0, 0.0])
        a = swz(self.input(ctx, pr, "Alpha") or const([1.0]), "x")
        return V(f"({v.code} * fx_depth_biased_alpha(i, {a.code}, 0.5, 1.0))", v.n)

    def x_SceneTexture(self, ctx, idx, pr, oi):
        # (High Moon's particle G-buffers, SceneTex_GBuffer1 and SceneTex_GBuffer2:
        # the explosions' lit smoke is drawn into them and read back from them, not
        # from the picture. The demo has no such buffers, so a read of one finds it
        # empty: fx_empty_gbuffer, no smoke's cover, and its depth as far as it goes.)
        kind = pr.get("SceneTextureType")
        if kind in ("SceneTex_GBuffer1", "SceneTex_GBuffer2"):
            self.flags.add("gbuffer")
            return V("fx_empty_gbuffer()", 4)
        self.flags.add("scene")
        c = self.input(ctx, pr, "Coordinates")
        uv = to_n(c, 2).code if c is not None else "i.screen_uv"
        return V(f"fx_scene_color({uv})", 4)

    def x_LensFlareIntensity(self, ctx, idx, pr, oi):
        return V("i.flare.x", 1)

    def x_LensFlareOcclusion(self, ctx, idx, pr, oi):
        return V("i.flare.y", 1)

    def x_LensFlareRadialDistance(self, ctx, idx, pr, oi):
        return V("i.flare.z", 1)

    def x_LensFlareRayDistance(self, ctx, idx, pr, oi):
        return V("i.flare.w", 1)

    def x_LensFlareSourceDistance(self, ctx, idx, pr, oi):
        return V("i.flare_src", 1)

    # switches, code, functions
    def x_StaticSwitchParameter(self, ctx, idx, pr, oi):
        name = str(pr.get("ParameterName", ""))
        default = pr.get("DefaultIndex", 1)
        if default is True:
            default = 1
        choice = self.params["switch"].get(name, default)
        banks = struct_array(ctx.pkg, pr.get("InputBanks")) if "InputBanks" in pr else []
        if banks:
            bank = banks[min(oi, len(banks) - 1)]
            inputs = struct_array(ctx.pkg, bank.get("Inputs"))
            if choice < len(inputs):
                v = self.input_struct(ctx, inputs[choice])
                return v if v is not None else const([0.0])
            return const([0.0])
        # (Unreal's own: A when on, B when off; DefaultValue says which.)
        val = self.params["switch"].get(name)
        on = (val == 0) if val is not None else bool(pr.get("DefaultValue", False))
        v = self.input(ctx, pr, "A" if on else "B")
        return v if v is not None else const([0.0])

    def x_StaticSwitch(self, ctx, idx, pr, oi):
        on = bool(pr.get("DefaultValue", False))
        v = self.input(ctx, pr, "A" if on else "B")
        return v if v is not None else const([0.0])

    def x_ShaderCode(self, ctx, idx, pr, oi):
        code = fstr(pr.get("ShaderCode")) if "ShaderCode" in pr else fstr(pr.get("Code"))
        code = " ".join(code.split())
        args = {k: self.input(ctx, pr, k) for k in "ABCDEF"}
        args = {k: (v if v is not None else const([0.0])) for k, v in args.items()}
        f = SNIPPETS.get(code)
        if f is None:
            raise CompileError(f"{self.label}: no translation for shader code {code!r}")
        return f(args)

    x_Custom = x_ShaderCode

    def x_Function(self, ctx, idx, pr, oi):
        """A material function call: its body compiled with the call's inputs."""
        fref = pr.get("Function")
        if not fref or not fref[1]:
            return const([0.0])
        found = self.lib.resolve_ref(ctx.pkg, fref[1], ("MaterialFunction",))
        if not found:
            raise CompileError(f"{self.label}: material function {ctx.pkg.obj_name(fref[1])} not found")
        fpkg, fidx = found
        fpr = self.lib.props(fpkg, fidx)
        inputs = struct_array(ctx.pkg, pr.get("Inputs")) if "Inputs" in pr else []
        key = ("fn", id(ctx), idx)
        fctx = self.memo.get(key)
        if fctx is None:
            fctx = Ctx(fpkg, call=idx, parent=ctx, inputs=inputs,
                       block=(fpr.get("InputBlock", (0, 0))[1], fpr.get("OutputBlock", (0, 0))[1]))
            self.memo[key] = fctx
        out_block = fctx.block[1]
        if not out_block:
            return const([0.0])
        opr = self.lib.props(fpkg, out_block)
        pins = struct_array(fpkg, opr.get("Pins")) if "Pins" in opr else []
        oi = _pin_index(pins, oi)
        if oi >= len(pins):
            return const([0.0])
        s = sub(fpkg, pins[oi].get("Input"))
        v = self.input_struct(fctx, s) if s else None
        return v if v is not None else const([0.0])

    def x_IOBlock(self, ctx, idx, pr, oi):
        """A function's inputs: the call's input `oi`, or the pin's own default."""
        if ctx.call is None or idx != ctx.block[0]:
            return const([0.0])
        pins = struct_array(ctx.pkg, pr.get("Pins")) if "Pins" in pr else []
        oi = _pin_index(pins, oi)
        if oi < len(ctx.inputs):
            v = self.input_struct(ctx.parent, ctx.inputs[oi])
            if v is not None:
                return v
        if oi < len(pins):
            s = sub(ctx.pkg, pins[oi].get("Input"))
            v = self.input_struct(ctx, s) if s else None
            if v is not None:
                return v
        return const([0.0])

    # -- the material --

    def output(self, mpr, key):
        s = sub(self.pkg, mpr.get(key)) if key in mpr else None
        if not s:
            return None
        return self.input_struct(Ctx(self.pkg), s)

    def run(self):
        mpr = self.lib.props(self.pkg, self.mat)
        self.root_ctx = Ctx(self.pkg)
        blend = mpr.get("BlendMode", "BLEND_Opaque")
        model = mpr.get("LightingModel", "MLM_Phong")
        lines_head = []
        # Vertex-stage outputs (High Moon's): texture coordinates, vertex colour and
        # dynamic parameters, worked out from the stage's own inputs first.
        self.stage = "vertex"
        tcv = self.output(mpr, "TextureCoordinates")
        vcv = self.output(mpr, "VertexColor")
        dpv = self.output(mpr, "DynamicParameter")
        self.stage = "pixel"
        if tcv is not None:
            t = self.var(to_n(tcv, 4))
            self.tc = [f"{t.code}.xy", f"{t.code}.zw"]
            self.flags.add("custom_uvs")
        if vcv is not None:
            c = self.var(to_n(vcv, 4))
            self.vcolor = c.code
        if dpv is not None:
            d = self.var(to_n(dpv, 4))
            self.dyn = d.code
        outs = {}
        nrm = self.output(mpr, "Normal")
        if nrm is not None:
            nv = self.var(to_n(nrm, 3))
            outs["normal"] = nv
            self.lines.append(f"    i.pixel_normal = fx_tangent_to_world(i, fx_normalize3({nv.code}));")
        for key, name, n in (("EmissiveColor", "emissive", 3), ("DiffuseColor", "diffuse", 3), ("Opacity", "opacity", 1),
                             ("OpacityMask", "opacity_mask", 1), ("TwoSidedLightingMask", "two_sided_mask", 1),
                             ("SpecularColor", "specular", 3), ("SpecularPower", "spec_power", 1),
                             ("Roughness", "spec_power", 1), ("ReflectionColor", "reflection", 3)):
            v = self.output(mpr, key)
            if v is not None:
                outs[name] = to_n(v, n)
        # Its distortion (heat haze, shock rings): how far it shifts the picture
        # behind it, in screen space, as UE3's distortion pass offsets the scene
        # (a see-through material's, when it uses it: bUsesDistortion).
        # (One whose distortion can't be translated is drawn without it.)
        if mpr.get("bUsesDistortion") and blend in ("BLEND_Translucent", "BLEND_Additive", "BLEND_Modulate"):
            kept = (list(self.lines), dict(self.memo), self.n, list(self.textures), self.cube, set(self.flags))
            try:
                v = self.output(mpr, "Distortion")
            except CompileError as ex:
                self.lines, self.memo, self.n, self.textures, self.cube, self.flags = kept
                self.warn(f"distortion left out: {ex}")
                v = None
            if v is not None:
                outs["distortion"] = to_n(v, 2)
                self.flags.add("distortion")
        body = [f"fn {self.fn}(input: FxIn) -> FxOut {{", "    var i = input;", "    var o = fx_out_default();"]
        body += self.lines
        for name in ("emissive", "diffuse", "opacity", "opacity_mask", "two_sided_mask", "specular", "spec_power",
                     "reflection", "distortion"):
            if name in outs:
                body.append(f"    o.{name} = {outs[name].code};")
        if "normal" in outs:
            body.append(f"    o.normal = fx_normalize3({outs['normal'].code});")
        body.append("    return o;")
        body.append("}")
        return Compiled(self, blend, model, mpr, "\n".join(body))


class Compiled:
    def __init__(self, c, blend, model, mpr, wgsl):
        self.name = c.label
        self.fn = c.fn
        self.wgsl = wgsl
        self.blend = blend
        self.lit = model in LIT_MODELS and blend != "BLEND_Additive" and blend != "BLEND_Modulate"
        self.model = model
        self.two_sided = bool(mpr.get("TwoSided"))
        self.textures = c.textures
        # The cube map its cube samples sample ((pkg, export idx), or None).
        self.cube = c.cube
        self.flags = c.flags
        self.uses_scene = "scene" in c.flags
        # It shifts the picture behind it (its Distortion: FxOut's distortion).
        self.distorts = "distortion" in c.flags
        self.uses_depth = "depth" in c.flags
        self.opacity_clip = mpr.get("OpacityMaskClipValue", 0.3333)
        self.chain = c.params["chain"]
        # (A material with a ReflectionCubeMapSample: how much its specular power
        # blurs the cube map's reflection.)
        self.cube_roughness = getattr(c, "reflection_roughness_scale", None)
        # High Moon's blur buffer materials write where to blur the picture (a
        # post-process pass), not colours: they can't be drawn as they are.
        self.blur_buffer = bool(mpr.get("bUsedWithBlurBuffer", False))
        # And those that draw what the particle G-buffers hold (bUsesGBuffer1: the
        # smoke's normals and cover, lit as a whole; LightSmokeFromGBuffer1_MAT, the
        # tank-killed enemies' explosions' FinalLitSmoke) have nothing to draw
        # without them. (Drawn reading the picture as those buffers, its 30 m sprites
        # turned whatever was dark behind them black for their 13 seconds.)
        self.draws_gbuffer = bool(mpr.get("bUsesGBuffer1", False))
