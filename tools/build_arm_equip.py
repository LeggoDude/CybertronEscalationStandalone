"""Each class's forearm sets' clips for putting the gun away and taking it out, as the
game plays them while a robot sprints (or gives a revive).

The game puts the gun away while its robot sprints (TnWeapon.ShouldHolster: its
acrobatics' Sprinting state, or giving a revive; TnWeapon.Active.Tick holsters and
unholsters by it). The gun plays its UnEquip effects (its Unequip_WEP clip on the
CloudB gun bones, its holster sound) and is hidden after its DetachDelay; taken out,
it is shown and plays its Equip effects. The character's forearm parts follow the
gun (TnArmAttachmentManager.Tick, by GetVisibleWeapon, none while the gun is
holstered): with the gun gone, the forearm on its arm shows and plays the forearm
set's ADD_Weapon_Equip_ARM, unfolding out of the elbow on the CloudA arm bones;
with the gun back, the forearm moves to the CloudB arm bones (RemapParentBoneMap)
and plays ADD_Weapon_Unequip_ARM, folding away, and is hidden once it has
(OnAnimationComplete). Both clips' TnAnimNotify_ScaleBone notifies shrink the pieces
to nothing or grow them back, as ADD_Weapon_MirrorReverse_ARM's do when switching
hands (build_class.py), and the shoulder pads play their
ADD_Weapon_Arm_Shoulder_EquipUnequip_ARM as for a gun change (build_class.py's
clips.glb has those).

This writes, under <assets>/models/<class>, beside build_class.py's files (which it
leaves as they are):

  arm_equip.glb         each forearm set's ADD_Weapon_Equip_ARM and
                        ADD_Weapon_Unequip_ARM (additive, robot.glb's skeleton: the
                        arms' bones, as build_class.py keeps for the switch, and the
                        CloudA arm bones Equip_ARM unfolds the forearm on),
                        two a set in the order <class>.txt lists its armset lines
  arm_equip_mirror.glb  the same mirrored left for right (mirror_anims.py), for the
                        gun in the left hand
  arm_equip.txt         armequip <set> <equip clip> <unequip clip>
                        equipscale <set> <bone> <time> <from> <to> <ramp>
                        unequipscale <set> <bone> <time> <from> <to> <ramp>
                        (the clips' ScaleBone notifies, as build_class.py's switch
                        lines: seconds, sizes and the ramp's seconds)

usage: python build_arm_equip.py <class> <umodel export folder> <CookedPC> <assets>
                                 [<export with RB_Jazz_CHARSKEL_Legs>]

The export folder is the one build_class.py was given for the class (its forearm
sets' animation sets, <set>.psa, exported whole). The robot skeleton is taken from
Jazz's legs (any export of RB_Jazz_CHARSKEL_Legs.gltf: every class's parts share
it), from the last argument when the class's own export lacks it.
"""
import os
import sys

import build_class as bc
import class_data
import mirror_anims
from gltf_io import Doc  # noqa: F401  (build_class's writer reads it)

CLIPS = (("ADD_Weapon_Equip_ARM", "equip"), ("ADD_Weapon_Unequip_ARM", "unequip"))


def arm_sets(txt):
    """A class's forearm sets, in its <class>.txt's order."""
    with open(txt, encoding="utf-8") as fh:
        return [line.rstrip("\n").split("\t")[1] for line in fh if line.startswith("armset\t")]


def main(cls, export_dir, cooked, assets, legs_export=None):
    out = os.path.join(assets, "models", cls)
    sets = arm_sets(os.path.join(out, f"{cls}.txt"))
    if not sets:
        raise SystemExit(f"{cls}.txt has no forearm sets")
    ex = bc.Exports(export_dir)
    ref_path = ex.find("SkeletalMesh3", "RB_Jazz_CHARSKEL_Legs.gltf")
    if not ref_path and legs_export:
        ref_path = bc.Exports(legs_export).find("SkeletalMesh3", "RB_Jazz_CHARSKEL_Legs.gltf")
    if not ref_path:
        raise SystemExit("no export of RB_Jazz_CHARSKEL_Legs.gltf to take the robot skeleton from")
    robot_glb = os.path.join(assets, "models", "robot.glb")
    w, names = bc.robot_writer(robot_glb, ref_path)
    # (The arms' bones, as build_class.py's forearm switch keeps, not the spine's
    # and legs', which would fight the running and jumping; with the CloudA arm
    # bones, which ADD_Weapon_Equip_ARM unfolds the forearm on.)
    arm_bones = lambda b: b[:2] in ("L_", "R_") and not any(  # noqa: E731
        k in b for k in ("_Leg", "Knee", "_Face"))
    packages = [class_data.Mp(cooked)]
    if os.path.exists(os.path.join(cooked, class_data.SHARED_PACKAGE)):
        packages.append(class_data.Mp(cooked, class_data.SHARED_PACKAGE))
    lines = [f"# {cls}: its forearm sets' gun-away and gun-back clips, made by tools/build_arm_equip.py"]
    index = 0
    for s in sets:
        ids = []
        for clip_name, kind in CLIPS:
            bones_, dt = bc.clip_tracks(ex, [s], clip_name)
            if bones_ is None:
                print("  ! no", clip_name, "in", s)
                bc.empty_additive(w, names, f"{s}_{kind}")
            else:
                bc.additive_clip(w, names, f"{s}_{kind}", bones_, dt, keep=arm_bones)
            ids.append(index)
            index += 1
            seq = None
            for pkg in packages:
                seq = pkg.sequence(s, clip_name)
                if seq:
                    break
            if not seq:
                print("  ! no notifies for", clip_name, "of", s)
                continue
            for e in pkg.scale_bones(seq):
                lines.append("\t".join([f"{kind}scale", s, e["bone"], f"{e['time']:.4f}",
                                        f"{e['start']:.4f}", f"{e['end']:.4f}", f"{e['ramp']:.4f}"]))
        lines.append("\t".join(["armequip", s, str(ids[0]), str(ids[1])]))
    path = os.path.join(out, "arm_equip.glb")
    size = w.save(path)
    print(f"wrote {path} ({index} clips, {size / 1e6:.2f} MB)")
    size, n = mirror_anims.mirror(path, ref_path, os.path.join(out, "arm_equip_mirror.glb"),
                                  lambda i, name: True)
    print(f"wrote arm_equip_mirror.glb ({n} mirrored clips, {size / 1e6:.2f} MB)")
    with open(os.path.join(out, "arm_equip.txt"), "w", encoding="utf-8") as fh:
        fh.write("\n".join(lines) + "\n")
    print(f"wrote arm_equip.txt ({len(lines)} lines)")


if __name__ == "__main__":
    if len(sys.argv) not in (5, 6):
        sys.exit(__doc__)
    main(*sys.argv[1:])
