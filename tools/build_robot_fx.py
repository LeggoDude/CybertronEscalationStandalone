"""Write the effects the robots' moves play under their feet (src/movefx.rs): the
HmAnimNotify_PlayEffect notifies of the clips a player plays to dash, jump and
land, each class's own where it plays its own clip.

  * The dashes (Nav_Boost_F/B/L/R on the ground, Nav_Air_Boost_F/B/L/R in the air)
    and the jump (Nav_TakeOff_01) fire the boosters at his ankles and soles
    (CarJump_01_FX on BoosterSocket_L/R and MeleeSocket_LeftFoot/RightFoot, in his
    EnergonColor: their TnParticleSystemParameterEnergonColor parameters); the side
    dashes kick up dust under him as they touch down (Long_Fall_Landing_1_FX on
    RootSocket) and fire the boosters again; the forward and back ones scuff the
    ground as they end (Long_Fall_Landing_Sm_FX at the feet; a Titan's and a
    Destroyer's back dash Vehland_Low_Med_FX).
  * The landings (Nav_Land, Nav_Land_Hard_01) throw up dust (Long_Fall_Landing_1_FX
    at the right ankle).

The clips are robot.glb's (all_shared.xxx's Shared_ROBO_NAV_ANIM: the Infiltrator's
and the Scientist's) or the class's own (the Destroyer's and the Titan's
mp_shared.xxx Shared_Truck_ROBO_ANIM), as build_class.py picks them (class_clip:
the last of the class's animation sets with the clip).

Writes <assets>/models/robot_fx.txt (tab-separated):
  clip  <class> <robot.glb clip> <role> <seconds>
        (the clip's length as the demo plays it at speed 1: its RateScale in it)
  fx    <class> <robot.glb clip> <seconds> <particle system> <socket> <attached 0/1>
        <x y z> <pitch yaw roll> <parameters> <auto-deactivate 0/1>
        (an HmAnimNotify_PlayEffect: its effect on the socket, carried with it if
        attached (bAttachToMesh), else left where it starts; its place and turn in
        the socket's frame (Unreal units and rotation units); its colour parameters
        set to the player's EnergonColor, "-" for none; stopped once the clip has
        played out (bAutoDeactivate: the clip's length and 0.04 s after it starts))

The particle systems are build_weapon_fx.py's (fx/weapons_fx.txt).

usage: python build_robot_fx.py <CookedPC> <TransGame> <foc_esc/assets>
"""
import contextlib
import io
import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.path.join(HERE, "ue3"))
sys.path.insert(0, HERE)

import build_ai_anims as ba  # noqa: E402
import build_class as bc  # noqa: E402
import class_data as cd  # noqa: E402

# robot.glb's clips of moving about whose effects are played (build_class.py's
# ROBOT_SLOTS roles): the jump, its landings and the dashes.
ROLES = ("land", "land_hard", "jump") + tuple(f"boost_{d}" for d in "fblr") + tuple(f"air_boost_{d}" for d in "fblr")
CLASSES = ("infiltrator", "destroyer", "titan", "scientist")
# The packages the clips are in (the multiplayer map's own sets, the shared ones).
PACKAGES = (cd.MP_PACKAGE, "all_shared.xxx", "mp_shared.xxx")


def robot_clips(assets, cls):
    """The robot.glb clips the class plays its own of (its <class>.txt robotclip
    lines: build_class.py's overrides)."""
    path = os.path.join(assets, "models", cls, cls + ".txt")
    out = set()
    if os.path.exists(path):
        for line in open(path):
            f = line.rstrip("\n").split("\t")
            if f[0] == "robotclip" and len(f) == 3:
                out.add(int(f[1]))
    return out


def class_lines(cooked, transgame, assets, cls, pkgs):
    with contextlib.redirect_stdout(io.StringIO()):
        data = cd.read(cooked, transgame, cls)
    pkgs = [data["mp"]] + pkgs
    own_clips = robot_clips(assets, cls) if cls != "infiltrator" else set()
    lines = []
    for number, role, game_clip, slot_set, _loop in bc.ROBOT_SLOTS:
        if role not in ROLES:
            continue
        own, _scout = bc.class_clip(data, slot_set, game_clip)
        anim_set = own if number in own_clips else slot_set
        seq, pkg = 0, None
        for pkg in pkgs:
            seq = pkg.sequence(anim_set, game_clip)
            if seq:
                break
        if not seq:
            print(f"  ! {cls}: {anim_set}.{game_clip} not found")
            continue
        rate = data["anim_rates"].get((anim_set, game_clip), 1.0) or 1.0
        length = float(pkg.props(seq).get("SequenceLength", 0.0)) / rate
        lines.append(["clip", cls, str(number), role, "%.4g" % length])
        notified = {}
        for t, kind, pr in pkg.notifies(seq):
            if kind == "HmAnimNotify_PlayEffect":
                notified[round(t, 4)] = pr.get("bAutoDeactivate") is True
        fx = [f for f in ba.notifies(pkg, seq, 1.0, 0.0, {}) if f[0] == "fx"]
        for f in fx:
            _, t, system, socket, attached, _weight, at, turn, params = f[:9]
            auto = notified.get(round(float(t), 4), False)
            lines.append(["fx", cls, str(number), "%.4g" % (float(t) / rate), system, socket, attached, at, turn,
                          params, "1" if auto else "0"])
        print(f"  {cls} {role}: {anim_set}.{game_clip} ({len(fx)} effects)")
    return lines


def main(cooked, transgame, assets):
    pkgs = [cd.Mp(cooked, p) for p in PACKAGES[1:] if os.path.exists(os.path.join(cooked, p))]
    lines = ["# The effects the robots' moves play (tools/build_robot_fx.py): see its header for the fields"]
    for cls in CLASSES:
        lines += ["\t".join(f) for f in class_lines(cooked, transgame, assets, cls, pkgs)]
    out = os.path.join(assets, "models", "robot_fx.txt")
    open(out, "w", newline="\n").write("\n".join(lines) + "\n")
    print("wrote", out)


if __name__ == "__main__":
    if len(sys.argv) != 4:
        sys.exit(__doc__)
    main(*sys.argv[1:])
