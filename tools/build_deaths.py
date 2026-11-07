"""Build the players' deaths: the clips a downed Jazz falls with and the ragdoll he
then falls as, from the game's data.

Every multiplayer class's robot (TR_Custom<...>_ROBODEF, the classes' chassis) dies
by the same death set as Escalation's soldiers, TR_Deaths_p.Sets.Basic_DEATHSET, and
falls as the same physics asset, TR_Shared_PHYSICS_p.RB_Shared_PHYS (class_data.py's
robot definitions; tools/enemy_data.py reads the death sets' and physics assets'
data). The death set takes the first of its deaths whose death types the kill has
(TnDeathPlayer.Pick): most of them are a ragdoll from the start, a few play a clip
first (Death_Shared_ANIM's, ROLES below) and let the robot fall from where it ends.

Writes into <assets>/models:

  * robot_deaths.glb: those clips on robot.glb's skeleton (its Root and bones, as
    they are, so the clips drive the robot model's bones);
  * robot_deaths.txt: what src/ragdoll.rs and main.rs read (tab-separated; glTF
    axes, metres, each bone's own space):
      clip  <role> <index> <seconds>        (role: the clip's name as the death set
                                             plays it: Death_Gunfire_Front (a chooser
                                             group: one at random), Death_Headshot03...)
      body  <bone> sphere|capsule|box|convex ...   (as build_enemies.py writes them)
      mass  <bone> <mass>                           (the same)
      joint <child> <parent> ...                    (the same)

usage: python build_deaths.py <umodel export dir> <CookedPC> <foc_esc/assets>

The export folder needs all_shared.xxx's Death_Shared_ANIM (umodel -game=trans
-export -path=<CookedPC> -out=<folder> all_shared.xxx -obj=Death_Shared_ANIM), as
build_enemies.py's does.
"""
import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.path.join(HERE, "ue3"))
sys.path.insert(0, HERE)

import build_class as bc  # noqa: E402
import build_enemies as be  # noqa: E402
import class_data as cd  # noqa: E402
import enemy_data  # noqa: E402
from gltf_io import Doc, Writer  # noqa: E402

PHYSICS = "TR_Shared_PHYSICS_p.RB_Shared_PHYS"
ANIM_SET = "Death_Shared_ANIM"
# The clips by role: build_enemies.py's NPC_DEATHS, which the soldiers' death set
# plays the same way (a chooser group's name, or the clips).
ROLES = be.NPC_DEATHS


def skeleton_writer(robot_glb):
    """A Writer with robot.glb's Root and skeleton (names, hierarchy, rest pose: what
    Bevy names a clip's targets by), and its nodes by name."""
    rg = Doc(robot_glb).g
    w = Writer(share=True)
    bones = {rg["nodes"][j]["name"] for j in rg["skins"][0]["joints"]}
    rid = next(i for i, n in enumerate(rg["nodes"]) if n.get("name") == "Root")
    keep = [rid] + [i for i, n in enumerate(rg["nodes"]) if n.get("name") in bones]
    new = {}
    for i in keep:
        n = rg["nodes"][i]
        new[i] = w.node({k: v for k, v in n.items() if k in ("name", "translation", "rotation", "scale")},
                        root=(i == rid))
    for i in keep:
        kids = [new[c] for c in rg["nodes"][i].get("children", []) if c in new]
        if kids:
            w.g["nodes"][new[i]]["children"] = kids
    return w, {w.g["nodes"][i]["name"]: i for i in new.values()}


def main(export_dir, cooked, assets):
    models = os.path.join(assets, "models")
    robot_glb = os.path.join(models, "robot.glb")
    ex = bc.Exports(export_dir)
    w, names = skeleton_writer(robot_glb)
    rest = bc.robot_rest(Doc(robot_glb))
    chooser = be.chooser_groups(cooked, ANIM_SET)
    lines = ["# The players' deaths (tools/build_deaths.py): see its header for the fields"]
    for role, clips in ROLES.items():
        for clip in chooser.get(clips, []) if isinstance(clips, str) else clips:
            bones, dt = bc.clip_tracks(ex, [ANIM_SET], clip)
            if bones is None:
                print("  ! clip not exported:", clip)
                continue
            length = bc.base_clip(w, names, clip, bones, dt, rest=rest, skip=())
            lines.append("\t".join(["clip", role, str(len(w.g["animations"]) - 1), "%.4g" % length]))
    size = w.save(os.path.join(models, "robot_deaths.glb"))
    print(f"wrote robot_deaths.glb ({len(w.g['animations'])} clips, {size / 1e6:.1f} MB)")
    mp = cd.Mp(cooked)
    pa = mp.find(PHYSICS)
    form = {"physics": enemy_data.physics(mp, pa), "joints": enemy_data.constraints(mp, pa)}
    for bone, shape, vals in be.bodies(form):
        if bone in names:
            lines.append("\t".join(["body", bone, shape] + [be.fmt(v) for v in vals]))
    for bone, mass in be.masses(form):
        if bone in names:
            lines.append("\t".join(["mass", bone, "%g" % mass]))
    for child, parent, frames, limits in be.joints(form):
        if child in names and parent in names:
            lines.append("\t".join(["joint", child, parent] + [be.fmt(v) for v in frames] + limits))
    open(os.path.join(models, "robot_deaths.txt"), "w").write("\n".join(lines) + "\n")
    print("wrote models/robot_deaths.txt")


if __name__ == "__main__":
    main(sys.argv[1], sys.argv[2], sys.argv[3])
