"""Writes assets/models/paint.txt: the values of the game's character materials,
which src/character.rs gives the painted parts (assets/shaders/paint.wgsl) so they
are shaded as the game shades them.

The characters' materials are instances of TR_BRDF_CustChar_TEXSET_MAT (and the
vehicles', enemies' and guns' of the same family, ENV_BRDF_A_MAT underneath): each
part's instance sets how glossy its texture set's gloss mask makes it and how bright
its colour is lit:

  Roughness Power         the highlight's power at full gloss (n = power * gloss)
  Roughness Offset        added to the gloss mask (Masks_NormY green) ...
  SmoothStep_Min, _Max    ... which smoothstep(min, max, mask + offset) makes the gloss
  DiffuseIntensityScalar  the colour's brightness ...
  DiffuseIntensityAdd     ... times (1 - the specular colour's luminance + this)
  CoefficientColor_C      the unpainted metal's reflection coefficient

Each line is a material's: the glTF models name their part materials after the
texture set (RB_Bumblebee_ArmLo_TEXSET), robot.glb and a few others after the
material instance (RB_Jazz_Legs_Mat_INST, RB_Soundwave_Wheels_Cust_Mat_INST), so
both names are written.

usage: python build_paint.py <CookedPC> <foc_esc/assets>
"""
import os
import sys

sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), "ue3"))
import fxmat  # noqa: E402
from fxmat import struct_array  # noqa: E402

# The packages with the characters' (and their vehicles', guns' and the enemies')
# material instances.
PACKAGES = ["MP_Array_BASE_m.xxx", "mp_shared.xxx", "all_shared.xxx", "ESC_Iacon_BASE_m.xxx",
            "ESC_Ark_BASE_m.xxx", "ESC_Temple_BASE_m.xxx", "ESC_Gladiator_BASE_m.xxx"]
# The base materials whose shading paint.wgsl copies (they share its functions:
# ENV_BRDF_MaterialSelector_MF, ENV_SpecPower_BRDF_MF, ENV_Reflection_BRDF_MF...).
BASES = ("ENV_BRDF_A_MAT", "ENV_BRDF_WEP_ResIn_MAT", "ENV_BRDF_Maksed_Defrag_MAT", "ENV_BRDF_Insecticon_B_MAT")
# The parameters' own defaults (their expressions' DefaultValues: ENV_SpecPower_BRDF_MF,
# ENV_Diffuse_BRDF_MF, ENV_DiffuseNormalization_BRDF_MF; a vector parameter's is 0).
SCALARS = [("Roughness Power", 90.0), ("Roughness Offset", 0.12), ("SmoothStep_Min", 0.0),
           ("SmoothStep_Max", 1.0), ("DiffuseIntensityScalar", 1.0), ("DiffuseIntensityAdd", 0.0)]
COEFFICIENT_C = "CoefficientColor_C"
# What an unknown material gets: TR_BRDF_CustChar_TEXSET_MAT's own values.
DEFAULT = "default 600 0.05 0 0.7 1 0 0.72 0.72 0.72"


def texture_set(lib, found):
    """The texture set a material instance (or its parents) samples, by name."""
    for _ in range(16):
        if not found:
            return None
        pkg, idx = found
        pr = lib.props(pkg, idx)
        for el in struct_array(pkg, pr.get("TextureSetParameterValues")):
            value = el.get("ParameterValue")
            if str(el.get("ParameterName")) == "TextureSet" and isinstance(value, tuple) and value[1]:
                return pkg.obj_name(value[1])
        parent = pr.get("Parent")
        if not parent or not parent[1]:
            return None
        found = lib.resolve_ref(pkg, parent[1], fxmat.MATERIAL_CLASSES)
    return None


def main(cooked, assets):
    files = [f for f in PACKAGES if os.path.exists(os.path.join(cooked, f))]
    lib = fxmat.Library(cooked, files)
    rows = {}
    for pkg in lib.pkgs:
        for i, e in enumerate(pkg.exports, 1):
            if pkg.class_name(e) != "MaterialInstanceConstant":
                continue
            name = e["name"]
            try:
                _, params = lib.material(name)
            except fxmat.CompileError:
                continue
            if params["chain"][-1] not in BASES:
                continue
            s = params["scalar"]
            values = [s.get(k, default) for k, default in SCALARS]
            coef = params["vector"].get(COEFFICIENT_C, (0.0, 0.0, 0.0, 0.0))[:3]
            row = " ".join("%g" % round(v, 5) for v in values + list(coef))
            texset = texture_set(lib, lib.find(name, fxmat.MATERIAL_CLASSES))
            for key in (name, texset):
                if key and key not in rows:
                    rows[key] = row
    lines = ["# The characters' material values (tools/build_paint.py): the game's character",
             "# materials' (TR_BRDF_CustChar_TEXSET_MAT and its family's) that",
             "# assets/shaders/paint.wgsl shades the painted parts with, by the glTF",
             "# material's name (a texture set's or a material instance's).",
             "# brdf <name> <roughness power> <roughness offset> <smoothstep min> <smoothstep max>",
             "#      <diffuse scalar> <diffuse add> <coefficient C r g b>",
             DEFAULT]
    lines += [f"brdf {k} {v}" for k, v in sorted(rows.items())]
    out = os.path.join(assets, "models", "paint.txt")
    with open(out, "w") as fh:
        fh.write("\n".join(lines) + "\n")
    print(f"wrote {out}: {len(rows)} materials")


if __name__ == "__main__":
    if len(sys.argv) != 3:
        print(__doc__)
        sys.exit(1)
    main(sys.argv[1], sys.argv[2])
