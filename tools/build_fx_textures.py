"""Copies the weapon effect textures src/fx.rs uses from a umodel export into
assets/textures/fx, splitting the game's channel-packed ("Greypak") textures and
turning the grenade explosion's material masks into plain sprite images.

usage: python build_fx_textures.py <umodel Texture2D folder> [more folders...] <foc_esc/assets dir>

Most effect textures are used as exported. A Greypak texture holds three greyscale
images, one per colour channel; the game's materials pick one channel and tint it.
Bevy's materials can't pick a channel, so each one used is saved as its own image
(white, with the channel as brightness; the effects are additive).

The Photon Burst Rifle's, the Scatter Blaster's, the grenade explosion's, the
Neutron Assault Rifle's newer effects' (its reload steam and bullet holes), the
Subsonic Repeater's dark impact bursts', the Nucleon Charge Rifle's and the Path
Blaster's textures are listed here, and the older effects' textures, copied as
umodel exports them (from L03b_CutAndRun_BASE_m.xxx: the names at the end of
TEXTURES). Export them first (tools/extract_game.py does):

  umodel -game=trans -export -png -path=<CookedPC> -out=<dir> FX_OS_Common_Textures_p Streak_CLR Texture2D
  (all_shared has two Streak_CLR: this one and FX_Textures_p's, a smaller other picture)
  umodel -game=trans -export -png -path=<CookedPC> -out=<dir> all_shared StarBurst_Greypak_01_CLR Texture2D
  (likewise Light_Star_Streaky_CLR, MuzzleFlash_Light_CLR)
  umodel -game=trans -export -png -path=<CookedPC> -out=<dir> all_shared MuzzleCore_ClusterBomb_CLR Texture2D
  umodel -game=trans -export -png -path=<CookedPC> -out=<dir> L03b_CutAndRun_BASE_m SmokeBall_CLR Texture2D
  (likewise Smoke_Lrg_CLR, SmokeSpear_CLR, MetalDebris_CLR)
  umodel -game=trans -export -png -path=<CookedPC> -out=<dir> FX_Textures_p SmokeThin_CLR Texture2D
  umodel -game=trans -export -png -path=<CookedPC> -out=<dir> ENV_ASH_interior1_p ENV_ASH_BulletHoleDecal_CLR Texture2D
  (likewise ENV_ASH_BulletHoleDecal_CLR2)
  umodel -game=trans -export -png -path=<CookedPC> -out=<dir> L03b_CutAndRun_BASE_m 4x4_Bursts_CLR Texture2D
  umodel -game=trans -export -png -path=<CookedPC> -out=<dir> L03b_CutAndRun_BASE_m Ring_CLR Texture2D
  (likewise MuzzleFlash_Sparks_CLR, MuzzleCap_ClusterBomb_CLR, Spark_Base_CLR,
  smokeball_01_CLR)
  umodel -game=trans -export -png -path=<CookedPC> -out=<dir> all_shared SpaceBridge_Iris_CLR Texture2D
  (likewise TankMuzzleFlash_CLR, MuzzleCap_RocketTurret_CLR, ember_clip_CLR,
  Flame_CLR, blastmark_glow_CLR)
  umodel -game=trans -export -png -path=<CookedPC> -out=<dir> L03b_CutAndRun_BASE_m Bolt_CLR Texture2D
  (likewise the other older effects' textures at the end of TEXTURES)

Their pixel data is in FX_OS_Common_Textures_p.xxx and FX_Textures_p.xxx, which have
to be in the CookedPC folder too.
"""
import os
import shutil
import sys

import numpy as np
from PIL import Image

# (texture in the export, file written, how: None = copy as is, a channel number =
# that channel as a white image, "luma_alpha" = white with the image's brightness as
# its opacity, "white_alpha" = white keeping the image's opacity, "debris",
# "bullet_hole", "path_core" and "scorch" = see below)
TEXTURES = [
    ("Streak_CLR", "Streak_CLR", None),
    ("Light_Star_Streaky_CLR", "Light_Star_Streaky_CLR", None),
    ("MuzzleFlash_Light_CLR", "MuzzleFlash_Light_CLR", None),
    # StarBurst_Greypak_01_CLR: R a small hot core, G a spray of spark chunks,
    # B a many-pointed star.
    ("StarBurst_Greypak_01_CLR", "StarBurst_Star_CLR", 2),
    ("StarBurst_Greypak_01_CLR", "StarBurst_Chunks_CLR", 1),
    # The grenade explosion (Grenade_EXP_FX). Its smoke materials light the smoke
    # through normal maps and colour curves; the demo's sprites take the shape (the
    # opacity) and are tinted. SmokeBall_CLR has no alpha: its brightness is the shape.
    ("SmokeBall_CLR", "SmokeBall_CLR", "luma_alpha"),
    ("Smoke_Lrg_CLR", "Smoke_Lrg_CLR", None),
    ("SmokeSpear_CLR", "SmokeSpear_CLR", "white_alpha"),
    # MetalDebris_CLR packs the debris shapes into green (2x2 pieces); the material
    # cuts them out (BLEND_Masked) and shades them as grey metal.
    ("MetalDebris_CLR", "MetalDebris_CLR", "debris"),
    # The Scatter Blaster's fire burst (Scatterblaster_MuzzleFlash_FX MuzzleCore):
    # four flames side by side.
    ("MuzzleCore_ClusterBomb_CLR", "MuzzleCore_ClusterBomb_CLR", None),
    # The rifle's reload steam (ReloadSteam_Med_FX SmokeSteam, Steam_Wispy_Mat): a
    # wisp of smoke whose brightness is its shape.
    ("SmokeThin_CLR", "SmokeThin_CLR", "luma_alpha"),
    # The bullet holes its impacts leave (AssaultRifle_VEH_Squib_FX's decal emitter,
    # ENV_ASH_BulletHoles): two holes on white, the material's opacity being one minus
    # that, and their rims in _CLR2, which glow hot. Saved as one image: the rims as
    # its colour and the holes as its opacity.
    ("ENV_ASH_BulletHoleDecal_CLR", "BulletHole_CLR", "bullet_hole"),
    # The Subsonic Repeater's impact (Subsonic_Repeater_Squib_FX Black_Spears,
    # 4x4_Bursts_MAT): dark bursts, the texture's brightness being their opacity.
    ("4x4_Bursts_CLR", "4x4_Bursts_Mask_CLR", "luma_alpha"),
    # The Nucleon Charge Rifle's: its tracer's rings (NucleonRifle_Tracer_FX
    # Rings_Export, Ring_Distort_MAT: a ring whose brightness is its shape), its
    # muzzle flash's four flames (Flash_Blast, PathBlaster_Muzzle_MAT), and its
    # impact's fire burst (bLast, NucleonRifle_Squib_MAT), white-hot spark burst
    # (Flare_Tall_Dup, Sparks_Base_Mat) and flares (Flare_Tall, flareball01_MAT).
    ("Ring_CLR", "Ring_CLR", "luma_alpha"),
    ("MuzzleFlash_Sparks_CLR", "MuzzleFlash_Sparks_CLR", None),
    ("MuzzleCap_ClusterBomb_CLR", "MuzzleCap_ClusterBomb_CLR", None),
    ("Spark_Base_CLR", "Spark_Base_CLR", None),
    ("smokeball_01_CLR", "smokeball_01_CLR", None),
    # The Path Blaster's muzzle flash: its fire burst (MuzzleCore_Export,
    # Burst_04_Mesh_MAT_INST: the texture's red, greyed, tinted by the particle) and
    # its fiery iris ring (MuzzleCap_Dup, PathBlaster_MuzzleCore_Mat, which mixes
    # four textures: see path_core).
    ("MuzzleCap_ClusterBomb_CLR", "MuzzleCap_ClusterBomb_R_CLR", 0),
    ("SpaceBridge_Iris_CLR", "PathBlaster_MuzzleCore_CLR", "path_core"),
    # Its impact (PathBlaster_Squib_FX): the golden sparkles (EmbersVelocity_Export,
    # Embers_02_MAT), the flames (Fire_A, Fire_Lrg_MAT_INST) and the scorch mark it
    # leaves (its decal emitter, DeathDecal_MAT: see scorch).
    ("ember_clip_CLR", "ember_clip_CLR", None),
    ("Flame_CLR", "Flame_CLR", None),
    ("blastmark_glow_CLR", "Blastmark_CLR", "scorch"),
] + [
    # The older effects' textures (src/fx.rs's TEXTURES), copied as umodel exported
    # them (L03b_CutAndRun_BASE_m.xxx's).
    (name, name, "copy") for name in (
        "2x2SubUV_lightburst_CLR", "4x4_Bursts_CLR", "4x4_Clouds_Dark_CLR", "Bolt_CLR", "FireSpears_CLR",
        "Ion_Cannon_MuzzleFlash_Cap_CLR", "Ion_Cannon_MuzzleFlash_Core_CLR", "LightBeam_Falloff_01_CLR",
        "MuzzleFlash2_CLR", "MuzzleFlash_Scrapmaker_CLR", "SmokeClouds_01_CLR", "Spark_Trail_CLR", "Sparks_01_CLR",
        "SphereGlow_01_CLR", "Spot_CLR", "embers_01_CLR", "lightningring_01_CLR")
]


def path_core(iris, sources):
    """PathBlaster_MuzzleCore_Mat's colour: TankMuzzleFlash_CLR x MuzzleCap_ClusterBomb_CLR
    plus five times SpaceBridge_Iris_CLR x MuzzleCap_RocketTurret_CLR, all times the
    rocket burst's opacity (its depth-biased alpha). The rocket burst is sampled where
    the material's panner leaves it, 0.05 across and 0.085 up (its time input is the
    constant 1); the slight wobble a smoke texture adds is left out. Saved at a fifth
    of its brightness (src/fx.rs's colour puts the five back), clipped to white."""
    def load(name):
        path = next((os.path.join(d, name + ".png") for d in sources
                     if os.path.exists(os.path.join(d, name + ".png"))), None)
        if path is None:
            print("  ! %s.png not found" % name)
        return Image.open(path) if path else None
    tank, cluster, rocket = (load(n) for n in ("TankMuzzleFlash_CLR", "MuzzleCap_ClusterBomb_CLR",
                                                 "MuzzleCap_RocketTurret_CLR"))
    if not (tank and cluster and rocket):
        return None
    size = rocket.size
    rgb = lambda im: np.asarray(im.convert("RGB").resize(size, Image.BILINEAR), np.float32) / 255.0
    rocket_rgba = np.asarray(rocket.convert("RGBA"), np.float32) / 255.0
    w, h = size
    # The panned lookup wraps around (the texture's default addressing).
    xs = (np.arange(w) + 0.5) / w + (-0.05)
    ys = (np.arange(h) + 0.5) / h + (-0.085)
    ix = (np.floor(xs * w).astype(int)) % w
    iy = (np.floor(ys * h).astype(int)) % h
    rocket_rgba = rocket_rgba[iy][:, ix]
    colour = rgb(tank) * rgb(cluster) + 5.0 * rgb(iris) * rocket_rgba[..., :3]
    colour *= rocket_rgba[..., 3:4]
    out = np.clip(colour / 5.0, 0.0, 1.0)
    return Image.fromarray((out * 255.0 + 0.5).astype(np.uint8), "RGB")


def main(sources, assets):
    out = os.path.join(assets, "textures", "fx")
    os.makedirs(out, exist_ok=True)
    for name, dest, channel in TEXTURES:
        src = next((os.path.join(d, name + ".png") for d in sources
                    if os.path.exists(os.path.join(d, name + ".png"))), None)
        if src is None:
            print("  ! %s.png not found" % name)
            continue
        if channel == "copy":
            shutil.copyfile(src, os.path.join(out, dest + ".png"))
            continue
        im = Image.open(src)
        if channel is None:
            mode = "RGBA" if "A" in im.getbands() else "RGB"
            im.convert(mode).save(os.path.join(out, dest + ".png"), optimize=True)
        elif channel == "luma_alpha":
            white = Image.new("L", im.size, 255)
            Image.merge("RGBA", (white, white, white, im.convert("L"))).save(os.path.join(out, dest + ".png"), optimize=True)
        elif channel == "white_alpha":
            white = Image.new("L", im.size, 255)
            Image.merge("RGBA", (white, white, white, im.convert("RGBA").split()[3])).save(
                os.path.join(out, dest + ".png"), optimize=True)
        elif channel == "bullet_hole":
            rims = os.path.join(os.path.dirname(src), name + "2.png")
            if not os.path.exists(rims):
                print("  ! %s2.png not found" % name)
                continue
            holes = im.convert("L").point(lambda v: 255 - v)
            rim = Image.open(rims).convert("L").resize(im.size)
            Image.merge("RGBA", (rim, rim, rim, holes)).save(os.path.join(out, dest + ".png"), optimize=True)
        elif channel == "path_core":
            core = path_core(im, sources)
            if core is None:
                continue
            core.save(os.path.join(out, dest + ".png"), optimize=True)
        elif channel == "scorch":
            # DeathDecal_MAT darkens the wall (modulated) across the burn in green,
            # down to black where red is dark (red x 20, lerped in by the decal's
            # opacity). Here: black, the green as its opacity.
            g = im.convert("RGB").split()[1]
            black = Image.new("L", im.size, 0)
            Image.merge("RGBA", (black, black, black, g)).save(os.path.join(out, dest + ".png"), optimize=True)
        elif channel == "debris":
            g = im.convert("RGB").split()[1]
            shade = g.point(lambda v: int(60 + v * 0.55))
            alpha = g.point(lambda v: 255 if v > 60 else 0)
            Image.merge("RGBA", (shade, shade, shade, alpha)).save(os.path.join(out, dest + ".png"), optimize=True)
        else:
            g = im.convert("RGB").split()[channel]
            Image.merge("RGB", (g, g, g)).save(os.path.join(out, dest + ".png"), optimize=True)
        print("wrote textures/fx/%s.png" % dest)


if __name__ == "__main__":
    if len(sys.argv) < 3:
        sys.exit(__doc__)
    main(sys.argv[1:-1], sys.argv[-1])
