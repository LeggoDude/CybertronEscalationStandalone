"""The Cloak's look (src/cloak.rs, assets/shaders/paint.wgsl) and the dead's dissolve
(src/enemies.rs): the textures of the game's two cloak materials and its dissolve
overlays, and a check that their values are still the ones the demo draws them
with.

The game's TnCloakMaterialOverrider (TnCharacterMaterialBlueprint's Cloak* values,
SharedCharacterMaterials) cloaks a Jazz in two materials:

  * TR_Cloak_Overlay_MATINST (TR_Cloak_Overlay_Mat: masked, unlit), drawn over him
    for CloakActivateDuration (1.5 s) by his HmMeshHighlightComponent as its
    DefragValue goes from 0 to 1: a ragged front out from his elbow's joint
    (fractalMaps panned at MaterialTiling 3), lit in Defrag Lead Color where it
    passes (pulseGradien_r01) with an orange Fresnel rim (Fresnel Color, its
    Exponent and Intensity), burning out (Burn Out Color, WEP_Defrag_Alphas) and
    gone (pulseGradien_r02) behind it;
  * TR_Cloak_MATINST (TR_Cloak_MAT: translucent, unlit, distorting), which his
    meshes are put on halfway through: only the picture behind him shifted by its
    normal (bubbles over DiffClouds_CLR's drift, on Pixel_4_NORM_DXT1), and, as his
    meter runs low, waves out from the joint (Fade Out Wave Color, Radial Falloff
    Intensity) as its Fade_In rises.

A dead robot dissolves in a third (TnDeathActionHighlight's TnDeresser, its
OverlayMaterial drawn over it by its HmMeshHighlightComponent as its DefragValue goes
from 0 to 1, the robot itself drawn no more past NormalizedTimeToMakeMeshInvisible):
TR_DefragOverlay_MATINST for nearly every death (TR_DefragOverlay_AB_Mat: the cloak's
overlay, out from the middle of the body, with a cyan rim and, where it holds, the
lines of IaconMask_NORM's alpha glowing in its lead colour; its front's glow growing
as it comes), its electrocution, corrosion, fire and Retro pistol variants in their
own colours (src/enemies.rs's DEFRAG_LOOKS; the electrocution's noise is
ElectricArcs).

paint.wgsl draws them all as the materials' graphs do (tools/ue3/fxmat.py translates
them; this prints them with -v). This writes their textures into
<assets>/textures/cloak/:

  WEP_Defrag_Alphas.ktx2   colour (sRGB), as the game samples it
  DiffClouds_CLR.ktx2      colour (sRGB)
  bubbles.ktx2             normal map (x and y; z worked out)
  Pixel_4_NORM_DXT1.ktx2   normal map
  pulseGradients.png       the overlay's two gradients, 512 x 1: pulseGradien_r01 in
                           red (0 up to 468/512, then rising to 1), pulseGradien_r02
                           in green (0 below half, 1 above), sRGB as the game samples
                           them
  IaconMask_NORM.ktx2      the dissolve's lines: its alpha (linear), grey
  ElectricArcs.ktx2        the electrocution's noise, colour (sRGB)

usage: python build_cloak.py <TransGame\\CookedPC> <foc_esc\\assets> [-v]
"""
import os
import sys

import numpy as np
from PIL import Image

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.path.join(HERE, "ue3"))
sys.path.insert(0, HERE)
import fxmat  # noqa: E402
import ktx2_io  # noqa: E402
import texture  # noqa: E402

PACKAGES = ["MP_Array_BASE_m.xxx", "all_shared.xxx", "TransGame.xxx", "ESC_Ark_BASE_m.xxx",
            "ESC_Gladiator_BASE_m.xxx"]
OVERLAY = "TR_Cloak_Overlay_MATINST"
CLOAK = "TR_Cloak_MATINST"
# The dissolve's overlays (src/enemies.rs's DEFRAG_LOOKS).
DEFRAG = "TR_DefragOverlay_MATINST"
ELECTRICAL = "TR_ElectricalDefragOverlay_MATINST"
CORROSION = "TR_CorrosionOverlay_MATINST"
MOLTEN = "TR_MoltenOverlay_MATINST"
RETRO = "TR_RetroPistolOverlay_MATINST"
OUT = os.path.join("textures", "cloak")
# The textures written, by how they are kept.
KINDS = {"WEP_Defrag_Alphas": "color", "DiffClouds_CLR": "color", "bubbles": "normal",
         "Pixel_4_NORM_DXT1": "normal"}
# The values paint.wgsl draws them with (its CLOAK_* constants).
EXPECTED = {
    OVERLAY: {"scalar": {"MaterialTiling": 3.0, "Lead Intensity": 45.0, "Core Intensity": 35.0,
                         "Core Exponent": 24.0, "Fresnel Intensity": 7.0, "Fresnel Exponent": 3.9137},
              "vector": {"Burn Out Color": (0.0, 0.2039, 1.0), "Defrag Lead Color": (0.0, 0.1296, 0.8641),
                         "Fresnel Color": (1.0, 0.2879, 0.0831)}},
    CLOAK: {"scalar": {"Radial Falloff Intensity": 2.0},
            "vector": {"Fade Out Wave Color": (0.02, 0.01, 0.55)}},
    # (What they don't set is TR_DefragOverlay_AB_Mat's: Lead Intensity 4, Core
    # Intensity 7 and Exponent 32, Energon Fractal Alpha Multiplier 1.)
    DEFRAG: {"scalar": {"MaterialTiling": 3.0, "Lead Intensity": 45.0, "Core Intensity": 35.0,
                        "Core Exponent": 24.0, "Fresnel Intensity": 7.0, "Fresnel Exponent": 3.9137,
                        "Energon Intensity": 5.2745, "Energon Contrast": 5.0},
             "vector": {"Burn Out Color": (0.0, 0.2039, 1.0), "Defrag Lead Color": (0.0, 0.1296, 0.8641),
                        "Fresnel Color": (0.0, 0.8393, 1.0)}},
    ELECTRICAL: {"scalar": {"Fresnel Intensity": 8.2863, "Fresnel Exponent": 4.4078, "Energon Intensity": 8.9804,
                            "Energon Contrast": 1.102, "Energon Fractal Alpha Multiplier": 5.0},
                 "vector": {"Burn Out Color": (0.4863, 0.5255, 1.0), "Defrag Lead Color": (0.0471, 0.1804, 1.0),
                            "Fresnel Color": (0.3059, 0.5647, 0.9137)}},
    CORROSION: {"scalar": {"MaterialTiling": 3.0, "Lead Intensity": 4.1569, "Core Intensity": 4.3804,
                           "Core Exponent": 24.0, "Fresnel Intensity": 7.0784, "Fresnel Exponent": 2.2471,
                           "Energon Intensity": 1.0118, "Energon Contrast": 7.5294},
                "vector": {"Burn Out Color": (0.3882, 0.2118, 0.0), "Defrag Lead Color": (0.1882, 0.0784, 0.0),
                           "Fresnel Color": (0.2353, 0.0902, 0.0)}},
    MOLTEN: {"scalar": {"MaterialTiling": 3.0, "Lead Intensity": 45.0, "Core Intensity": 35.0,
                        "Core Exponent": 24.0, "Fresnel Intensity": 7.0, "Fresnel Exponent": 3.9137,
                        "Energon Intensity": 5.2745, "Energon Contrast": 5.0},
             "vector": {"Burn Out Color": (1.0, 0.0, 0.0), "Defrag Lead Color": (0.9059, 0.0, 0.0),
                        "Fresnel Color": (1.0, 0.098, 0.0)}},
    RETRO: {"scalar": {"MaterialTiling": 3.0, "Lead Intensity": 4.1569, "Core Intensity": 4.3804,
                       "Core Exponent": 24.0, "Fresnel Intensity": 7.0784, "Fresnel Exponent": 2.2471,
                       "Energon Intensity": 1.0118, "Energon Contrast": 7.5294},
            "vector": {"Burn Out Color": (0.5225, 0.0152, 0.4362), "Defrag Lead Color": (0.4, 0.0784, 1.0),
                       "Fresnel Color": (0.6348, 0.107, 0.3463)}},
}
# The dissolve's own textures: IaconMask_NORM's alpha (its lines), ElectricArcs.
DEFRAG_TEXTURES = {"IaconMask_NORM": "lines", "ElectricArcs": "color"}


def check(lib, name):
    """Warns where the material's values aren't paint.wgsl's."""
    _, params = lib.material(name)
    ok = True
    for kind, values in EXPECTED[name].items():
        for pname, want in values.items():
            got = params[kind].get(pname)
            if got is None:
                print(f"  ! {name}: no {pname}")
                ok = False
                continue
            got = got[:len(want)] if isinstance(want, tuple) else got
            if not np.allclose(np.atleast_1d(got), np.atleast_1d(want), atol=1e-3):
                print(f"  ! {name}: {pname} is {got}, paint.wgsl has {want}")
                ok = False
    return ok


def gradient(lib, name):
    """A gradient's 512 values (0-255)."""
    pkg, idx = lib.find(name, ("Texture2D",))
    img, _ = texture.read_texture(pkg, idx, lib.cooked)
    return img[0, :, 0].astype(int)


def main(cooked, assets, verbose=False):
    lib = fxmat.Library(cooked, PACKAGES)
    ok = all([check(lib, name) for name in EXPECTED])
    folder = os.path.join(assets, OUT)
    os.makedirs(folder, exist_ok=True)
    r01, r02 = gradient(lib, "pulseGradien_r01"), gradient(lib, "pulseGradien_r02")
    pulse = np.stack([r01, r02, np.zeros(512, int), np.full(512, 255)], -1).astype(np.uint8)[None]
    Image.fromarray(pulse, "RGBA").save(os.path.join(folder, "pulseGradients.png"))
    print(f"wrote {OUT}/pulseGradients.png (512 x 1)")
    written = set()
    for name in (OVERLAY, CLOAK):
        m = lib.compile(name, "cloak_" + name.lower())
        if verbose:
            print(m.wgsl)
        for found in m.textures:
            if not found:
                continue
            pkg, idx = found
            tname = pkg.exports[idx - 1]["name"]
            kind = KINDS.get(tname)
            if kind is None or tname in written:
                continue
            img, _ = texture.read_texture(pkg, idx, cooked)
            if kind == "color" and img[..., 3].min() == 255:
                img = img[..., :3]
            with open(os.path.join(folder, tname + ".ktx2"), "wb") as fh:
                fh.write(ktx2_io.encode(img, kind))
            written.add(tname)
            print(f"wrote {OUT}/{tname}.ktx2 ({img.shape[1]} x {img.shape[0]}, {kind})")
    for name in (DEFRAG, ELECTRICAL):
        m = lib.compile(name, "defrag_" + name.lower())
        if verbose:
            print(m.wgsl)
    for tname, kind in DEFRAG_TEXTURES.items():
        found = lib.find(tname, ("Texture2D",))
        if not found:
            continue
        pkg, idx = found
        img, _ = texture.read_texture(pkg, idx, cooked)
        if kind == "lines":
            img, kind = img[..., 3], "data"
        else:
            img = img[..., :3]
        with open(os.path.join(folder, tname + ".ktx2"), "wb") as fh:
            fh.write(ktx2_io.encode(img, kind))
        written.add(tname)
        print(f"wrote {OUT}/{tname}.ktx2 ({img.shape[1]} x {img.shape[0]}, {kind})")
    missing = (set(KINDS) | set(DEFRAG_TEXTURES)) - written
    if missing:
        print("  ! not found:", ", ".join(sorted(missing)))
        ok = False
    return 0 if ok else 1


if __name__ == "__main__":
    args = [a for a in sys.argv[1:] if a != "-v"]
    if len(args) != 2:
        print(__doc__)
        sys.exit(2)
    sys.exit(main(args[0], args[1], "-v" in sys.argv))
