"""The parameters a FoC material instance sets, read from a map package: what the
texture-set materials' glow depends on.

FoC's environment materials are instances (MaterialInstanceConstant) of one base
material, ENV_AllShader_p's ENV_BRDF_A_MAT, through a chain of parents (a set's own,
a family's: KON_BRDF_Base_MATINST, RST_BRDF_Base_MATINST, ...). Its glow is its
ENV_Emissive_MF function: the texture set's Masks red channel (the glow mask) x
EmissiveIntensity x the glow's colour, which is the material's diffuse colour, or its
Emissive_Color parameter when the static switch Emissive_UseColorParam is on. A
parameter an instance doesn't set comes from its parent, and from the base
material's default at the end of the chain (EmissiveIntensity's is 0: no glow).

A parent needn't be in the map's packages: the game loads its shared packages
(SHARED) with every Escalation map, and some families' instances are only there.
Downfall's IAC_BRDF_Base_MATINST, the parent of its Street_Light_MoreSpec_MATINST
(the plating of the upper floors round the map room), its TrainTrack_noemi_MATINST
and others, Oblivion's FloorRamp_MAT_INST, Ancients' rustsigns_MATINST and
Ignition's TrainTrack_orange_MATINST, is all_shared's (ENV_IAC_Shader_p.Material in
the map's imports); RB_MetroplexArmLo_0emi_MatInst's Iconic_Cars_TEXSET_Mat and
TR_BRDF_CustChar_TEXSET_MAT too. map_params reads them, under the map's own.

    material_params(pkg) -> {instance name: Params}
    map_params(packages, cooked) -> the map's packages' instances (the first
                            package's winning) over the shared packages', or the
                            map's alone when `cooked` is None
    glow(params, name)   -> (intensity, colour or None for the diffuse colour), or None
                            when the material isn't one of ENV_BRDF_A_MAT's
    brdf(params, name)   -> its shading values (BRDF_SCALARS and "coefficient",
                            CoefficientColor_C), or None likewise
    pickup_glow(params, name) -> (intensity, colour) of a pickup material's instance
                            (the purchase points' bases), or None

Static switches are cooked into an instance as records after its properties:
ParameterName, ParameterCategory, the chosen input (0: the first, "Enabled"; 1:
"Disabled"), the inputs' names, bOverride and a GUID. The most derived instance that
has a record decides.
"""
import os
import struct
from collections import namedtuple

from layout import props_any, read_struct_array
from ue3pkg import Package

Params = namedtuple("Params", "parent scalars vectors switches")

# The base material whose glow glow() models, and its parameters' defaults.
BASE = "ENV_BRDF_A_MAT"
DEFAULT_INTENSITY = 0.0
DEFAULT_COLOR = (0.875, 0.247, 0.128)
SWITCHES = ("Emissive_UseColorParam", "Enable_EmissiveAlpha_Smoothing", "Use Character Emissive Parameters",
            "Enable_Emissive_Pulse")


def _switches(pkg, data):
    """{switch name: chosen input index} from an instance's cooked static parameters."""
    names = {n: i for i, n in enumerate(pkg.names)}
    out = {}
    for sw in SWITCHES:
        k = names.get(sw)
        if k is None:
            continue
        pat = struct.pack("<ii", k, 0)
        at = data.find(pat)
        while at >= 0:
            if at + 44 <= len(data):
                cat, cat_n, value, labels, labels_n, override = struct.unpack_from("<iiiiii", data, at + 8)
                if (0 <= cat < len(pkg.names) and cat_n == 0 and value in (0, 1) and 0 <= labels < len(pkg.names)
                        and labels_n == 0 and override in (0, 1)):
                    out[sw] = value
                    break
            at = data.find(pat, at + 4)
    return out


def material_params(pkg):
    out = {}
    for e in pkg.exports:
        if pkg.class_name(e) != "MaterialInstanceConstant":
            continue
        try:
            pr, _ = props_any(pkg, e, (4, 0, 8))
        except ValueError:
            continue
        parent = pkg.obj_name(pr["Parent"][1]) if "Parent" in pr and pr["Parent"][1] else None
        scalars, vectors = {}, {}
        for key, dest in (("ScalarParameterValues", scalars), ("VectorParameterValues", vectors)):
            if key not in pr:
                continue
            try:
                for el in read_struct_array(pkg, pr[key][1]):
                    name, value = el.get("ParameterName"), el.get("ParameterValue")
                    if name is None or value is None:
                        continue
                    if isinstance(value, tuple) and value and value[0] == "struct":
                        value = struct.unpack("<4f", value[2][:16])
                    dest[str(name)] = value
            except Exception:
                pass
        data = pkg.data[e["offset"]:e["offset"] + e["size"]]
        out[e["name"]] = Params(parent, scalars, vectors, _switches(pkg, data))
    return out


# The packages the game loads with every Escalation map, whose material instances
# the maps' own may take as parents (tools/levelfx.py reads their effects too).
SHARED = ("mp_shared.xxx", "all_shared.xxx")


def map_params(packages, cooked=None):
    """{instance name: Params} of a map: its packages' instances (`packages`, Package
    objects, the first one's winning), over those of the shared packages (SHARED)
    in `cooked` (the game's CookedPC folder; None: the map's alone)."""
    out = {}
    for name in SHARED if cooked else ():
        path = os.path.join(cooked, name)
        if os.path.exists(path):
            out.update(material_params(Package(path)))
    for pkg in reversed(list(packages)):
        out.update(material_params(pkg))
    return out


def _chain(params, name):
    seen = []
    while name in params and name not in seen:
        seen.append(name)
        name = params[name].parent
    return seen, name


def glow(params, name):
    """(EmissiveIntensity, glow colour (linear RGB) or None: the diffuse colour) for
    instance `name` of ENV_BRDF_A_MAT, or None for any other material."""
    chain, root = _chain(params, name)
    if not chain or root != BASE:
        return None
    intensity, color, switches = None, None, {}
    for n in chain:
        p = params[n]
        if intensity is None and "EmissiveIntensity" in p.scalars:
            intensity = float(p.scalars["EmissiveIntensity"])
        if color is None and "Emissive_Color" in p.vectors:
            color = tuple(float(c) for c in p.vectors["Emissive_Color"][:3])
        for k, v in p.switches.items():
            switches.setdefault(k, v)
    intensity = DEFAULT_INTENSITY if intensity is None else intensity
    use_color = switches.get("Emissive_UseColorParam", 1) == 0
    return intensity, ((color or DEFAULT_COLOR) if use_color else None)


# The pickups' materials (WEP_Shared_p's Pickup_Unlit_MAT, the weapons' pickups', and
# the Escalation purchase points' bases on it through MP_PickupBase_Ammo_MATINST and
# MP_PowerUp_XP_MATINST): their glow areas lit in Emissive_Color at
# EmissiveIntensity.
PICKUP_ROOTS = ("Pickup_Unlit_MAT", "MP_PickupBase_Ammo_MATINST", "MP_PowerUp_XP_MATINST")


def pickup_glow(params, name):
    """(EmissiveIntensity, Emissive_Color (linear RGB)) of instance `name` of the
    pickups' material (PICKUP_ROOTS), each from the most derived instance that sets
    it, or None for any other material (or one that sets neither)."""
    chain, root = _chain(params, name)
    if not chain or not (root in PICKUP_ROOTS or any(n in PICKUP_ROOTS for n in chain)):
        return None
    intensity, color = None, None
    for n in chain:
        p = params[n]
        if intensity is None and "EmissiveIntensity" in p.scalars:
            intensity = float(p.scalars["EmissiveIntensity"])
        if color is None and "Emissive_Color" in p.vectors:
            color = tuple(float(c) for c in p.vectors["Emissive_Color"][:3])
    if intensity is None or color is None:
        return None
    return intensity, color


# ENV_BRDF_A_MAT's shading values and their defaults (its parameters' DefaultValues,
# as tools/build_paint.py has them; a vector parameter's is 0; MetallicOffset's 0 and
# NewReflectionIntensity's 1, the characters' own).
BRDF_SCALARS = (("Roughness Power", 90.0), ("Roughness Offset", 0.12), ("SmoothStep_Min", 0.0),
                ("SmoothStep_Max", 1.0), ("DiffuseIntensityScalar", 1.0), ("DiffuseIntensityAdd", 0.0),
                ("MetallicOffset", 0.0), ("NewReflectionIntensity", 1.0))
BRDF_COEFFICIENT = "CoefficientColor_C"


def brdf(params, name):
    """Instance `name` of ENV_BRDF_A_MAT's shading values ({scalar name: value, and
    "coefficient": CoefficientColor_C's RGB}), each from the most derived instance
    that sets it, or None for any other material."""
    chain, root = _chain(params, name)
    if not chain or root != BASE:
        return None
    out = {}
    for n in chain:
        p = params[n]
        for key, _ in BRDF_SCALARS:
            if key not in out and key in p.scalars:
                out[key] = float(p.scalars[key])
        if "coefficient" not in out and BRDF_COEFFICIENT in p.vectors:
            out["coefficient"] = tuple(float(c) for c in p.vectors[BRDF_COEFFICIENT][:3])
    for key, default in BRDF_SCALARS:
        out.setdefault(key, default)
    out.setdefault("coefficient", (0.0, 0.0, 0.0))
    return out

