"""Escalation's waves, read from the maps' packages: what each map's fifteen waves
field, and where its enemies come in.

usage: python build_waves.py <CookedPC folder> <foc_esc/assets folder>

Each Escalation map's gameplay package (ESC_<map>_BASE_m.xxx) places its survival
actor (TnSurvivalActor: NumAttempts, the team's lives; GlobalMaxAICount, the most
enemies at once) with its goals: TnSurvivalGoalTimePressure (the waits before the
first wave and between waves, StartGameDownTime and DownTime; how few enemies
left shows them all, AiLeftToTriggerEnemyView; how long the whole team can be down
before a wave is lost, FailureEndRoundTime) and TnSurvivalGoalWaveSpawning (its
spawn list, TR_Survival30_m.SurvivalSpawnList_<Map>Tuned_p, and WinEndGameDelay).
Its wave spawn action fields one enemy at a time, a random 1 to 3 seconds apart
(TnSurvivalWeightConditionCooldown's CooldownRange).

The spawn list (TnSurvivalSpawnList) has the waves (each one's spawn groups, its
living and total budgets, its money, and the buffs every player has through it:
TnBuffSurvivalWavePlayerDamageDone and DamageTaken, at the wave's number as their
level) and the spawn groups (each one's AI types: the character, what it costs of
a budget, its weight, the most of it at once, its cooldown after one dies, and
the spawn request it comes in by: which clusters, and whether it must come in
out of the players' sight), its last wave (FinalWave) and the money each player
starts with (StartingMoney).

The clusters (TnSpawnClusterSurvival volumes: SpawnIdentifier, the spawn request
they serve; _CenterPoint, scored against the players; SpawnPoints) hold the
spawn points: TnSurvivalSpawnPoint on the ground (some with an animation the
enemy comes in with, SpawnAnimationName) and TnSurvivalAirSpawnPoint in the air
(the jets' and the snipers').

Writes assets/levels/<map>_waves.txt, tab-separated, a record a line (positions
in Bevy's axes, metres; headings in radians about the up axis, 0 facing -Z):

  settings <first wait s> <wait s> <enemies shown at> <lost after s> <lives>
           <most enemies> <last wave> <starting money> <won after s>
           <spawns apart min s> <spawns apart max s>
  wave <number, from 1> <living budget> <total budget> <money> <money if lost>
       <damage done> <damage taken> <spawn groups, comma> <special groups, comma or ->
  type <group> <kind> <cost> <weight> <most at once, 0: any> <cooldown s>
       <spawn request> <sight: 0 out of it, 1 in it, 2 either>
  cluster <number> <spawn request> <middle x y z> <name>
  point <cluster> <x y z> <heading> <animation or ->
  air <cluster> <x y z> <heading>
  sniper <x y z> <heading>

(A cluster's name is the level's for it: the doors' Kismet switches some off as
gameplay starts and on as their doors open, tools/build_doors.py.) The sniper
lines are the level's TnSniperPoints: the perches its snipers make for once they
have flown in.

(The kinds are src/enemies.rs's: the character's name less _CHARACTER, as
tools/build_enemies.py names them.)
"""
import math
import os
import struct
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.path.join(HERE, "ue3"))
sys.path.insert(0, HERE)

import class_data as cd  # noqa: E402
from build_map import _clean, actor_props, rot3, vec3  # noqa: E402
from enemy_data import MAPS, SPAWN_LIST, SPAWN_LIST_MAP  # noqa: E402
from props import read_props  # noqa: E402
from ue3pkg import Reader  # noqa: E402

# The players' wave buffs' factors by level (TransGame's and TransContent's
# classes).
DONE_BUFF = "Default__TnBuffSurvivalWavePlayerDamageDone"
TAKEN_BUFF = "TransContent.Default__TnBuffSurvivalWavePlayerDamageTaken"
# The spawn requests' sight rule (ESpawnRequestLOSRequirement).
SIGHT = {"SRLR_RequiresNoLOS": 0, "SRLR_RequiresLOS": 1, "SRLR_NoRequirement": 2}


def conv(v):
    """Unreal units (x, y, z up) to Bevy's metres (x, y up, z)."""
    return (round(v[0] * 0.01, 3), round(v[2] * 0.01, 3), round(v[1] * 0.01, 3))


def heading(yaw):
    """An Unreal yaw (1/65536 turns) as the demo's heading in radians (its facing
    turned about the up axis from -Z, as enemies.rs's yaw_towards)."""
    t = yaw * 2 * math.pi / 65536.0
    return round(math.atan2(-math.cos(t), -math.sin(t)), 5)


def placed_props(pkg, e, maxoff=200):
    """A placed actor's tagged properties: the longest clean parse that has its
    Location (the longest of all can be another list's: an air spawn point's
    paths parse longer than it does), else build_map's longest."""
    best = None
    for st in range(0, maxoff):
        try:
            pr = read_props(pkg, Reader(pkg.data, e["offset"] + st), e["offset"] + e["size"])
        except Exception:
            continue
        if pr and "Location" in pr and all(_clean(k) for k in pr):
            if best is None or len(pr) > len(best):
                best = pr
    return best or actor_props(pkg, e)


def floats(v):
    if isinstance(v, tuple) and v[0] == "array" and len(v[1]) >= 4:
        n = struct.unpack_from("<i", v[1], 0)[0]
        if len(v[1]) == 4 + 4 * n:
            return list(struct.unpack_from("<%df" % n, v[1], 4))
    return []


def ints(v):
    return [int(x) for x in cd._objs(v)]


def num(pr, key, default):
    v = pr.get(key, default)
    return v if isinstance(v, (int, float)) and not isinstance(v, bool) else default


def of_class(mp, cls):
    """The level's objects of a class (not its default object)."""
    p = mp.pkg
    return [i + 1 for i, e in enumerate(p.exports)
            if p.class_name(e) == cls and not e["name"].startswith("Default__")]


def defaults(mp, cls):
    """A TransContent class's default object's properties (cooked into the map)."""
    return mp.props(mp.find("TransContent.Default__" + cls))


def struct_props(mp, v):
    if isinstance(v, tuple) and v[0] == "struct":
        return cd._struct_props(mp.pkg, v) or {}
    return {}


def name_of(mp, v):
    """An object property's object's name."""
    i = cd._obj(v)
    return mp.name(i) if i else None


def read_map(cooked, package, stem, done, taken_cache, list_defaults):
    mp = cd.Mp(cooked, package)
    p = mp.pkg
    lines = []

    # The survival actor and its goals, over their classes' defaults (the actor is
    # an actor: its properties follow its state frame, actor_props).
    actor = defaults(mp, "TnSurvivalActor")
    for i in of_class(mp, "TnSurvivalActor"):
        actor = {**actor, **actor_props(p, p.exports[i - 1])}
    pressure = defaults(mp, "TnSurvivalGoalTimePressure")
    for i in of_class(mp, "TnSurvivalGoalTimePressure"):
        pressure = {**pressure, **mp.props(i)}
    spawning = defaults(mp, "TnSurvivalGoalWaveSpawning")
    for i in of_class(mp, "TnSurvivalGoalWaveSpawning"):
        spawning = {**spawning, **mp.props(i)}
    cooldown = struct_props(mp, defaults(mp, "TnSurvivalWeightConditionCooldown").get("CooldownRange"))
    sl_idx = mp.find(SPAWN_LIST.format(SPAWN_LIST_MAP[stem]))
    sl = {**list_defaults, **mp.props(sl_idx)}
    waves = cd._structs(p, sl.get("Waves"))
    groups = cd._structs(p, sl.get("SpawnGroups"))
    final_wave = int(num(sl, "FinalWave", len(waves) - 1))
    lines.append("\t".join(str(x) for x in [
        "settings",
        num(pressure, "StartGameDownTime", 15.0),
        num(pressure, "DownTime", 10.0),
        int(num(pressure, "AiLeftToTriggerEnemyView", 3)),
        num(pressure, "FailureEndRoundTime", 3.0),
        int(num(actor, "NumAttempts", 3)),
        int(num(actor, "GlobalMaxAICount", 20)),
        final_wave + 1,
        int(num(sl, "StartingMoney", 0)),
        num(spawning, "WinEndGameDelay", 5.0),
        num(cooldown, "Min", 1.0),
        num(cooldown, "Max", 3.0),
    ]))

    # The waves, with their players' buffs' factors.
    if taken_cache is None:
        taken_cache = floats(mp.props(mp.find(TAKEN_BUFF)).get("FloatModifier"))
    for n, w in enumerate(waves):
        dmg_done, dmg_taken = 1.0, 1.0
        for b in cd._structs(p, w.get("PlayerBuffs")):
            cls = name_of(mp, b.get("BuffClass")) or ""
            level = int(num(b, "BuffLevel", 0))
            if cls.endswith("DamageDone") and done:
                dmg_done = done[min(level, len(done) - 1)]
            elif cls.endswith("DamageTaken") and taken_cache:
                dmg_taken = taken_cache[min(level, len(taken_cache) - 1)]
        special = ints(w.get("SpecialSpawnGroupIds"))
        lines.append("\t".join(str(x) for x in [
            "wave", n + 1,
            int(num(w, "LivingBudget", 0)),
            int(num(w, "TotalBudget", 0)),
            int(num(w, "MoneyRewardAmount", 0)),
            int(num(w, "MoneyLostWaveRewardAmount", 0)),
            round(dmg_done, 4), round(dmg_taken, 4),
            ",".join(str(g) for g in ints(w.get("SpawnGroupIds"))),
            ",".join(str(g) for g in special) or "-",
        ]))

    # The spawn groups' AI types.
    kinds = set()
    for g in groups:
        gid = int(num(g, "SpawnGroupId", 0))
        for t in cd._structs(p, g.get("AITypes")):
            kind = (name_of(mp, t.get("Description")) or "").replace("_CHARACTER", "")
            req = struct_props(mp, t.get("SpawnRequestData"))
            sight = SIGHT.get(str(req.get("LOSRequirement", "SRLR_RequiresNoLOS")), 0)
            kinds.add(kind)
            lines.append("\t".join(str(x) for x in [
                "type", gid, kind,
                int(num(t, "Cost", 1)),
                num(t, "Weight", 1.0),
                int(num(t, "MaxCount", 0)),
                num(t, "Cooldown", 0.0),
                int(num(req, "Identifier", 0)),
                sight,
            ]))

    # The clusters and their points.
    clusters = 0
    for ci in of_class(mp, "TnSpawnClusterSurvival"):
        pr = placed_props(p, p.exports[ci - 1])
        if pr.get("_Disabled") is True:
            continue
        points = cd._objs(pr.get("SpawnPoints"))
        if not points:
            continue
        centre = vec3(pr, "_CenterPoint", vec3(pr, "Location"))
        ident = int(num(pr, "SpawnIdentifier", 0))
        name = p.full_name(ci).split(".")[-1]
        lines.append("\t".join(str(x) for x in ["cluster", clusters, ident, *conv(centre), name]))
        for pi in points:
            if pi <= 0:
                continue
            e = p.exports[pi - 1]
            cls = p.class_name(e)
            pp = placed_props(p, e)
            at = conv(vec3(pp, "Location"))
            yaw = heading(rot3(pp)[1])
            if cls == "TnSurvivalAirSpawnPoint":
                lines.append("\t".join(str(x) for x in ["air", clusters, *at, yaw]))
            else:
                anim = pp.get("SpawnAnimationName")
                anim = str(anim) if anim and str(anim) != "None" else "-"
                lines.append("\t".join(str(x) for x in ["point", clusters, *at, yaw, anim]))
        clusters += 1

    # The snipers' perches.
    for si in of_class(mp, "TnSniperPoint"):
        pp = placed_props(p, p.exports[si - 1])
        at = conv(vec3(pp, "Location"))
        lines.append("\t".join(str(x) for x in ["sniper", *at, heading(rot3(pp)[1])]))
    return lines, taken_cache, kinds, clusters


def main():
    cooked, assets = sys.argv[1], sys.argv[2]
    tg = cd.Mp(cooked, "TransGame.xxx")
    done = floats(tg.props(tg.find(DONE_BUFF)).get("FloatModifier"))
    # (The spawn list's class is TransGame's: its FinalWave and StartingMoney.)
    list_defaults = tg.props(tg.find("Default__TnSurvivalSpawnList"))
    taken = None
    for stem, package in MAPS.items():
        lines, taken, kinds, clusters = read_map(cooked, package, stem, done, taken, list_defaults)
        out = os.path.join(assets, "levels", f"{stem}_waves.txt")
        os.makedirs(os.path.dirname(out), exist_ok=True)
        with open(out, "w") as fh:
            fh.write(f"# {stem}'s Escalation waves (tools/build_waves.py): see its header for the fields\n")
            for line in lines:
                fh.write(line + "\n")
        waves = sum(1 for line in lines if line.startswith("wave\t"))
        points = sum(1 for line in lines if line.startswith(("point\t", "air\t")))
        print(f"wrote {out}: {waves} waves, {len(kinds)} kinds, {clusters} clusters, {points} points")


if __name__ == "__main__":
    main()
