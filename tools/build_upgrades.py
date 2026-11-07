"""The weapon upgrades' parts (src/upgrades.rs), read from the game for src/character.rs.

Each upgrade the multiplayer game offers has a part that goes on its gun as it is
installed (TnWeapon.InstallModification -> TnWeaponMesh.InstallMeshAttachmentModification):
its TnWeaponUpgrade's StaticMeshAttachment, a WEP_WepUpgrades_p static mesh, on the
gun's weapon mesh at the socket its MeshAttachmentSocket names (every gun's right-arm
and left-arm meshes have the sockets of their upgrades, most on the CloudA bones of
the gun's moving parts, turned and sized there - often mirrored, a negative scale),
painted in the character's colours (ApplyColorsToMesh: its materials are texture set
materials as the guns' are).

usage: python build_upgrades.py <TransGame\\CookedPC> <umodel export folder> <foc_esc\\assets>

The parts' meshes come from umodel's glTF export of all_shared.xxx (one command a
part, PARTS below):

    umodel -game=trans -export -gltf -path=<CookedPC> -out=<folder> all_shared.xxx WEP_LargerClip_STAT

their texture sets (WEP_WepUpgradesPhase01, GenUnique, Special01 and Special02) at
full size from WEP_WepUpgrades_p.xxx, the guns' sockets from their skeletal meshes
in all_shared.xxx. It writes:

  * models/weapons/upgrades.glb - each part's mesh as a node of its own, named after
    it, in its texture sets' materials (with the paint mask: src/character.rs paints
    them in the look's colours), their maps in models/weapons/upgrades/ (KTX2)
  * models/weapons/upgrades.txt - where each upgrade's part goes on its gun, one line
    for the gun's right-arm mesh and one for its left:

      part <upgrade id> <weapon mesh> <part mesh> <bone> <tx ty tz> <qx qy qz qw> <sx sy sz>

    in umodel's glTF axes (Unreal's Y and Z swapped), metres, on the bone (as the
    class files' socket lines: build_class.gltf_socket)
"""
import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
sys.path.insert(0, os.path.join(HERE, "ue3"))
import build_class  # noqa: E402
import class_data  # noqa: E402
from gltf_io import Doc, Writer  # noqa: E402

# The demo's guns' weapon meshes (src/main.rs's WEAPONS: mesh, mesh_left), by the
# weapon's id (its TnDataProvider_Weapon's UniqueId, what an upgrade's
# WeaponRestriction names).
WEAPON_MESHES = {
    "AssaultRifle": ("RB_AssaultRifleR_SKEL", "RB_AssaultRifleL_SKEL"),
    "IonBlaster": ("RB_IonBlasterR_SKEL", "RB_IonBlasterL_SKEL"),
    "BurstRifle": ("RB_BurstRifleR_SKEL", "RB_BurstRifleL_SKEL"),
    "Shotgun": ("RB_ScatterblasterR_SKEL", "RB_ScatterblasterL_SKEL"),
    "SniperRifle": ("RB_SniperRifleR_SKEL", "RB_SniperRifleL_SKEL"),
    "HeavyPistol": ("RB_HeavyPistolR_SKEL", "RB_HeavyPistolL_SKEL"),
    "Bazooka": ("RB_BazookaR_SKEL", "RB_BazookaL_SKEL"),
    "TracerDart": ("RB_SysCorruptorR_SKEL", "RB_SysCorruptorL_SKEL"),
    "HeavyMG": ("RB_HeavyMGR_SKEL", "RB_HeavyMGL_SKEL"),
    "DrainGun": ("RB_EmpShotgunR_SKEL", "RB_EmpShotgunL_SKEL"),
    "ChainBeam": ("RB_RepairBeamR_SKEL", "RB_RepairBeamL_SKEL"),
    "AoESlow": ("RB_RustCannonR_SKEL", "RB_RustCannonL_SKEL"),
    "SawBlade": ("RB_SawbladeR_SKEL", "RB_SawbladeL_SKEL"),
    "HomingRocket": ("RB_RocketLauncherR_SKEL", "RB_RocketLauncherL_SKEL"),
    "GrenadeLauncher": ("RB_GrenadeLauncherR_SKEL", "RB_GrenadeLauncherL_SKEL"),
    "ClusterCannon": ("RB_GrenadeArmR_SKEL", "RB_GrenadeArmL_SKEL"),
}

# Every upgrade the demo has (src/upgrades.rs's UPGRADES): its UniqueId, its gun,
# its part (StaticMeshAttachment) and the socket it goes on (MeshAttachmentSocket),
# from its TnWeaponUpgrade class's defaults in TransGame.xxx.
UPGRADES = [
    ("SniperRifleHitsCharge", "SniperRifle", "WEP_SniperRifleHitsSpeedUpCharge_STAT", "SniperRifleHitsSpeedUpCharge"),
    ("SniperRifleFastReload", "SniperRifle", "WEP_FastReload_STAT", "FastReload"),
    ("SniperRifleLargerClip", "SniperRifle", "WEP_LargerClip_STAT", "LargerClip"),
    ("TracerDartIncreaseDuration", "TracerDart", "WEP_IncreasedHardLockDuration_STAT", "IncreasedHardLockDuration"),
    ("TracerDartFireInterval", "TracerDart", "WEP_FireInterval_STAT", "FireInterval"),
    ("TracerDartIncreaseDamage", "TracerDart", "WEP_ImprovedHardLock_STAT", "IncreasedHardLockDuration"),
    ("AoESlowProjectileRadius", "AoESlow", "WEP_ProjectileDamageRadius_STAT", "ProjectileDamageRadius"),
    ("AoESlowFireInterval", "AoESlow", "WEP_FireInterval_STAT", "FireInterval"),
    ("AoESlowIncreaseDuration", "AoESlow", "WEP_PuddlesLingerLonger_STAT", "PuddlesLingerLonger"),
    ("DrainGunRange", "DrainGun", "WEP_Range_STAT", "Range"),
    ("DrainGunLargerClip", "DrainGun", "WEP_LargerClip_STAT", "LargerClip"),
    ("DrainGunIncreaseDrain", "DrainGun", "WEP_DrainGunIncreaseDrain_STAT", "DrainGunIncreaseDrain"),
    ("AssaultRifleLargerClip", "AssaultRifle", "WEP_LargerClip_STAT", "LargerClip"),
    ("AssaultRifleAccuracy", "AssaultRifle", "WEP_Spread_STAT", "Spread"),
    ("AssaultRifleReduceRecoil", "AssaultRifle", "WEP_ReducedRecoil_STAT", "ReducedRecoil"),
    ("IonBlasterIncreaseAccuracy", "IonBlaster", "WEP_Spread_STAT", "Spread"),
    ("IonBlasterLargerClip", "IonBlaster", "WEP_LargerClip_STAT", "LargerClip"),
    ("IonBlasterFastReload", "IonBlaster", "WEP_FastReload_STAT", "FastReload"),
    ("HeavyPistolSpreadCooldown", "HeavyPistol", "WEP_SpreadCooldown_STAT", "SpreadCooldown"),
    ("HeavyPistolFireInterval", "HeavyPistol", "WEP_FireInterval_STAT", "FireInterval"),
    ("HeavyPistolLastShotInClip", "HeavyPistol", "WEP_LastShotInClipSpecial_STAT", "LastShotInClipSpecial"),
    ("BurstRifleLargerClip", "BurstRifle", "WEP_LargerClip_STAT", "LargerClip"),
    ("BurstRifleShowInfo", "BurstRifle", "WEP_BurstRifleShowInfo_STAT", "BurstRifleShowInfo"),
    ("BurstRifleAutoFire", "BurstRifle", "WEP_AutoFire_STAT", "AutoFire"),
    ("HeavyMGIncreaseAccuracy", "HeavyMG", "WEP_Spread_STAT", "Spread"),
    ("HeavyMGLargerClip", "HeavyMG", "WEP_LargerClip_STAT", "LargerClip"),
    ("HeavyMGWindUpSpeed", "HeavyMG", "WEP_ReduceWindUpSpeed_STAT", "ReduceWindUpSpeed"),
    ("GrenadeLauncherLargerClip", "GrenadeLauncher", "WEP_LargerClip_STAT", "LargerClip"),
    ("GrenadeLauncherProjectileSpeed", "GrenadeLauncher", "WEP_ProjectileSpeedIncrease_STAT", "ProjectileSpeedIncrease"),
    ("GrenadeLauncherProjectileRadius", "GrenadeLauncher", "WEP_ProjectileDamageRadius_STAT", "ProjectileDamageRadius"),
    ("BazookaFireInterval", "Bazooka", "WEP_FireInterval_STAT", "FireInterval"),
    ("BazookaLargerClip", "Bazooka", "WEP_LargerClip_STAT", "LargerClip"),
    ("BazookaLastShotInClip", "Bazooka", "WEP_LastShotInClipSpecial_STAT", "LastShotInClipSpecial"),
    ("ChainBeamLargerClip", "ChainBeam", "WEP_LargerClip_STAT", "LargerClip"),
    ("ChainBeamFiringGroundSpeed", "ChainBeam", "WEP_FiringGroundSpeed_STAT", "FiringGroundSpeed"),
    ("ChainBeamRange", "ChainBeam", "WEP_Range_STAT", "Range"),
    ("SawBladeChargeTime", "SawBlade", "WEP_SawbladeChargeTime_STAT", "SawbladeChargeTime"),
    ("SawBladeFastReload", "SawBlade", "WEP_FastReload_STAT", "FastReload"),
    ("SawBladeFineAimFootSpeed", "SawBlade", "WEP_IncreaseFineAimFootSpeed_STAT", "IncreaseFineAimFootSpeed"),
    ("ShotgunLargerClip", "Shotgun", "WEP_LargerClip_STAT", "LargerClip"),
    ("ShotgunFireInterval", "Shotgun", "WEP_FireInterval_STAT", "FireInterval"),
    ("ShotgunRandomSpecial", "Shotgun", "WEP_RandomShotSpecial_STAT", "RandomShotSpecial"),
    ("HomingRocketReserveAmmo", "HomingRocket", "WEP_ReserveAmmo_STAT", "ReserveAmmo"),
    ("HomingRocketFastReload", "HomingRocket", "WEP_FastReload_STAT", "FastReload"),
    ("HomingRocketLockOnTime", "HomingRocket", "WEP_WeaponLockOnTime_STAT", "WeaponLockOnTime"),
    ("ClusterCannonFastReload", "ClusterCannon", "WEP_FastReload_STAT", "FastReload"),
    ("ClusterCannonReserveAmmo", "ClusterCannon", "WEP_ReserveAmmo_STAT", "ReserveAmmo"),
    ("ClusterCannonNumClusters", "ClusterCannon", "WEP_NumClusters_STAT", "NumClusters"),
]
PARTS = sorted({part for _, _, part, _ in UPGRADES})

# Where the parts' materials' texture sets are at full size (then all_shared's mips).
TEXTURE_PACKAGES = ("WEP_WepUpgrades_p.xxx", "all_shared.xxx")


class Export:
    """umodel's export folder: finds a file by its class folder and name."""

    def __init__(self, root):
        self.files = {}
        for d, _dirs, files in os.walk(root, followlinks=True):
            kind = os.path.basename(d)
            for f in files:
                self.files.setdefault((kind, f), os.path.join(d, f))

    def find(self, kind, name):
        return self.files.get((kind, name))


def main(cooked, export, assets):
    ex = Export(export)
    mp = class_data.Mp(cooked, "all_shared.xxx")
    out = os.path.join(assets, "models", "weapons")
    os.makedirs(out, exist_ok=True)

    # The parts' meshes, in their materials (the texture sets at full size, with
    # the paint mask: build_class.Textures, as the class parts' are).
    build_class.WEAPON_PACKAGES = TEXTURE_PACKAGES
    textures = build_class.Textures(ex, os.path.join(out, "textures"), "", cooked)
    w = Writer(share=True, image_dir=os.path.join(out, "upgrades"), image_uri="upgrades")
    game_mats = {}
    docs = {}
    for part in PARTS:
        path = ex.find("StaticMesh3", part + ".gltf")
        if not path:
            print("  ! part not exported:", part)
            continue
        d = Doc(path)
        docs[part] = d
        for p in d.g["meshes"][0]["primitives"]:
            name = d.g["materials"][p["material"]]["name"]
            if name not in game_mats:
                info = mp.material(name)
                game_mats[name] = info
                if info.get("texset"):
                    # (Read now, from the packages: the Materials take them as made.)
                    textures.texset(info["texset"], cooked=True)
    mats = build_class.Materials(w, textures, game_mats)
    for part, d in docs.items():
        prims = build_class.primitives(w, mats, d, d.g["nodes"][0]["mesh"], part)
        w.g["meshes"].append({"name": part, "primitives": prims})
        w.node({"name": part, "mesh": len(w.g["meshes"]) - 1}, root=True)
    size = w.save(os.path.join(out, "upgrades.glb"))
    print(f"wrote models/weapons/upgrades.glb ({len(docs)} parts, {len(w.g['materials'])} materials, "
          f"{size / 1e6:.1f} MB)")

    # Where they go: each gun's sockets, right arm and left.
    lines = ["# The weapon upgrades' parts on the guns (tools/build_upgrades.py): "
             "part <upgrade> <weapon mesh> <part mesh> <bone> <t> <q> <s>"]
    for upgrade, weapon, part, socket in UPGRADES:
        for mesh in WEAPON_MESHES[weapon]:
            idx = next((i for full, i in mp.by_full.items() if full.endswith("." + mesh)), 0)
            if not idx:
                print("  ! weapon mesh not found:", mesh)
                continue
            s = next((s for s in mp.sockets(idx) if s["name"].lower() == socket.lower()), None)
            if s is None:
                print(f"  ! {mesh} has no socket {socket} for {upgrade}")
                continue
            t, q, sc = build_class.gltf_socket(s)
            lines.append("\t".join(["part", upgrade, mesh, part, s["bone"], build_class.fmt(t),
                                    build_class.fmt(q), build_class.fmt(sc)]))
    with open(os.path.join(out, "upgrades.txt"), "w", encoding="utf-8") as fh:
        fh.write("\n".join(lines) + "\n")
    print(f"wrote models/weapons/upgrades.txt ({len(lines) - 1} parts on guns)")


if __name__ == "__main__":
    if len(sys.argv) != 4:
        sys.exit(__doc__)
    main(*sys.argv[1:])
