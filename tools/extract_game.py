"""Builds the demo's game assets (assets\\models, textures, sounds, levels, HUD,
menus...) from your own copy of Transformers: Fall of Cybertron, in one go.

usage: python extract_game.py [--game <Transformers Fall of Cybertron folder>]
                              [--umodel <umodel.exe>] [--assets <foc_esc\\assets>]
                              [--work <work folder>] [--from <step>] [--only <step>[,<step>...]]
                              [--list] [--redo] [--keep-exports]

It finds the game (the --game folder, or in Steam's libraries: those Steam's own
settings list, or its usual places) and UE Viewer
(umodel: --umodel, or umodel.exe next to this folder, in tools\\umodel, or on the
PATH), then runs the steps below in order: each exports from the game's packages
what its build script reads (umodel, into <work>\\exports) and runs the script
(tools\\build_*.py), which writes its part of the assets. Every step logs to
<work>\\logs\\<step>.log; the steps done are kept in <work>\\done.txt, so run it
again after a failure and it carries on from the step that failed (--redo starts
over, --from <step> from that step, --only runs just those). An export is deleted
once no step still to run needs it (--keep-exports keeps them all), so the work
folder stays small. The text files a step writes are given LF line endings, as
the demo's own have. Into another folder than the demo's own assets (--assets), it
also copies the demo's own shaders (assets\\shaders: written by hand, not built from
the game), so that folder runs as it is.

umodel exports the animation sets (.psa) too, but the official UE Viewer builds
drop the clips FoC keeps in its own layout (most of them), so each set is written
again with psa_write.py from the package, the clips umodel did save kept as they
are. An export that leaves out an object it named stops the step.

It needs Python 3 with numpy, Pillow and fontTools (pip install numpy pillow
fonttools) and ffmpeg on the PATH (the sounds and the menu's movies), and UE Viewer
from https://www.gildor.org/en/projects/umodel. --list prints the steps.

The game's files are the game's publisher's: what this builds is for your own use
(see the README's *Keep the game's files private*).
"""
import argparse
import hashlib
import os
import shutil
import subprocess
import sys
import time

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.dirname(HERE)
# Part of every export's marker: a new value has exports made by an earlier version
# of this script made again (2: animation sets written by psa_write.py).
EXPORT_VERSION = "2"

# The assets' text files (given LF line endings: Runner.lf_endings).
TEXT = {".txt", ".json", ".ron", ".csv", ".cfg", ".wgsl"}

# Where Steam usually puts the game.
GAME_DIR = "Transformers Fall of Cybertron"
STEAM_LIBRARIES = [
    r"C:\Program Files (x86)\Steam\steamapps\common",
    r"C:\Program Files\Steam\steamapps\common",
    r"D:\Program Files\Steam\steamapps\common",
    r"D:\Program Files (x86)\Steam\steamapps\common",
    r"D:\SteamLibrary\steamapps\common",
    r"E:\SteamLibrary\steamapps\common",
    r"D:\Steam\steamapps\common",
    r"E:\Steam\steamapps\common",
    os.path.expanduser("~/.steam/steam/steamapps/common"),
    os.path.expanduser("~/.local/share/Steam/steamapps/common"),
]


# ---------------------------------------------------------------------------
# What a step is made of
# ---------------------------------------------------------------------------

class Export:
    """A umodel export from `package` (.xxx) into <exports>/<folder>: the objects
    named (none: all of it; with `cls`, the one object of that class), with -gltf
    (meshes as glTF) and/or -png (textures as PNG); neither: meshes .psk(x),
    animations .psa (rewritten by psa_write.py: Runner.umodel_run). umodel writes
    <folder>/<package name>/<class>/<object>.<ext>."""

    def __init__(self, folder, package, objects=(), gltf=False, png=False, cls=None):
        self.folder = folder
        self.package = package
        self.objects = list(objects)
        self.gltf = gltf
        self.png = png
        self.cls = cls


class Printed:
    """The umodel commands a build script prints (its --umodel mode): `args` are the
    script and its arguments; every printed line that starts with "umodel" is run,
    with this umodel, into the export `folder` the arguments name."""

    def __init__(self, folder, args):
        self.folder = folder
        self.args = list(args)


class Step:
    """A step: its name, what it makes, the exports it needs (Export or Printed, in
    order; an export another step already made this run is not made again), and the
    commands it then runs: each a list of arguments for Python ({cooked},
    {transgame}, {game}, {assets}, {exports}, {tools} and {work} filled in), or a
    function of the runner and the log returning 0 for success."""

    def __init__(self, name, what, exports=(), commands=()):
        self.name = name
        self.what = what
        self.exports = list(exports)
        self.commands = [c if callable(c) else list(c) for c in commands]

    def folders(self):
        """The export folders this step reads."""
        out = {e.folder for e in self.exports}
        for c in self.commands:
            if not callable(c):
                for a in c:
                    if "{exports}/" in a:
                        out.add(a.split("{exports}/", 1)[1].split("/")[0])
        return out


def py(*args):
    return list(args)


# ---------------------------------------------------------------------------
# The steps
# ---------------------------------------------------------------------------

# The Escalation maps: their key, packages (ESC_<x>_ART_m and _BASE_m) and the
# animation sets and textures besides the whole packages build_map.py reads (the
# README's map sections).
MAPS = [
    ("downfall", "Iacon", [("BASE", "AP_CoverABIG_ANIM")], []),
    ("oblivion", "Ark", [("ART", "AP_IntDoor_ANIM")], [("ENV_AllShader_p.xxx", "ChainLinkFence_02_MASK")]),
    ("ancients", "Temple", [("BASE", "AP_CoverABIG_ANIM")], []),
    ("ignition", "Gladiator", [("BASE", "AP_SteamVent_ANIM")], []),
]
PICKUP_BASE = ["MP_PickupBase_TEXSET_Color_NormX", "MP_PickupBase_TEXSET_Masks_NormY"]


def map_exports(key):
    _, pk, anims, textures = next(m for m in MAPS if m[0] == key)
    out = [Export(key, f"ESC_{pk}_ART_m.xxx", gltf=True, png=True),
           Export(key, f"ESC_{pk}_BASE_m.xxx", gltf=True, png=True)]
    for part, anim in anims:
        out.append(Export(key, f"ESC_{pk}_{part}_m.xxx", [anim], cls="AnimSet"))
    for t in PICKUP_BASE:
        out.append(Export(key, "ENV_MP_PropsShared_p.xxx", [t], png=True, cls="Texture2D"))
    for package, t in textures:
        out.append(Export(key, package, [t], png=True, cls="Texture2D"))
    return out


def copy_pictures(packages, into):
    """A step's command: copies the pictures of the packages' whole exports (in the
    "assets" export folder) over the all_shared export's (where build_assets.py
    finds them first: the weapons' packages hold the full-size texture sets)."""
    def run(runner, log):
        dst = os.path.join(runner.exports, "assets", into, "Texture2D")
        os.makedirs(dst, exist_ok=True)
        n = 0
        for package in packages:
            src = os.path.join(runner.exports, "assets", package[:-4], "Texture2D")
            for f in sorted(os.listdir(src)) if os.path.isdir(src) else []:
                shutil.copyfile(os.path.join(src, f), os.path.join(dst, f))
                n += 1
        log.write(f"\ncopied {n} pictures into {dst}\n")
        return 0
    return run


def place_animations(runner, log):
    """A step's command: puts each animation set exported (.psa, from the "assets"
    export folder's packages, L03b_CutAndRun_BASE_m's first) where build_assets.py
    and build_customize.py look: Animations/<set>/<set>.psa."""
    root = os.path.join(runner.exports, "assets")
    first = "L03b_CutAndRun_BASE_m"
    n = 0
    for package in [first] + sorted(d for d in os.listdir(root) if d not in (first, "Animations")):
        src = os.path.join(root, package, "AnimSet")
        for f in sorted(os.listdir(src)) if os.path.isdir(src) else []:
            dst = os.path.join(root, "Animations", f[:-4], f)
            if f.endswith(".psa") and not os.path.exists(dst):
                os.makedirs(os.path.dirname(dst), exist_ok=True)
                shutil.copyfile(os.path.join(src, f), dst)
                n += 1
    log.write(f"\nplaced {n} animation sets in {os.path.join(root, 'Animations')}\n")
    return 0


# What build_assets.py and build_pickups.py read (the "assets" export folder): Jazz,
# the guns, his car, the stations' props, and their textures and animation sets.
L03B = "L03b_CutAndRun_BASE_m.xxx"
WEAPON_PACKAGES = ["WEP_TracerDart_p.xxx", "WEP_HeavyMG_p.xxx", "WEP_EMPShotgun_p.xxx", "WEP_RepairBeam_p.xxx",
                   "WEP_AoeSlow_p.xxx", "WEP_Sawblade_p.xxx", "WEP_RocketLauncher_p.xxx", "WEP_GrenadeLauncher_p.xxx",
                   "WEP_GrenadeArm_p.xxx"]


def texture_sets(*names):
    return [n + s for n in names for s in ("_Color_NormX", "_Masks_NormY")]


ASSETS_EXPORTS = [
    Export("assets", L03B, ["RB_Jazz_CHARSKEL_" + p for p in "Legs Chest Head ArmUpL ArmUpR ArmLoL CapL CapR ArmLoR".split()]
           + ["RB_AssaultRifleR_SKEL", "RB_AssaultRifleL_SKEL", "RB_IonBlasterR_SKEL", "RB_IonBlasterL_SKEL",
              "RB_BurstRifleL_SKEL", "RB_SniperRifleR_SKEL", "RB_SniperRifleL_SKEL", "RB_HeavyPistolR_SKEL",
              "RB_HeavyPistolL_SKEL", "VH_Jazz_SKEL", "WEP_AssaultRifleVH_SKEL", "AP_AttackSentry_SKEL"],
           gltf=True, png=True),
    Export("assets", L03B, ["RB_Jazz_STAT_Wheel", "PROP_UND_PistonSet_C_STAT", "ENV_RST_EXT_Wall01_STAT",
                            "vt_pipeset_02_STAT", "ENV_RST_VentCover01_STAT", "PROP_NEU_EnergonCube_STAT",
                            "WEP_Grenade_Flak_STAT", "ENV_SkyDome_STAT"], gltf=True, png=True),
    Export("assets", L03B, texture_sets(
        "RB_Jazz_Legs_TEXSET", "RB_Jazz_Chest_TEXSET", "RB_Jazz_Head_TEXSET", "RB_Jazz_ArmUP_TEXSET",
        "RB_Jazz_ArmLo_TEXSET", "RB_Jazz_Wheel_TEXSET", "VH_Jazz_Clr_TEXSET", "WEP_ArmUp_TEXSET",
        "WEP_AssaultRifle_TEXSET", "WEP_SubsonicRepeater_TEXSET", "WEP_IonBlaster_TEXSET", "WEP_SniperRifle_TEXSET",
        "AP_AttackSentry_body_TEXSET", "AP_AttackSentry_parts_TEXSET", "ENV_RST_EXT_Wall01l_TXTSET",
        "ENV_RST_SewerLedgeFloor01_TXTSET", "ENV_RST_VentCover_TXTSET", "PROP_UND_PistonSet_C_CLR_TEXSET",
        "WEP_Grenade_TEXSET") + ["vint_pipeset_CLR", "vint_pipeset_NORM", "IaconSunset_CLR"], png=True),
    # (Jazz's animation sets: the glTF exports of Jazz with a set that only umodel's
    # viewer saves are stood in for by the sets' .psa, build_assets.gltf_or_psa.)
    Export("assets", L03B, ["Shared_ROBO_NAV_ANIM", "Shared_SCOUT_ROBO_ANIM", "Shared_ROBO_ANIM", "Death_Shared_ANIM",
                            "Jazz_ROBO_ANIM_NEW", "WeaponArm_ROBO_ANIM", "Weapon_AssaultRifle_ANIM",
                            "Weapon_IonBlaster_ANIM", "Weapon_SniperRifle_ANIM", "Weapon_HeavyPistol_ANIM",
                            "Jazz_Arm_ROBO_ANIM", "Jazz_VEH_ANIM", "WEP_AssaultRifleVH_ANIM"]),
    Export("assets", "all_shared.xxx", [
        "RB_BurstRifleR_SKEL", "RB_ScatterblasterR_SKEL", "RB_ScatterblasterL_SKEL", "RB_BazookaR_SKEL",
        "RB_BazookaL_SKEL", "RB_SysCorruptorR_SKEL", "RB_SysCorruptorL_SKEL", "RB_HeavyMGR_SKEL", "RB_HeavyMGL_SKEL",
        "RB_EmpShotgunR_SKEL", "RB_EmpShotgunL_SKEL", "RB_RepairBeamR_SKEL", "RB_RepairBeamL_SKEL",
        "RB_RustCannonR_SKEL", "RB_RustCannonL_SKEL", "RB_SawbladeR_SKEL", "RB_SawbladeL_SKEL",
        "RB_RocketLauncherR_SKEL", "RB_RocketLauncherL_SKEL", "RB_GrenadeLauncherR_SKEL", "RB_GrenadeLauncherL_SKEL",
        "RB_GrenadeArmR_SKEL", "RB_GrenadeArmL_SKEL", "AmmoPickupLight_STAT", "AP_KMine_SKEL"], gltf=True, png=True),
    Export("assets", "all_shared.xxx", texture_sets(
        "WEP_BurstRifle_TEXSET", "WEP_Shotgun_TEXSET", "WEP_RiotCannon_TEXSET", "PROP_NEU_AmmoPickup_NEW_texset"),
        png=True),
    Export("assets", "all_shared.xxx", [
        "Weapon_PulseRifle_ANIM", "Weapon_Shotgun_ANIM", "Weapon_Bazooka_ANIM", "Weapon_TracerDart_ANIM",
        "Weapon_HeavyMG_ANIM", "WEP_EMPShotgun_ANIM2", "Weapon_RepairBeam_ANIM", "Weapon_RustCannon_ANIM",
        "Weapon_Sawblade_ANIM", "Weapon_RocketLauncher_ANIM", "Weapon_GrenadeLauncher_ANIM", "WEP_GrenadeArm_ANIM",
        "WEP_KamikazeMine_ANIM_02"]),
    *[Export("assets", p, png=True) for p in WEAPON_PACKAGES],
    Export("assets", "ESC_Iacon_BASE_m.xxx", ["ESC_PickupBase_Health_STAT"], gltf=True, cls="StaticMesh"),
    *[Export("assets", "ENV_MP_PropsShared_p.xxx", [t], png=True, cls="Texture2D") for t in PICKUP_BASE],
    Export("assets", "PROP_NEU_Pickups_p.xxx", ["fractalMaps"], png=True, cls="Texture2D"),
    Export("assets", "ESC_Ark_BASE_m.xxx", ["AmmoPickupHeavy_STAT", "MP_Pickup_Barrier_STAT",
                                            "MP_Pickup_KamikazeMine_STAT", "MP_Pickup_EnergonRecharger_STAT",
                                            "MP_Pickup_BlackHoleGrenade_STAT", "WEP_GravityBomb_STAT"], gltf=True),
    Export("assets", "mp_shared.xxx", ["MP_Pickup_Intercooler_STAT"], gltf=True),
    Export("assets", "ENV_MP_PropsShared_p.xxx", texture_sets(
        "MP_Pickup_Barrier_TEXSET", "MP_Pickup_EnergonRecharger_TEXSET", "MP_Pickup_Intercooler_TEXSET"), png=True),
    Export("assets", "WEP_KamikazeMine_p.xxx", texture_sets("WEP_KMine_TEXSET"), png=True),
    Export("assets", "PROP_NEU_Pickups_p.xxx", texture_sets("PROP_NEU_AmmoPickup_NEW_texset"), png=True),
    Export("assets", "ESC_Ark_BASE_m.xxx", texture_sets("WEP_GravityBomb2_TEXSET"), png=True),
]
ASSETS_SRC = "{exports}/assets/L03b_CutAndRun_BASE_m"

# The menus' and HUD's pictures (the "ui" export folder).
UI_EXPORTS = [
    Export("ui", "TransGame.xxx", png=True),
    Export("ui", "UI_FrontEnd_m.xxx", png=True),
    Export("ui", "UI_Lobby_m.xxx", png=True),
    Export("ui", "UI_Lobby_ESC_m.xxx", png=True),
    *[Export("ui", "mp_shared.xxx", [t], png=True, cls="Texture2D")
      for t in ["PauseMenu_GFX_I51", "PauseMenu_GFX_I4D", "PlayerList_GFX_I51", "PlayerList_GFX_I53",
                "PlayerList_GFX_I55", "PlayerList_GFX_I27", "PlayerList_GFX_I2B"]],
]

ENEMY_EXPORT = Printed("enemies", py("build_enemies.py", "--umodel", "{cooked}", "{exports}/enemies"))
CLASSES = ["infiltrator", "destroyer", "titan", "scientist"]


def class_step(cls):
    folder = "class_" + cls
    printed = Printed(folder, py("build_class.py", "--umodel", cls, "{cooked}", "{transgame}", "{exports}/" + folder))
    arm = py("build_arm_equip.py", cls, "{exports}/" + folder, "{cooked}", "{assets}")
    if cls != "infiltrator":
        # (Jazz's legs, which its forearm sets are fitted against, from the
        # Infiltrator's export.)
        arm.append("{exports}/class_infiltrator")
    # (--umodel twice: the second time it also lists the textures of the meshes the
    # first exported.)
    return Step(folder, f"the {cls.capitalize()} class's characters, vehicles and clips", [printed, printed],
                [py("build_class.py", cls, "{exports}/" + folder, "{cooked}", "{transgame}", "{assets}"), arm])


# The effect textures src/fx.rs uses (build_fx_textures.py's list and the plain copies).
FX_TEXTURES = {
    # (all_shared.xxx holds two Streak_CLR, FX_OS_Common_Textures_p's and FX_Textures_p's,
    # which umodel would both write to one file: the first is the effects'.)
    "FX_OS_Common_Textures_p.xxx": ["Streak_CLR"],
    "all_shared.xxx": ["StarBurst_Greypak_01_CLR", "Light_Star_Streaky_CLR", "MuzzleFlash_Light_CLR",
                       "MuzzleCore_ClusterBomb_CLR", "SpaceBridge_Iris_CLR", "TankMuzzleFlash_CLR",
                       "MuzzleCap_RocketTurret_CLR", "ember_clip_CLR", "Flame_CLR", "blastmark_glow_CLR"],
    "L03b_CutAndRun_BASE_m.xxx": ["SmokeBall_CLR", "Smoke_Lrg_CLR", "SmokeSpear_CLR", "MetalDebris_CLR", "4x4_Bursts_CLR",
                                  "Ring_CLR", "MuzzleFlash_Sparks_CLR", "MuzzleCap_ClusterBomb_CLR", "Spark_Base_CLR",
                                  "smokeball_01_CLR",
                                  # (the older effects', copied as they are)
                                  "2x2SubUV_lightburst_CLR", "4x4_Clouds_Dark_CLR", "Bolt_CLR", "FireSpears_CLR",
                                  "Ion_Cannon_MuzzleFlash_Cap_CLR", "Ion_Cannon_MuzzleFlash_Core_CLR",
                                  "LightBeam_Falloff_01_CLR", "MuzzleFlash2_CLR", "MuzzleFlash_Scrapmaker_CLR",
                                  "SmokeClouds_01_CLR", "Spark_Trail_CLR", "Sparks_01_CLR", "SphereGlow_01_CLR",
                                  "Spot_CLR", "embers_01_CLR", "lightningring_01_CLR"],
    "FX_Textures_p.xxx": ["SmokeThin_CLR"],
    "ENV_ASH_interior1_p.xxx": ["ENV_ASH_BulletHoleDecal_CLR", "ENV_ASH_BulletHoleDecal_CLR2"],
}

STEPS = [
    # The maps, then what is built on them (the doors and consoles change the maps'
    # files in place; the pods, barrels and navigation read them).
    *[Step(f"map_{key}", f"the {key.capitalize()} map (meshes, textures, collision, lighting, effects)",
           map_exports(key), [py("build_map.py", key, "{exports}/" + key, "{cooked}", "{assets}")])
      for key, *_ in MAPS],
    Step("doors", "the maps' doors, consoles and traps' scripting",
         [e for key, *_ in MAPS for e in map_exports(key)],
         [py("build_doors.py", "{cooked}", "{assets}", *[a for key, *_ in MAPS for a in (key, "{exports}/" + key)])]),
    Step("pods", "the Armory Recreator storage pods", map_exports("downfall"),
         [py("build_pods.py", "{cooked}", "{exports}/downfall", "{assets}")]),
    Step("barrels", "Oblivion's exploding barrels", map_exports("oblivion"),
         [py("build_barrels.py", "{exports}/oblivion", "{cooked}", "{assets}")]),
    Step("motion", "the maps' spinning parts and pulsing lights", [],
         [py("build_motion.py", "{cooked}", "{assets}")]),
    Step("nav", "the maps' navigation meshes", [], [py("build_nav.py", "{cooked}", "{assets}")]),
    Step("waves", "the Escalation waves", [], [py("build_waves.py", "{cooked}", "{assets}")]),
    Step("traps", "the trap sentries",
         [Export("traps", "ESC_Iacon_BASE_m.xxx", ["WEP_SentryRocket_SKEL"], gltf=True),
          Export("traps", "ESC_Iacon_BASE_m.xxx", ["WEP_SentryRocket_ANIM"]),
          Export("traps", "ESC_Gladiator_BASE_m.xxx", ["WEP_SentryBlaster_SKEL"], gltf=True),
          Export("traps", "ESC_Gladiator_BASE_m.xxx", ["WEP_SentryBlaster_ANIM"]),
          Export("traps", "ESC_Iacon_BASE_m.xxx", ["WEP_Sentry_New_TEXSET_Color_NormX",
                                                   "WEP_Sentry_New_TEXSET_Masks_NormY"], png=True)],
         [py("build_traps.py", "{exports}/traps", "{cooked}", "{assets}")]),
    Step("fx_meshes", "the ion core and booster cone effect meshes",
         [Export("fx_meshes", "L03b_CutAndRun_BASE_m.xxx", ["Ion_Cannon_MuzzleFlash_STAT"]),
          Export("fx_meshes", "L03b_CutAndRun_BASE_m.xxx", ["Boostermesh_03"])],
         [py("build_fx_meshes.py", "{exports}/fx_meshes/L03b_CutAndRun_BASE_m/StaticMesh3/Ion_Cannon_MuzzleFlash_STAT.pskx",
             "{assets}/models/fx/ion_core.glb"),
          py("build_fx_meshes.py", "{exports}/fx_meshes/L03b_CutAndRun_BASE_m/StaticMesh3/Boostermesh_03.pskx",
             "{assets}/models/fx/booster_cone.glb", "--along", "x")]),
    Step("fx_textures", "the weapons' effect textures",
         [Export("fx_textures", pkg, names, png=True) for pkg, names in FX_TEXTURES.items()],
         [py("build_fx_textures.py", "{exports}/fx_textures/all_shared/Texture2D",
             "{exports}/fx_textures/L03b_CutAndRun_BASE_m/Texture2D", "{exports}/fx_textures/FX_Textures_p/Texture2D",
             "{exports}/fx_textures/ENV_ASH_interior1_p/Texture2D",
             "{exports}/fx_textures/FX_OS_Common_Textures_p/Texture2D", "{assets}")]),
    Step("pregame", "the pre-game lobby's pictures", [], [py("build_pregame.py", "{cooked}", "{assets}")]),
    Step("garage", "the Customize screen's garage",
         [Export("garage", "UI_CustomCharacter_m.xxx", gltf=True, png=True)],
         [py("build_garage.py", "{exports}/garage/UI_CustomCharacter_m", "{cooked}", "{assets}")]),
    Step("paint", "the characters' paint", [], [py("build_paint.py", "{cooked}", "{assets}")]),
    Step("cloak", "the cloak's look", [], [py("build_cloak.py", "{cooked}", "{assets}")]),
    Step("upgrades", "the weapon upgrades' parts",
         [Export("upgrades", "all_shared.xxx", [
             "WEP_AutoFire_STAT", "WEP_BurstRifleShowInfo_STAT", "WEP_DrainGunIncreaseDrain_STAT", "WEP_FastReload_STAT",
             "WEP_FireInterval_STAT", "WEP_FiringGroundSpeed_STAT", "WEP_ImprovedHardLock_STAT",
             "WEP_IncreaseFineAimFootSpeed_STAT", "WEP_IncreasedHardLockDuration_STAT", "WEP_LargerClip_STAT",
             "WEP_LastShotInClipSpecial_STAT", "WEP_NumClusters_STAT", "WEP_ProjectileDamageRadius_STAT",
             "WEP_ProjectileSpeedIncrease_STAT", "WEP_PuddlesLingerLonger_STAT", "WEP_RandomShotSpecial_STAT",
             "WEP_Range_STAT", "WEP_ReduceWindUpSpeed_STAT", "WEP_ReducedRecoil_STAT", "WEP_ReserveAmmo_STAT",
             "WEP_SawbladeChargeTime_STAT", "WEP_SniperRifleHitsSpeedUpCharge_STAT", "WEP_SpreadCooldown_STAT",
             "WEP_Spread_STAT", "WEP_WeaponLockOnTime_STAT"], gltf=True)],
         [py("build_upgrades.py", "{cooked}", "{exports}/upgrades", "{assets}")]),
    Step("abilities", "the class abilities' models and clips",
         [Export("abilities", "mp_shared.xxx", ["AP_AttackSentry_SKEL", "AP_AttackSentrySlow_SKEL",
                                                "WEP_ShieldLeader_SKEL"], gltf=True),
          Export("abilities", "mp_shared.xxx", ["AP_AttackSentry_ANIM", "WEP_ShieldLeader_ANM"]),
          Export("abilities", "mp_shared.xxx", texture_sets(
              "AP_AttackSentry_body_TEXSET", "AP_AttackSentry_parts_TEXSET", "WEP_RustCannon_TEXSET",
              "WEP_MPShield_TEXSET", "WEP_MPShield_Energon_TEXSET"), png=True)],
         [py("build_abilities.py", "{exports}/abilities", "{cooked}", "{assets}")]),
    Step("assets", "Jazz (the robot and his car), the guns, the stations' props",
         ASSETS_EXPORTS,
         [copy_pictures(WEAPON_PACKAGES, "all_shared"), place_animations,
          py("build_assets.py", ASSETS_SRC, "{assets}")]),
    Step("pickups", "the weapon and equipment pickups", ASSETS_EXPORTS,
         [py("build_pickups.py", ASSETS_SRC, "{assets}")]),
    Step("customize", "the Customize and Choose Character screens", ASSETS_EXPORTS + UI_EXPORTS,
         [py("build_customize.py", "{transgame}", "{assets}", "{exports}/assets/Animations",
             "{exports}/ui/UI_Lobby_m/Texture2D", "{exports}/ui/UI_FrontEnd_m/Texture2D",
             "{exports}/ui/TransGame/Texture2D")]),
    Step("hud", "the HUD (its Flash movie's layout and pictures)", UI_EXPORTS,
         [py("build_hud.py", "{cooked}", "{exports}/ui/TransGame/Texture2D", "{assets}")]),
    Step("menu", "the menus (pictures, fonts, text, background and credits movies)", UI_EXPORTS,
         [py("build_menu.py", "{transgame}", "{exports}/ui/UI_FrontEnd_m/Texture2D", "{assets}",
             "{exports}/ui/mp_shared/Texture2D", "{exports}/ui/UI_Lobby_ESC_m/Texture2D")]),
    Step("robot_extras", "Jazz's extra clips (downed, revive, abilities, hit reactions) and sockets",
         [Export("extras", "all_shared.xxx", ["Shared_ROBO_ANIM"]),
          Export("extras", "mp_shared.xxx", ["Shared_ROBO_NAV_Downed_ANIM", "HitReactions_ROBO_ANIM"]),
          Export("extras", "MP_Array_BASE_m.xxx", ["Shared_SCOUT_ROBO_ANIM", "Breakdown_ROBO_ANIM",
                                                   "Titan_ROBO_ANIM"])],
         [py("build_robot_extras.py", "{exports}/extras", "{assets}", "{cooked}")]),
    Step("deaths", "the players' death clips",
         [Export("deaths", "all_shared.xxx", ["Death_Shared_ANIM"])],
         [py("build_deaths.py", "{exports}/deaths", "{cooked}", "{assets}")]),
    Step("melee", "the melee weapon",
         [Printed("melee", py("build_melee.py", "--umodel", "{cooked}", "{exports}/melee"))],
         [py("build_melee.py", "{cooked}", "{exports}/melee", "{assets}")]),
    Step("decals", "the vehicles' decal sets",
         [Printed("decals", py("build_decals.py", "--umodel", "{cooked}", "{exports}/decals"))],
         [py("build_decals.py", "{exports}/decals", "{cooked}", "{assets}")]),
    *[class_step(c) for c in CLASSES],
    Step("robot_fx", "the characters' own effects", [], [py("build_robot_fx.py", "{cooked}", "{transgame}", "{assets}")]),
    # (--umodel twice: the second time it also lists the textures of the meshes the
    # first exported.)
    Step("enemies", "the enemies' models, textures and clips", [ENEMY_EXPORT, ENEMY_EXPORT],
         [py("build_enemies.py", "{exports}/enemies", "{cooked}", "{assets}")]),
    Step("ai", "the enemies' AI data", [], [py("build_ai.py", "{cooked}", "{assets}")]),
    Step("ai_anims", "the enemies' animation trees and clips", [ENEMY_EXPORT, ENEMY_EXPORT],
         [py("build_ai_anims.py", "{exports}/enemies", "{cooked}", "{assets}")]),
    Step("death_sets", "the enemies' and players' death effects and pieces", [],
         [py("build_death_sets.py", "{cooked}", "{assets}")]),
    Step("weapon_fx", "the weapons' particle effects",
         [Printed("weapon_fx", py("build_weapon_fx.py", "--umodel", "{cooked}", "{exports}/weapon_fx"))],
         [py("build_weapon_fx.py", "{cooked}", "{exports}/weapon_fx", "{assets}")]),
    Step("sounds", "every sound (last: it reads what the others wrote for the cues they play)", [],
         [py("build_sounds.py", "{cooked}", "{assets}")]),
]


# ---------------------------------------------------------------------------
# Running
# ---------------------------------------------------------------------------

def steam_libraries():
    """Steam's library folders on this PC: where Steam is installed (from the
    registry) and the libraries its libraryfolders.vdf lists, then the usual
    places (STEAM_LIBRARIES)."""
    roots = []
    if os.name == "nt":
        try:
            import winreg
            for hive, key, value in [(winreg.HKEY_CURRENT_USER, r"Software\Valve\Steam", "SteamPath"),
                                     (winreg.HKEY_LOCAL_MACHINE, r"SOFTWARE\WOW6432Node\Valve\Steam", "InstallPath"),
                                     (winreg.HKEY_LOCAL_MACHINE, r"SOFTWARE\Valve\Steam", "InstallPath")]:
                try:
                    with winreg.OpenKey(hive, key) as k:
                        roots.append(os.path.normpath(winreg.QueryValueEx(k, value)[0]))
                except OSError:
                    pass
        except ImportError:
            pass
    libs = []
    for root in roots:
        libs.append(os.path.join(root, "steamapps", "common"))
        libs += [os.path.join(p, "steamapps", "common")
                 for p in library_paths(os.path.join(root, "steamapps", "libraryfolders.vdf"))]
    out = []
    for lib in libs + STEAM_LIBRARIES:
        if os.path.normcase(lib) not in {os.path.normcase(o) for o in out}:
            out.append(lib)
    return out


def library_paths(vdf):
    """The "path" entries of Steam's libraryfolders.vdf (its library folders)."""
    try:
        text = open(vdf, encoding="utf-8", errors="replace").read()
    except OSError:
        return []
    import re
    return [m.group(1).replace("\\\\", "\\") for m in re.finditer(r'"path"\s+"([^"]+)"', text)]


def find_game(given):
    candidates = [given] if given else [os.path.join(lib, GAME_DIR) for lib in steam_libraries()]
    for c in candidates:
        if c and os.path.isdir(os.path.join(c, "TransGame", "CookedPC")):
            return os.path.abspath(c)
    return None


def find_umodel(given):
    names = ["umodel_64.exe", "umodel.exe"] if os.name == "nt" else ["umodel64", "umodel"]
    if given:
        return os.path.abspath(given) if os.path.isfile(given) else None
    for folder in [HERE, os.path.join(HERE, "umodel"), ROOT, os.path.join(ROOT, "umodel")]:
        for n in names:
            p = os.path.join(folder, n)
            if os.path.isfile(p):
                return p
    for n in names:
        p = shutil.which(n)
        if p:
            return p
    return None


def check_python():
    missing = []
    for mod, pip in [("numpy", "numpy"), ("PIL", "pillow"), ("fontTools", "fonttools")]:
        try:
            __import__(mod)
        except ImportError:
            missing.append(pip)
    return missing


def split_command(line):
    """A printed command's words: split at spaces, "..." kept whole (no escapes, so
    Windows paths come through as they are)."""
    words, cur, quoted, any_ = [], [], False, False
    for ch in line:
        if ch == '"':
            quoted = not quoted
            any_ = True
        elif ch in " \t" and not quoted:
            if cur or any_:
                words.append("".join(cur))
            cur, any_ = [], False
        else:
            cur.append(ch)
    if cur or any_:
        words.append("".join(cur))
    return words


def umodel_targets(words):
    """What umodel export arguments ask for: (package, objects, class), objects []
    for the whole package, class only with a single object (<package> <object> <class>)."""
    pos = [w for w in words if not w.startswith("-")]
    objects = [w[5:] for w in words if w.lower().startswith("-obj=")]
    cls = None
    if len(pos) >= 2:
        objects = [pos[1]] + objects
        cls = pos[2] if len(pos) >= 3 else None
    return (pos[0] if pos else None), objects, cls


class Runner:
    def __init__(self, args, game, umodel):
        self.game = game
        self.umodel = umodel
        self.cooked = os.path.join(game, "TransGame", "CookedPC")
        self.assets = os.path.abspath(args.assets)
        self.work = os.path.abspath(args.work)
        self.exports = os.path.join(self.work, "exports")
        self.logs = os.path.join(self.work, "logs")
        self.keep = args.keep_exports
        os.makedirs(self.logs, exist_ok=True)
        os.makedirs(self.assets, exist_ok=True)
        self.done_path = os.path.join(self.work, "done.txt")
        self.done = []
        if os.path.exists(self.done_path) and not args.redo:
            self.done = [l.strip() for l in open(self.done_path) if l.strip()]
        elif os.path.exists(self.done_path):
            os.remove(self.done_path)
        self.env = dict(os.environ, PYTHONIOENCODING="utf-8", PYTHONUTF8="1")

    def fill(self, arg):
        return arg.format(cooked=self.cooked, transgame=os.path.join(self.game, "TransGame"), game=self.game,
                          assets=self.assets, exports=self.exports, tools=HERE, work=self.work).replace("/", os.sep)

    def run(self, cmd, log, capture=False, cwd=HERE):
        log.write("\n$ " + " ".join(f'"{c}"' if " " in c else c for c in cmd) + "\n")
        log.flush()
        if capture:
            p = subprocess.run(cmd, stdout=subprocess.PIPE, stderr=log, cwd=cwd, env=self.env)
            out = p.stdout.decode("utf-8", "replace")
            log.write(out)
            return p.returncode, out
        p = subprocess.run(cmd, stdout=log, stderr=subprocess.STDOUT, cwd=cwd, env=self.env)
        return p.returncode

    def umodel_run(self, folder, words, log):
        """Runs umodel with `words` (its arguments) into exports/<folder>; then, when
        it saved animation sets as .psa (no -gltf), writes those again with
        psa_write.py, with the FoC clips official umodel builds drop; then checks a
        file came out for every object named. A marker per command (its words)
        notes it done, so a later step or run doesn't do it again while the files
        it made are there."""
        out = os.path.join(self.exports, folder)
        os.makedirs(out, exist_ok=True)
        key = hashlib.md5("\0".join([EXPORT_VERSION] + words).encode()).hexdigest()[:16]
        marker = os.path.join(out, f".exported_{key}")
        package, objects, cls = umodel_targets(words)
        if os.path.exists(marker):
            with open(marker) as f:
                empty = set(f.read().split())
            missing = self.not_exported(folder, objects, empty)
            if not missing:
                log.write(f"\n(already exported: umodel {' '.join(words)})\n")
                return 0
            log.write(f"\n(exported before, but {', '.join(missing[:4])} isn't there: again)\n")
            os.remove(marker)
        # umodel reads a umodel.cfg from the folder it runs in: the work folder has none.
        code = self.run([self.umodel] + words, log, cwd=self.work)
        if code != 0:
            return code
        empty = []
        if "-gltf" not in words and "-md5" not in words and (cls is None or cls.lower() == "animset"):
            code, empty = self.write_animsets(out, package, objects, log)
            if code != 0:
                return code
        missing = self.not_exported(folder, objects, set(empty))
        if missing:
            log.write(f"\n! umodel wrote nothing for {', '.join(missing)}\n")
            return 1
        with open(marker, "w") as f:
            f.write("".join(n + "\n" for n in empty))
        return 0

    def not_exported(self, folder, objects, empty):
        """The objects named that have no file in exports/<folder> (but for the
        animation sets in `empty`, which have no clip to save)."""
        if not objects:
            return []
        have = set()
        for _d, _dirs, names in os.walk(os.path.join(self.exports, folder)):
            have.update(os.path.splitext(n)[0].lower() for n in names)
        return [o for o in objects if o.lower() not in have and o not in empty]

    def write_animsets(self, out, package, objects, log):
        """Writes the animation sets of `package` that umodel just exported (the
        objects named, or all of them) with psa_write.py, over umodel's own .psa
        files: returns (0 or 1, the sets with no clip to save)."""
        import psa_write
        path = os.path.join(self.cooked, package)
        if not os.path.isfile(path):
            found = [n for n in os.listdir(self.cooked) if n.lower() in (package.lower(), package.lower() + ".xxx")]
            path = os.path.join(self.cooked, found[0]) if found else None
        if path is None:
            log.write(f"\n! psa_write: no package {package} in {self.cooked}\n")
            return 1, []
        try:
            pkg = psa_write.Package(path)
            sets = sorted(e["name"] for e in pkg.exports if pkg.class_name(e) == "AnimSet")
            names = [o for o in objects if o in sets] if objects else sets
            if not names:
                return 0, []
            notes = []
            written = psa_write.write(path, out, names, pkg=pkg, log=notes.append)
        except Exception as ex:  # a set psa_write can't read: say so, don't go on without it
            log.write(f"\n! psa_write failed on {package}: {type(ex).__name__}: {ex}\n")
            return 1, []
        for n in notes:
            log.write(n + "\n")
        done = {os.path.splitext(os.path.basename(w))[0] for w in written}
        log.write(f"psa_write: {len(written)} animation set(s) of {package} written\n")
        return 0, [n for n in names if n not in done]

    def export(self, ex, log):
        if isinstance(ex, Printed):
            code, text = self.run([sys.executable] + [self.fill(a) for a in ex.args], log, capture=True)
            if code != 0:
                return code
            for line in text.splitlines():
                words = split_command(line.strip())
                if not words or words[0] != "umodel":
                    continue
                code = self.umodel_run(ex.folder, words[1:], log)
                if code != 0:
                    return code
            return 0
        out = os.path.join(self.exports, ex.folder)
        words = ["-game=trans", "-export", f"-path={self.cooked}", f"-out={out}"]
        if ex.gltf:
            words.append("-gltf")
        if ex.png:
            words.append("-png")
        words.append(ex.package)
        if ex.cls and len(ex.objects) == 1:
            words += [ex.objects[0], ex.cls]
        else:
            words += [f"-obj={o}" for o in ex.objects]
        return self.umodel_run(ex.folder, words, log)

    def step(self, s):
        path = os.path.join(self.logs, f"{s.name}.log")
        t0 = time.time()
        with open(path, "w", encoding="utf-8", errors="replace") as log:
            log.write(f"{s.name}: {s.what}\n")
            for ex in s.exports:
                code = self.export(ex, log)
                if code != 0:
                    log.write(f"\n! the export failed ({code})\n")
                    return False, time.time() - t0
            for c in s.commands:
                if callable(c):
                    code = c(self, log)
                else:
                    code = self.run([sys.executable] + [self.fill(a) for a in c], log)
                if code != 0:
                    log.write(f"\n! failed ({code})\n")
                    return False, time.time() - t0
            fixed = self.lf_endings(t0)
            if fixed:
                log.write(f"\n{fixed} text file(s) given LF line endings\n")
            log.write(f"\nok ({time.time() - t0:.0f} s)\n")
        return True, time.time() - t0

    def lf_endings(self, since):
        """Gives the text files written into the assets since `since` LF line endings,
        as the demo's own have (Python on Windows writes CRLF): the demo reads
        either, but a build then compares the same as the demo's own assets
        (compare_assets.py). How many it changed."""
        fixed = 0
        for d, _dirs, names in os.walk(self.assets):
            for n in names:
                if os.path.splitext(n)[1].lower() not in TEXT:
                    continue
                path = os.path.join(d, n)
                if os.path.getmtime(path) < since - 2:
                    continue
                with open(path, "rb") as f:
                    data = f.read()
                if b"\r\n" in data:
                    with open(path, "wb") as f:
                        f.write(data.replace(b"\r\n", b"\n"))
                    fixed += 1
        return fixed

    def tidy(self, later):
        """Deletes the export folders no step in `later` reads."""
        if self.keep or not os.path.isdir(self.exports):
            return
        wanted = set().union(*[s.folders() for s in later]) if later else set()
        for name in os.listdir(self.exports):
            if name not in wanted:
                shutil.rmtree(os.path.join(self.exports, name), ignore_errors=True)

    def mark(self, name, done=True):
        """Notes a step done (or, `done` False, not done: about to run again)."""
        self.done = [d for d in self.done if d != name] + ([name] if done else [])
        with open(self.done_path, "w") as f:
            f.write("".join(d + "\n" for d in self.done))


# The demo's own shaders (assets\shaders, written by hand, not built from the game),
# but for those the steps write (the effects' materials).
BUILT_SHADERS = {"levelfx.wgsl"}


def copy_own_shaders(assets):
    """Copies the demo's own shaders into `assets` when it isn't the demo's own assets
    folder (a build into another folder: --assets), so the folder runs as it is.
    Returns how many were copied (those already the same are left), or None if the
    demo's own folder has none to copy."""
    own = os.path.join(ROOT, "assets", "shaders")
    dest = os.path.join(assets, "shaders")
    if os.path.normcase(os.path.abspath(own)) == os.path.normcase(os.path.abspath(dest)):
        return 0
    names = sorted(n for n in os.listdir(own) if n.endswith(".wgsl") and n not in BUILT_SHADERS) \
        if os.path.isdir(own) else []
    if not names:
        return None
    os.makedirs(dest, exist_ok=True)
    copied = 0
    for n in names:
        src, out = os.path.join(own, n), os.path.join(dest, n)
        if os.path.exists(out) and open(src, "rb").read() == open(out, "rb").read():
            continue
        shutil.copy2(src, out)
        copied += 1
    return copied


def main():
    ap = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    ap.add_argument("--game")
    ap.add_argument("--umodel")
    ap.add_argument("--assets", default=os.path.join(ROOT, "assets"))
    ap.add_argument("--work", default=os.path.join(ROOT, "extract_work"))
    ap.add_argument("--from", dest="start")
    ap.add_argument("--only")
    ap.add_argument("--list", action="store_true")
    ap.add_argument("--redo", action="store_true")
    ap.add_argument("--keep-exports", action="store_true")
    args = ap.parse_args()

    if args.list:
        for i, s in enumerate(STEPS, 1):
            print(f"{i:2}. {s.name:16} {s.what}")
        return 0
    missing = check_python()
    if missing:
        print("Python is missing: " + ", ".join(missing) + "  (pip install " + " ".join(missing) + ")")
        return 1
    if shutil.which("ffmpeg") is None:
        print("ffmpeg isn't on the PATH (the sounds and the menu's movies need it): https://ffmpeg.org/download.html")
        return 1
    game = find_game(args.game)
    if not game:
        print("Can't find the game: give its folder with --game "
              "(the one with TransGame\\CookedPC in it).")
        return 1
    umodel = find_umodel(args.umodel)
    if not umodel:
        print("Can't find UE Viewer (umodel): give it with --umodel, or put umodel.exe in tools\\umodel.")
        return 1
    r = Runner(args, game, umodel)
    names = [s.name for s in STEPS]
    todo = STEPS
    if args.only:
        want = args.only.split(",")
        unknown = [w for w in want if w not in names]
        if unknown:
            print("No such step: " + ", ".join(unknown))
            return 1
        todo = [s for s in STEPS if s.name in want]
    elif args.start:
        if args.start not in names:
            print(f"No such step: {args.start}")
            return 1
        todo = STEPS[names.index(args.start):]
    else:
        todo = [s for s in STEPS if s.name not in r.done]
    print(f"Game:   {game}\numodel: {umodel}\nAssets: {r.assets}\nWork:   {r.work}\n")
    # (Built into another folder, it gets the demo's own shaders too.)
    copied = copy_own_shaders(r.assets)
    if copied is None:
        print(f"  ! no shaders in {os.path.join(ROOT, 'assets', 'shaders')} to copy: the demo needs its own"
              " shaders (*.wgsl) there to run\n")
    elif copied:
        print(f"  copied the demo's own shaders into {os.path.join(r.assets, 'shaders')} ({copied})\n")
    if not (args.only or args.start):
        for s in STEPS:
            if s.name in r.done:
                print(f"  done   {s.name}")
    t_all = time.time()
    for i, s in enumerate(todo):
        print(f"  ...    {s.name}: {s.what}", flush=True)
        r.mark(s.name, done=False)
        ok, secs = r.step(s)
        if not ok:
            print(f"  FAILED {s.name} after {secs:.0f} s: see {os.path.join(r.logs, s.name + '.log')}")
            print("Run it again to carry on from this step.")
            return 1
        r.mark(s.name)
        r.tidy(todo[i + 1:])
        print(f"  ok     {s.name} ({secs:.0f} s)", flush=True)
    print(f"\nAll done ({(time.time() - t_all) / 60:.0f} min).")
    return 0


if __name__ == "__main__":
    sys.exit(main())
