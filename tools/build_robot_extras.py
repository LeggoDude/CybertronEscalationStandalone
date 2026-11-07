"""Build the robot's extra clips: clips a Jazz plays over his own (src/main.rs's
animate_jazz) that robot.glb doesn't have, on its skeleton.

  * trigger: Nav_ToStealth (all_shared.xxx's Shared_ROBO_ANIM), the support items'
    use (TnAbility's OnTriggerAnimParams: AnimName Nav_ToStealth, MaskType
    CAMT_UpperBody, BlendInTime 0.2, BlendOutTime 0.4), his upper body's bones only
    (from C_Spine01_Lumbar01_XB up: the arms, the chest, the head), so that his legs
    go on as they were.
  * Escalation's downed robot (mp_shared.xxx's TR_Iconic_Car_ANM_p.
    Shared_ROBO_NAV_Downed_ANIM, src/revive.rs), whole body: going down
    (Revive_Fall), lying (Nav_Idle_Downed), his aim lying there
    (Aim_Downed_<F/L/R>_<C/D/U>: ahead, left and right, level, down and up), his
    turns on the spot lying (Nav_Downed_IdlePivot90L/R), getting up once revived
    (Revive_Getup) and reviving another (Revive_Heal).

  * The classes' abilities (src/abilities.rs): the Whirlwind's spin
    (Transform_Whirlwind_ROBO, the multiplayer's MPMeleeSet_Titan: upper body), as
    the Titan plays it: the one in his robot definition's animation sets,
    MP_Array_BASE_m.xxx's TR_Iconic_Melee_ANM_p.Titan_ROBO_ANIM (CustomSoldier_ROBODEF
    has no Shared_SCOUT_ROBO_ANIM, whose own 5.9 s spin is the Infiltrator's set's):
    1.67 s, drawing his blade and winding up, then going round and round from its
    LoopOffsetTime, 1.36 s (LOOPS: the clip is written with the pose there added at
    its end, where it goes back to it), the Heal Beam's arm
    (Skill_Heal, CAMT_LeftArm: the left arm's bones alone, from L_Arm01_Clav_XB:
    1.03 s, his hand coming up, then held up from its LoopOffsetTime, 0.54 s, round
    and round to its end, LOOPS as the spin is), the Flak Shield's stance
    (Shared_SCOUT_ROBO_ANIM's Nav_Shield_Idle, its turns on the spot and its jog
    and walk in ten directions, whole body, looping), the shield breaking
    (HitReaction_Shield_Break: the game's HitReactions_React_EnergonShield_Stand_R
    isn't cooked into anything umodel reads) and its bash (Melee_ShieldBash), both
    upper body, and the Hover Slam's hover (Nav_Hover, MP_Array's
    Breakdown_ROBO_ANIM: the hover on the car robots' skeleton, whole body,
    looping).

  * His reaction to an enemy's melee blow (src/hitreact.rs): Escalation's melee set
    (TR_Shared_MS_p.SharedMeleeSet, every Escalation character's) reacts with
    mp_shared.xxx's HitReactions_ROBO_ANIM.ADD_Hit_Reaction_Melee_FL, _FR and _B
    (by the side the blow came from), on CAMT_UpperBody. They are additive clips
    (bIsAdditive, made against Nav_Idle_Shoot): written as such, the turns to put
    in front of his bones' and the offsets to add to them, his upper body's bones
    only, for main.rs to add over what he plays.

Writes into <assets>/models:

  * robot_extras.glb: those clips on robot.glb's skeleton (its Root and bones, as
    they are, so the clips drive the robot model's bones);
  * robot_extras.txt: what main.rs reads (tab-separated):
      clip    <role> <index> <seconds> <upper|full|left|add>
    ("add": an additive clip over his upper body)
      socket  <name> <bone> <tx ty tz> <qx qy qz qw> <sx sy sz>
    the sockets (glTF axes, metres) being the robot skeleton's
    (all_shared.xxx's TR_Skeletons_p.RB_Iconic_SKEL): where the abilities' effects
    and the shield go (BoosterSocket_L/R, Stabilizer_*, MeleeSocket_*,
    ShieldSocket_L/R and the rest), when the CookedPC folder is given; and then the
    robot anim tree's aim offset node (MP_Array_BASE_m.xxx's TR_Shared_ANIMTREE_p.
    Robot_ANIMTREE: its TnAnimNodeAimOffset, which turns his upper body and gun arm
    to where he aims, src/robot_aim.rs), as build_ai_anims.py writes an enemy's:
      aimoffset <class> <interp speed> <switch when left-handed 0/1>
      aoprofile <index> <name> <horizontal min max> <vertical min max>
      aoremap <profile> <horizontal min max> <vertical min max>
      aobone <profile> <bone> <LU> <LC> <LD> <CU> <CC> <CD> <RU> <RC> <RD>
      aostance <stance> <profile>

usage: python build_robot_extras.py <umodel export dir> <foc_esc/assets> [<CookedPC>]

The export folder needs all_shared.xxx's Shared_ROBO_ANIM, mp_shared.xxx's
Shared_ROBO_NAV_Downed_ANIM and HitReactions_ROBO_ANIM and MP_Array_BASE_m.xxx's
Shared_SCOUT_ROBO_ANIM,
Breakdown_ROBO_ANIM and Titan_ROBO_ANIM as .psa files (umodel -game=trans -export -path=<CookedPC>
-out=<folder> all_shared.xxx -obj=Shared_ROBO_ANIM, and the same for the others).
"""
import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.path.join(HERE, "ue3"))
sys.path.insert(0, HERE)

import build_class as bc  # noqa: E402
from build_deaths import skeleton_writer  # noqa: E402
from gltf_io import Doc  # noqa: E402

# (role, anim set, clip, its bones: True the upper body's, False all, "left" the
# left arm's, "add" the upper body's, additive; looping)
DOWNED = "Shared_ROBO_NAV_Downed_ANIM"
SCOUT = "Shared_SCOUT_ROBO_ANIM"
TITAN = "Titan_ROBO_ANIM"
# Clips that go back, past their end, to a time of their own (High Moon's
# AnimSequence.LoopOffsetTime): the Titan's spin, round and round from 1.36 s, and
# the Heal Beam's arm, held up from 0.54 s (Skill_Heal: going back to its start, as
# a clip closed on its first pose does, took his hand down to his waist every
# second).
LOOPS = {"whirlwind": 1.36, "heal_beam": 0.54}
SHIELD_DIRS = ("F", "FL", "FR", "L", "R", "B", "BL", "BR", "LB", "RB")
CLIPS = [
    ("trigger", "Shared_ROBO_ANIM", "Nav_ToStealth", True),
    ("downed_fall", DOWNED, "Revive_Fall", False),
    ("downed_idle", DOWNED, "Nav_Idle_Downed", False),
] + [
    (f"downed_aim_{h.lower()}_{v.lower()}", DOWNED, f"Aim_Downed_{h}_{v}", False)
    for h in ("F", "L", "R") for v in ("C", "D", "U")
] + [
    ("downed_pivot_l", DOWNED, "Nav_Downed_IdlePivot90L", False),
    ("downed_pivot_r", DOWNED, "Nav_Downed_IdlePivot90R", False),
    ("downed_getup", DOWNED, "Revive_Getup", False),
    ("revive_heal", DOWNED, "Revive_Heal", False),
    ("whirlwind", TITAN, "Transform_Whirlwind_ROBO", True, False),
    ("heal_beam", "Shared_ROBO_ANIM", "Skill_Heal", "left", False),
    ("shield_idle", SCOUT, "Nav_Shield_Idle", False, True),
    ("shield_pivot_l", SCOUT, "Nav_Shield_IdlePivot90_L", False),
    ("shield_pivot_r", SCOUT, "Nav_Shield_IdlePivot90_R", False),
] + [
    (f"shield_{gait}_{d.lower()}", SCOUT, f"Nav_Strafe{name}_Shield_{d}", False, True)
    for gait, name in (("jog", "Jog"), ("walk", "Walk")) for d in SHIELD_DIRS
] + [
    ("shield_break", "Shared_ROBO_ANIM", "HitReaction_Shield_Break", True),
    ("shield_bash", "Shared_ROBO_ANIM", "Melee_ShieldBash", True),
    ("hover", "Breakdown_ROBO_ANIM", "Nav_Hover", False, True),
] + [
    (f"melee_react_{d.lower()}", "HitReactions_ROBO_ANIM", f"ADD_Hit_Reaction_Melee_{d}", "add")
    for d in ("FL", "FR", "B")
]
# CAMT_UpperBody's branch: this bone and everything under it; CAMT_LeftArm's.
UPPER_BODY = "C_Spine01_Lumbar01_XB"
LEFT_ARM = "L_Arm01_Clav_XB"
# The robot skeleton whose sockets the abilities use (all_shared.xxx).
SKELETON = ("all_shared.xxx", "TR_Skeletons_p.RB_Iconic_SKEL")
# The robots' anim tree (every class's: its aim offset node).
ANIM_TREE = ("MP_Array_BASE_m.xxx", "TR_Shared_ANIMTREE_p.Robot_ANIMTREE")


def upper_bones(robot_glb, top=UPPER_BODY):
    """The names of `top` and every bone under it in robot.glb."""
    g = Doc(robot_glb).g
    by_name = {n.get("name"): i for i, n in enumerate(g["nodes"])}
    out, todo = set(), [by_name[top]]
    while todo:
        i = todo.pop()
        out.add(g["nodes"][i]["name"])
        todo.extend(g["nodes"][i].get("children", []))
    return out


def looped_back(bones, dt, at):
    """A clip's tracks with one more frame at their end: the pose `at` seconds in
    (between its frames), where a clip with a LoopOffsetTime goes back to as it
    reaches its end, so that it plays on into it as the game's does."""
    import numpy as np
    out = {}
    for bone, (pos, rot) in bones.items():
        pos, rot = np.asarray(pos, np.float32), np.asarray(rot, np.float32)

        def at_time(v, slerp):
            if len(v) == 1:
                return v[0]
            f = min(at / dt, len(v) - 1.0)
            i = min(int(f), len(v) - 2)
            k = np.float32(f - i)
            a, b = v[i], v[i + 1]
            if not slerp:
                return a + (b - a) * k
            if float(np.dot(a, b)) < 0.0:
                b = -b
            q = a + (b - a) * k
            return q / max(float(np.linalg.norm(q)), 1e-8)

        p = pos if len(pos) == 1 else np.concatenate([pos, at_time(pos, False)[None]])
        r = rot if len(rot) == 1 else np.concatenate([rot, at_time(rot, True)[None]])
        out[bone] = (p, r)
    return out


def socket_lines(cooked):
    """The robot skeleton's sockets as robot_extras.txt's socket lines."""
    from class_data import Mp
    pkg, full = SKELETON
    mp = Mp(cooked, pkg)
    idx = mp.find(full)
    if not idx:
        print("  ! no", full, "in", pkg)
        return []
    lines = []
    for s in sorted(mp.sockets(idx), key=lambda s: s["name"]):
        t, q, sc = bc.gltf_socket(s)
        lines.append("\t".join(["socket", s["name"], s["bone"], bc.fmt(t), bc.fmt(q), bc.fmt(sc)]))
    return lines


def aim_offset_lines(cooked):
    """The robot anim tree's aim offset node as robot_extras.txt's lines (its
    profiles' bones' offsets as the game baked them from their aim poses)."""
    from build_ai_anims import aim_offset_lines as node_lines
    from class_data import Mp
    pkg, tree = ANIM_TREE
    lines = node_lines(Mp(cooked, pkg), tree)
    if not lines:
        print("  ! no aim offset node in", tree)
    return lines


def main(export_dir, assets, cooked=None):
    models = os.path.join(assets, "models")
    robot_glb = os.path.join(models, "robot.glb")
    ex = bc.Exports(export_dir)
    w, names = skeleton_writer(robot_glb)
    rest = bc.robot_rest(Doc(robot_glb))
    masks = {True: upper_bones(robot_glb), "left": upper_bones(robot_glb, LEFT_ARM)}
    masks["add"] = masks[True]
    lines = ["# The robot's extra clips (tools/build_robot_extras.py): see its header"]
    for role, anim_set, clip, part, *looping in CLIPS:
        bones, dt = bc.clip_tracks(ex, [anim_set], clip)
        if bones is None:
            print("  ! clip not exported:", clip)
            continue
        if part in masks:
            keep = masks[part]
            bones = {b: v for b, v in bones.items() if b in keep}
            held = {b: v for b, v in rest.items() if b in keep}
        else:
            held = rest
        if part == "add":
            # (Differences, not poses: no bone held at rest, nothing for the bones
            # it leaves alone.)
            frames = max(len(r) for _, r in bones.values())
            bc.additive_clip(w, names, role, bones, dt)
            lines.append("\t".join(["clip", role, str(len(w.g["animations"]) - 1), "%.4g" % (frames * dt), "add"]))
            continue
        if role in LOOPS:
            bones = looped_back(bones, dt, LOOPS[role])
        length = bc.base_clip(w, names, role, bones, dt, loop=bool(looping and looping[0]), rest=held,
                              skip=("Root_Reference",))
        kind = {True: "upper", "left": "left"}.get(part, "full")
        lines.append("\t".join(["clip", role, str(len(w.g["animations"]) - 1), "%.4g" % length, kind]))
    if cooked:
        lines += socket_lines(cooked)
        lines += aim_offset_lines(cooked)
    size = w.save(os.path.join(models, "robot_extras.glb"))
    print(f"wrote robot_extras.glb ({len(w.g['animations'])} clips, {size / 1e6:.2f} MB)")
    open(os.path.join(models, "robot_extras.txt"), "w").write("\n".join(lines) + "\n")
    print("wrote models/robot_extras.txt")


if __name__ == "__main__":
    if len(sys.argv) not in (3, 4):
        sys.exit(__doc__)
    main(*sys.argv[1:])
