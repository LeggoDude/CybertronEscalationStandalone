"""The maps' parts that move by themselves in the game, for src/mapmotion.rs: the
actors that spin and the glows that pulse.

  * Spinning actors: an actor whose Physics is PHYS_Rotating turns by its
    RotationRate every second (Actor.physRotating: its Rotation's pitch, yaw and roll
    each go on by the rate's, Unreal's rotation units, 65536 a turn). Ignition's two
    turbines over the rooms beside the players' start (InterpActors of
    DES_TransportDeckTurbine_p's turbine_STAT, a turn every 6 s) and Downfall's map
    room's monitors (ENV_ASH_MaproomMonitor*_STAT) do; tools/build_map.py builds
    them into the map's model where they stand as the level starts, a node each,
    named after the mesh, at the actor's place. None of them collides with
    anything (CollideActors off).
  * Pulsing glows: ENV_BRDF_A_MAT's glow (its ENV_Emissive_MF function), with the
    static switch Enable_Emissive_Pulse on, is multiplied by
    clamp(sin(Time x Emissive_Pulse_Rate x 2 pi) + 1, 0, 1.5) (a Sine node of
    Period 1, then "half(clamp((%A+1),0,1.5))"): bright for a third of each cycle,
    dimming to nothing and back for the rest, Emissive_Pulse_Rate cycles a second
    (its default 1). Oblivion's machines' lights do: the orange bars on the
    ordnance feeder's doors ahead of the players' start (OrdinanceDoor_MATINST) and
    the blue ones on its pipes and pillars (ArkDrivingPipe_MATINST,
    ArkDrivingPillar_MATINST), once a second, and its warning signs
    (WallPanelWarningSign_Red_MATINST, 0.5 a second), and others. The instances are
    the map's own and their parents (matparams.py's chain, the art package's
    first, over the shared packages': matparams.map_params, as tools/build_map.py
    reads them); those that glow at all
    (EmissiveIntensity over 0) are listed, by name, for every model of the map that
    draws them (the map's, its doors', its consoles').

Writes levels/<map>_motion.txt, tab-separated:

  spin <mesh> <x y z> <pitch yaw roll> <rate pitch yaw roll> <scale x y z>
      A spinning actor: its mesh (the node's name in models/<map>.glb), its place
      (Bevy metres: the node's translation), its Rotation as the level starts and
      its RotationRate (Unreal's units, and a second), and its size
      (DrawScale3D x DrawScale, Unreal's axes).
  pulse <material> <cycles a second>
      A material whose glow pulses.

usage: python build_motion.py <TransGame\\CookedPC> <foc_esc\\assets> [<map> ...]
(no maps: all four)
"""
import os
import struct
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.path.join(HERE, "ue3"))
sys.path.insert(0, HERE)

import matparams  # noqa: E402
from kismet import _props  # noqa: E402
from ue3pkg import Package  # noqa: E402

# Each map's packages (tools/build_map.py's MAPS).
MAPS = {
    "downfall": ("ESC_Iacon_ART_m", "ESC_Iacon_BASE_m"),
    "oblivion": ("ESC_Ark_ART_m", "ESC_Ark_BASE_m"),
    "ancients": ("ESC_Temple_ART_m", "ESC_Temple_BASE_m"),
    "ignition": ("ESC_Gladiator_ART_m", "ESC_Gladiator_BASE_m"),
}
# The glow's pulse (ENV_Emissive_MF): its switch and its rate's parameter and default.
PULSE_SWITCH = "Enable_Emissive_Pulse"
PULSE_RATE = "Emissive_Pulse_Rate"
PULSE_RATE_DEFAULT = 1.0
# Unreal units to metres.
UU = 0.01


def _obj(pkg, v):
    return v[1] if isinstance(v, tuple) and v and v[0] == "obj" and isinstance(v[1], int) else 0


def _vec(v, default):
    if isinstance(v, tuple) and v[0] == "struct" and len(v[2]) >= 12:
        return struct.unpack_from("<3f", v[2])
    return default


def _rot(v):
    if isinstance(v, tuple) and v[0] == "struct" and len(v[2]) >= 12:
        return struct.unpack_from("<3i", v[2])
    return (0, 0, 0)


def spins(pkg):
    """The package's spinning actors: (mesh, place in Bevy metres, Rotation,
    RotationRate, scale)."""
    out = []
    for i, e in enumerate(pkg.exports, 1):
        c = pkg.class_name(e)
        if not c.endswith("Actor") or c in ("StaticMeshCollectionActor", "StaticLightCollectionActor"):
            continue
        pr = _props(pkg, i, 40)
        if str(pr.get("Physics", "")) != "PHYS_Rotating" or "RotationRate" not in pr:
            continue
        comp = _obj(pkg, pr.get("StaticMeshComponent"))
        mesh = _obj(pkg, _props(pkg, comp, 40).get("StaticMesh")) if comp > 0 else 0
        if not mesh:
            print(f"  ! {e['name']} spins without a static mesh: left out")
            continue
        x, y, z = _vec(pr.get("Location"), (0.0, 0.0, 0.0))
        ds = float(pr.get("DrawScale", 1.0)) if isinstance(pr.get("DrawScale"), (int, float)) else 1.0
        sx, sy, sz = _vec(pr.get("DrawScale3D"), (1.0, 1.0, 1.0))
        out.append((pkg.full_name(mesh).split(".")[-1], (x * UU, z * UU, y * UU), _rot(pr.get("Rotation")),
                    _rot(pr.get("RotationRate")), (sx * ds, sy * ds, sz * ds)))
    return out


def pulses(params):
    """{material instance: its pulse's rate} for the instances of ENV_BRDF_A_MAT
    whose glow pulses."""
    out = {}
    for name in params:
        chain, root = matparams._chain(params, name)
        if root != matparams.BASE:
            continue
        switch, rate, intensity = None, None, None
        for n in chain:
            p = params[n]
            if switch is None and PULSE_SWITCH in p.switches:
                switch = p.switches[PULSE_SWITCH]
            if rate is None and PULSE_RATE in p.scalars:
                rate = float(p.scalars[PULSE_RATE])
            if intensity is None and "EmissiveIntensity" in p.scalars:
                intensity = float(p.scalars["EmissiveIntensity"])
        # (The switch's first input, 0, is "Enabled".)
        if switch == 0 and (intensity or matparams.DEFAULT_INTENSITY) > 0:
            out[name] = PULSE_RATE_DEFAULT if rate is None else rate
    return out


def fmt(v):
    return " ".join("%.6g" % x for x in v)


def main(cooked, assets, maps):
    for stem in maps or MAPS:
        art, base = MAPS[stem]
        pkgs = [Package(os.path.join(cooked, f"{p}.xxx")) for p in (art, base)]
        params = matparams.map_params(pkgs, cooked)
        lines = [f"# {stem}'s parts that move by themselves (tools/build_motion.py): see its header"]
        found = [s for pkg in pkgs for s in spins(pkg)]
        for mesh, at, rot, rate, scale in found:
            lines.append("\t".join(["spin", mesh, fmt(at), fmt(rot), fmt(rate), fmt(scale)]))
        glowing = pulses(params)
        for name, rate in sorted(glowing.items()):
            lines.append("\t".join(["pulse", name, "%.6g" % rate]))
        path = os.path.join(assets, "levels", f"{stem}_motion.txt")
        with open(path, "w") as f:
            f.write("\n".join(lines) + "\n")
        print(f"wrote levels/{stem}_motion.txt ({len(found)} spinning, {len(glowing)} pulsing)")


if __name__ == "__main__":
    if len(sys.argv) < 3:
        sys.exit(__doc__)
    main(sys.argv[1], sys.argv[2], sys.argv[3:])
