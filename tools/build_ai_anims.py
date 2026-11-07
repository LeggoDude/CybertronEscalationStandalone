"""Build the enemies' AI clips: each anim tree's (the types that play the same tree
on the same skeleton from the same anim sets share a set: the soldiers' and the
Sharpshooter's Soldier_ANIMTREE, the Shotgunners', the Rocket Troopers', the
Scrappers', the Titan's and the Heavy Gunners' Titan_ANIMTREE, the Guard's, the
Leaper's), as their anim tree and their blueprints' animations (ai.txt's values)
play them. For each set <set>:

  * models/enemies/ai_<set>.glb: the set's skeleton (its first type's) with every
    clip its types play: the anim tree's (its sequences, its chooser groups', its
    stance transitions', its turn-in-place and acrobatics pickers'), the
    blueprints' (their reloads, grenade throws, blind fire, reactions and hit
    reactions) and the melee sets' (their chassis' MeleeSet: ai.txt's Melee.*).
    Poses as poses (a clip a looping node of the tree plays closed on its first
    pose); additive ones (ADD_*) as the turns put in front of the bones' and the
    offsets added to their places (build_class.py's additive_clip); and the aimer's
    aim poses (its profile's AimPoses) as additive offsets from its reference pose
    (TnAnimNodeAimer adds the aim's blend of them to the pose under it).
  * models/enemies/ai_anims_<set>.txt: what src/ai reads (tab-separated):
      types  <enemy type ids...>    the types that play the set
      clip   <name> <seconds> <rate scale> <index> <kind>
             (its SequenceLength; its index in ai_<set>.glb; kind: pose, additive
             or aim)
      group  <name> <clips...>      a chooser group (one of its clips at random)
      htn    <clip> <seconds> <event> <socket> <mirrored socket>
             (TnAnimNotify_HtnEvent: the AI's AnimNotify event, a grenade's release)
      sweep  <clip> <seconds> <duration> <x y z> <x y z>
             (TnAnimNotify_DamageSweep: a melee blow's box, its centre from the
             pawn's middle and its half sizes, Unreal units in the pawn's axes:
             forward, right, up)
      impact <clip> <seconds> <duration> <socket> <x y z> <damage> <damage type>
             <forward> <sideways> <vertical>
             (TnAnimNotify_ImpactDamageSweep: the melee service's impact sweep, a
             box of the half sizes (uu) on the socket, swept as it moves for the
             duration; each pawn it meets takes the damage (its DamageOverride) of
             the type, and the push (HitMomentum: forward from the pawn along the
             ground, sideways, up))
      round  <clip> <seconds> <0/1>
             (TnAnimNotify_AIFireRound: a round fired, along the gun if 1)
      burst  <clip> <seconds> <duration>
             (TnAnimNotify_AIFireWeapon: the gun fired for the duration: blind fire)
      step   <clip> <seconds> <foot 0 left/1 right> <footstep type> <min weight>
             (AnimNotify_Footstep: a footstep, its EFootstepType by number: 0 walk,
             1 scuff, 2 jump, 3 land, 4 run, 5 jog, 6-9 crouching, 10 hard land)
      sound  <clip> <seconds> <cue> <socket or -> <min weight>
             (HmAnimNotify_Sound: a cue played at the socket)
      event  <clip> <seconds> <sound event> <socket or -> <min weight>
             (HmAnimNotify_SoundEvent: a sound event, its cue the pawn's set's)
      dialog <clip> <seconds> <dialog event> <socket or -> <min weight> <death 0/1>
             (HmAnimNotify_Dialog, or HmAnimNotify_DialogDeath (1): a line of
             dialog, through the dialog manager)
      voice  <clip> <seconds> <cue> <socket or -> <min weight>
             (an HmAnimNotify_Dialog with a cue of its own: HmPawn.PlayDialog, the
             cue spoken in its voice, cutting short the one before; no dialog
             rules: the Swarmers' chirps)
      fx     <clip> <seconds> <particle system> <socket> <attached 0/1> <min weight>
             <x y z> <pitch yaw roll> <parameters> <auto off 0/1>
             (HmAnimNotify_PlayEffect: an effect on the socket, held on it if
             attached (bAttachToMesh), else left where it starts; its place and turn
             in the socket's frame, uu and Unreal's rotation units; the colour
             parameters it sets to the AI's EnergonColor and "@team" for its side's
             module channel (its Parameters), "-" for none; and 1 if it is turned
             off with its clip (bAutoDeactivate: as the clip stops, or its playback
             length and 0.04 s after it starts), 0 if it plays on till it ends by
             itself)
      proj   <clip> <seconds> <socket> <projectile class> <cone degrees> <count>
             (TnAnimNotify_SpawnProjectile: rounds fired from the socket, along it
             but for a random turn within the cone (TnProjectileSpawner), at the
             AI's enemy)
      toggle <clip> <seconds> <hide 0/1>
             (TnAnimNotify_ToggleHidden: the form whose model plays it hidden (1,
             Toggle_Hide) or shown: a transformation's halves')
      vehiclefx <clip> <seconds> <allowed 0/1>
             (TnAnimNotify_ToggleVehicleFx: the alt form's effects allowed (1) or
             not: a jet's engines as it changes form)
             (A notify plays only while its node's weight in the tree is at least
             its min weight.)
      chain  <kind> <args...>
             (the tree's nodes over its base pose, from the bottom up: "acro" the
             acrobatics slot, "aim" the TnAnimNodeAimer, "aimoffset" a
             TnAnimNodeAimOffset, "masked <slot> <first bone>" an
             AnimNodeBlendMultiBone's custom slot over the bones from its first
             down, "slot <slot>" a custom slot over everything (AdditiveSlot: the
             additive one), "jump" the TnJumpTrajectoryBlend, "mirror" the
             TnAnimNodeHandednessMirror, "nodemirror" a TnAnimNodeMirror)
      mask   <slot> <bone path>
             (the bones a masked slot takes over: each bone's path of names from the
             model's root, as Bevy names animation targets)
      maskclips <slot> <clips...>
             (the clips the types play in a masked slot: their blueprints' and
             melee sets' animations of its mask)
      vblend <blend> <stance> <triangle indices...>
      vchild <blend> <child> <name> <clip> <group> <vx> <vy> <synch group>
             (a TnLinearVelocityBlend: the stance profile it serves, its samples'
             triangles, and each child: its node's name, its sequence and chooser
             group ("-" for none), its sample velocity, its sequence's root motion
             over its length, uu a second in the pawn's axes: forward, right (the
             first, its idle, at rest), and the synch group it plays in ("-": its
             own time))
      vpick  <blend> <child> <stance> <clip>
             (a child that is a TnStanceAnimNodeSequence: its TnStanceAnimationPicker's
             animation for a stance; its own clip for the rest)
      flyblend <blend> <blend speed> <min diagonal weight> <forward-back min max>
             <right-left min max> <up-down min max>
      fchild <blend> <child> <name> <clip> <group> <synch group>
             (a TnFlyingRobotAnimBlend: a flying robot's pose by the way it flies,
             its velocity in its own axes: its idle and its forward, backward, right,
             left, up and down children, each way's weight its speed's share of its
             range (uu a second), eased at the blend speed)
      strafer <blend> <blend speed>
             (a TnStraferAnimBlend written as a velocity blend: its forward,
             backward, right and left children (vchild lines, no triangles: their
             velocities one unit along those ways) weighted by the way the pawn
             moves, each one's weight moving that share of the way there a tick)
      jblend <triangle indices...>
      jchild <child> <name> <clip> <x> <z> <landing clip>
             (the TnJumpTrajectoryBlend: each jump's trajectory, its root motion's
             forward and up, uu, and the landing it plays)
      sprofile <index> <stance> <blend time> <kind> <args...>
             (TnStanceAnimBlend's profiles, in order: the stance's own pose, by
             kind: "switch" the velocity switch (the switch line), "vblend <name>"
             a velocity blend, "seq <clip> <group>" an auto-play sequence, "add
             <clip> <group> <clip> <group>" an additive blending of two; a tree with
             no stance blend has one profile, STANCE_None's, its base pose)
      strans <index> <target stance> <clip> <reverse 0/1> <blend in> <blend out>
             (a profile's transition to another stance's)
      switch <idle clip> <moving blend>
             (a TnVelocitySwitch: its idle and its moving states, seen through the
             state machine's switches and states (an AmpAnimSwitch's first state)
             and a TnVelocityAnimBlend whose walk and jog are one node)
      turn   <angle> <clip>
      turnparams <blend time> <threshold angle> <look ahead seconds>
             (the idle's TnAnimTurnInPlace: each turn (degrees, right positive) and
             the clip its TnAnimTurnInPlacePlayer plays for it; its blend time, how
             far short of a turn's angle it may start, and how far ahead it looks
             at the yaw rate)
      acro   <state> <clip> <blend in> <blend out> <looping 0/1>
             (TnAcrobaticsSlotLogic: what the acrobatics slot plays as the pawn
             jumps (state 0, its loop as state 100) and falls (5))
      aim    <bone> <reference clip> <blend speed> <stances...>
      aimpose <index> <clip> <x> <y>
             (the TnAnimNodeAimer: its aim bone, its reference pose, its stances, and
             each aim pose's aim from the reference's: the aimer's units, a quarter
             turn's yaw and pitch)
      aimof  <clip> <x> <y>
             (where a clip's first pose aims the aim bone: the aimer's input aim)
      aimoffset <class> <interp speed> <switch when left-handed 0/1>
      aoprofile <index> <name> <horizontal min max> <vertical min max>
      aoremap <profile> <horizontal min max> <vertical min max>
      aobone <profile> <bone> <LU> <LC> <LD> <CU> <CC> <CD> <RU> <RC> <RD>
      aostance <stance> <profile>
             (the aim offset node, UE3's AnimNodeAimOffset: High Moon's
             HmAnimNodeAimOffset (the Snipers' insects') or TnAnimNodeAimOffset (the
             Guard's, the Leaper's: its aim eased in at its InterpSpeed, 0 for none,
             and mirrored while the pawn is left-handed if it switches); its
             profiles' aim ranges (and a TnAnimNodeAimOffset profile's
             PawnAimOffsetRange where it remaps the pawn's aim, RemapPawnAimRange:
             the pawn's aims each side that are the profile's whole aim that way),
             and their bones' offsets baked from their aim poses, one for each way
             it aims (left, centre or right; up, centre or down), each
             "qx qy qz qw tx ty tz": a turn put in front of the bone's own in the
             mesh's space and a move of its place there, glTF's axes and metres; and
             the stances that pick a profile other than the first
             (StanceToProfileMappingOverrides))
      rootm  <clip> <frame seconds> <x y z yaw>...
             (the root bone's motion each frame from the first: uu forward, right
             and up at the mesh's own size, and degrees of yaw: a custom animation's
             root motion; only clips whose root moves)
  * models/enemies/ai_<set>_mirror.glb: the same clips mirrored left for right
    (mirror_anims.py), in the same order, for a tree with a
    TnAnimNodeHandednessMirror (a soldier holding his gun left-handed, at a corner
    on his left).

usage: python build_ai_anims.py <umodel export dir> <CookedPC> <foc_esc/assets> [enemy_data.json] [sets,...]
       (the export dir build_enemies.py reads, with the types' anim sets: its
       --umodel commands export them)
       python build_ai_anims.py --effects <CookedPC> <foc_esc/assets> [enemy_data.json]
       (brings the sets' ai_anims_<set>.txt already built up to date with their fx
       lines' auto off field, without building them again: effects_pass)
"""
import json
import math
import os
import struct
import sys

import numpy as np

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.path.join(HERE, "ue3"))
sys.path.insert(0, HERE)

import build_class as bc  # noqa: E402
import build_enemies as be  # noqa: E402
import class_data as cd  # noqa: E402
from gltf_io import Doc, Writer, quat_to_mat  # noqa: E402
import mirror_anims  # noqa: E402

OUT_DIR = os.path.join("models", "enemies")
SHARED = "all_shared.xxx"
# The sets: each one's types (its first's meshes, anim sets and tree it is built
# from; the others' anim sets add their clips).
SETS = [
    ("soldier", ["SoldierDec_Kaon", "SoldierAut_Prime", "SoldierDec_Rust_Sea", "SharpShooterAut"]),
    ("shotgunner", ["ShotgunnerAut", "ShotgunnerDec"]),
    ("rocket", ["RocketTrooper_AUT", "RocketTrooper_DEC"]),
    ("scrapper", ["ScrapperAut", "ScrapperDec"]),
    ("titan", ["Titan", "HeavyGunnerAut", "HeavyGunnerDec"]),
    ("guard", ["Guard"]),
    ("leaper", ["Leaper"]),
    ("swarmer", ["SwarmerLite"]),
    ("drone", ["SupportDrone", "ExplosiveDrone"]),
    # The Snipers' two forms: the robot (flying) and the insect on its perch (its
    # other form: <id>~other, build_enemies.py's TRANSFORMING).
    ("sniper", ["Sniper_AUT", "Sniper"]),
    ("sniper_alt", ["Sniper_AUT~other", "Sniper~other"]),
    # The jets' two forms: the robot (its other form, on the soldiers' tree) and
    # the plane (the form they start in, on VEH_Plane_ANIMTREE).
    ("flyer", ["Flyer_Aut~other", "Flyer_Dec~other"]),
    ("flyer_alt", ["Flyer_Aut", "Flyer_Dec"]),
]


# The clips the waves' spawn points bring their enemies in with (TnSurvivalSpawnPoint's
# SpawnAnimationName, build_waves.py's points: TnHtnAiPawn.StartSpawnAnimation), for
# the sets that have them: Ancients' Swarmers come up out of the ground
# (Swarmer_ALT_ANIM's). (Its CINE_ReadytoRide_s01_Swarmer points' clip is in no
# set the Swarmer plays; Ignition's Leapers' "Spawn" the demo plays as a leap.)
SPAWN_CLIPS = {"Nav_Spawn_Ground"}

# A transforming type's halves of its transformations, each form its own (the
# robot's _ROBO, the other's _VEH): TnRobotToVehicleTransformation's and
# TnVehicleToRobotTransformation's _SourceAnimationName and _TargetAnimationName,
# played in its full body slot as it changes form (TnTransformation.Execute).
TRANSFORM_CLIPS = ("Transform_ToRobot_", "Transform_ToVehicle_")


def form_kind(data, tid):
    """A set's type's form's kind (robot, jet, orb, insect), as form_for picks it."""
    if tid.endswith("~other"):
        return be.other_form_of(data["types"][tid[:-len("~other")]])[0]
    return be.form_of(data["types"][tid])[0]


def form_for(data, tid):
    """A set's type's form: the one it starts in, or for <id>~other its other one."""
    if tid.endswith("~other"):
        return be.other_form_of(data["types"][tid[:-len("~other")]])[1]
    return be.form_of(data["types"][tid])[1]


def base_ids(type_ids):
    """The types as ai.txt has them (an other form's values are its type's)."""
    return [t.split("~")[0] for t in type_ids]
# The custom slots by their nodes' names (TnRobotAnimator's players' masks, and the
# additive animator service's slot).
SLOT_MASKS = {"FullBodyCustom": "CAMT_FullBody", "UpperBodyCustom": "CAMT_UpperBody",
              "LeftArmCustom": "CAMT_LeftArm", "WeaponCustom": "CAMT_Weapon",
              "ShoulderCustom": "CAMT_Shoulder"}
FPS = 30.0
# The root bone (the root motion's, and the aimer measures from it).
ROOT_REF = "C_Root_Reference_XR"
# The aimer's unit: a quarter turn (its aims are yaw and pitch over 16384).
AIM_UNIT = 90.0
# A root that moves less than this (uu, degrees) over a clip stays put.
STILL_ROOT = (1.0, 0.5)
# TnAcrobaticsSlotLogic's TnAcrobaticsAnim defaults (BlendInTime, BlendOutTime).
ACRO_BLEND = (0.1, 0.1)


def sequences(mps, sets):
    """Each set's sequences and chooser groups, later sets first (as the mesh finds
    an animation: from its last set back), from every package's copy of the set
    (packages cook their own copies of a set, each with the sequences it needs:
    their union, the first package's where they share one): ({lower name: (name,
    set, package, export)}, {lower name: (name, [clips])})."""
    seqs, groups = {}, {}
    for s in reversed(sets):
        found = False
        for m in mps:
            for i in m.by_name.get(s, []):
                if m.pkg.class_name(m.pkg.exports[i - 1]) != "AnimSet":
                    continue
                found = True
                pr = m.props(i)
                for q in cd._objs(pr.get("Sequences")):
                    if q > 0:
                        name = str(m.props(q).get("SequenceName"))
                        seqs.setdefault(name.lower(), (name, s, m, q))
                for g in cd._structs(m.pkg, pr.get("ChooserGroups")):
                    name = str(g.get("GroupName"))
                    clips = [str(a.get("AnimName")) for a in cd._structs(m.pkg, g.get("Anims"))]
                    groups.setdefault(name.lower(), (name, clips))
        if not found:
            print("  ! anim set not found:", s)
    return seqs, groups


def clip_tracks(ex, sets, name):
    """A clip's tracks from the last of `sets` that has it, in any package's export
    of that set: ({bone: (pos, rot)}, frame time)."""
    for s in reversed(sets):
        for p in ex.pkgs:
            path = os.path.join(ex.root, p, "AnimSet", s + ".psa")
            if os.path.exists(path) and any(c["name"] == name for c in bc.psa(path).clips):
                return bc.psa(path).clip(name)
    return None, None


def strings(mp, v, out):
    """Every name and string a property holds (into `out`), through structs and
    arrays."""
    if isinstance(v, str):
        out.add(v)
    elif isinstance(v, tuple):
        if v[0] == "struct":
            try:
                for x in cd._struct_props(mp.pkg, v).values():
                    strings(mp, x, out)
            except Exception:
                pass
        elif v[0] == "array":
            try:
                for e in cd._structs(mp.pkg, v):
                    for x in e.values():
                        strings(mp, x, out)
            except Exception:
                pass
            try:
                out.update(mp.names(v))
            except Exception:
                pass


def tree_names(mp, tree):
    """Every name the anim tree's nodes and their objects (pickers, slot logic)
    hold, and those of its looping nodes ({names}, {looping names})."""
    names, looping = set(), set()
    prefix = tree + "."
    for full, i in mp.by_full.items():
        if not full.startswith(prefix):
            continue
        pr = mp.props(i)
        found = set()
        for v in pr.values():
            strings(mp, v, found)
        names |= found
        if pr.get("bLooping") is True:
            for k in ("AnimSeqName", "ChooserName"):
                if isinstance(pr.get(k), str):
                    looping.add(pr[k])
    return names, looping


def aim_profile(mp, tree):
    """The aimer's profile: its reference pose, its nine aim poses and the anim
    set they are in (its SourceAnimSet)."""
    for full, i in mp.by_full.items():
        if full.startswith(tree + ".") and full.split(".")[-1].startswith("AimerProfileGroup"):
            for p in cd._structs(mp.pkg, mp.props(i).get("Profiles")):
                poses = [str(p[k]) for k in sorted(p, key=lambda k: (len(k), k)) if k.startswith("AimPoses")]
                src = cd._obj(p.get("SourceAnimSet"))
                return str(p.get("ReferencePose")), poses, (mp.name(src) if src else None)
    return None, [], None


def type_props(assets, type_ids):
    """The types' values (ai.txt's): [(key, value)]."""
    out = []
    cur = None
    for line in open(os.path.join(assets, OUT_DIR, "ai.txt")):
        f = line.rstrip("\n").split("\t")
        if f[0] == "type":
            cur = f[1]
        elif f[0] == "prop" and cur in type_ids and len(f) >= 3:
            out.append((f[1], f[2]))
    return out


def blueprint_names(assets, type_ids):
    """The values the types' blueprints hold (ai.txt's)."""
    return {v for _, v in type_props(assets, type_ids)}


def masked_clips(assets, type_ids):
    """The animations the types play in each custom slot but the full body's: their
    blueprints' animation params of that mask (a MaskType beside an AnimName) and
    their melee sets' attacks (an AnimationMaskType beside an AnimName):
    ({mask: {names}}, {melee attacks' names})."""
    props = type_props(assets, type_ids)
    by_key = dict(props)
    out, melee = {}, set()
    for k, v in props:
        for field in ("MaskType", "AnimationMaskType"):
            if not k.endswith("." + field):
                continue
            base = k[:-len(field)]
            name = by_key.get(base + "AnimName")
            if not name or name == "None":
                continue
            if k.startswith("Melee."):
                melee.add(name)
            if v != "CAMT_FullBody":
                out.setdefault(v, set()).add(name)
    return out, melee


def resolve(names, seqs, groups):
    """The sequences (by their own names) and chooser groups the names are, a
    group's clips with it."""
    clips, grps = set(), {}
    for n in names:
        low = n.lower()
        if low in groups:
            gname, members = groups[low]
            grps[gname] = members
            for m in members:
                if m.lower() in seqs:
                    clips.add(seqs[m.lower()][0])
        if low in seqs:
            clips.add(seqs[low][0])
    return clips, grps


def notifies(sh, seq, scale, half_height, sockets):
    """The clip's notifies the AI acts on, as ai_anims.txt's lines (less the clip's
    name): their times the sequence's (a sweep's duration is the game's seconds)."""
    out = []
    for t, cls, pr in sh.notifies(seq):
        if cls == "TnAnimNotify_HtnEvent":
            event = pr.get("AnimEvent", 0)
            event = int(event) if isinstance(event, (int, float)) else 0
            out.append(("htn", "%.4g" % t, str(event), str(pr.get("SocketName", "None")),
                        str(pr.get("MirroredSocketName", "None"))))
        elif cls == "TnAnimNotify_DamageSweep":
            info = cd._struct_props(sh.pkg, pr["DamageSweepInfo"]) if "DamageSweepInfo" in pr else {}
            extent = cd._vec(info.get("Extent", pr.get("Extent")))
            offset = cd._vec(info.get("Offset"))
            duration = float(info.get("Duration", pr.get("Duration", 0.0)))
            socket = str(info.get("SocketName", pr.get("SocketName", "")))
            bone, at = sockets.get(socket, ("", (0.0, 0.0, 0.0)))
            # (A socket on the root reference is in the pawn's frame from its feet:
            # the mesh's scale on it, the half height off it.)
            c = np.array(at) * scale + np.array(offset)
            c[2] -= half_height
            out.append(("sweep", "%.4g" % t, "%.4g" % duration, fmt(c), fmt(extent)))
        elif cls == "TnAnimNotify_ImpactDamageSweep":
            info = cd._struct_props(sh.pkg, pr["DamageSweepInfo"]) if "DamageSweepInfo" in pr else {}
            extent = cd._vec(info.get("Extent", pr.get("Extent")))
            duration = float(info.get("Duration", pr.get("Duration", 0.0)))
            socket = str(info.get("SocketName", pr.get("SocketName", "None")))
            # (Its HitMomentum; an old one's MomentumScalar and ZOverride:
            # Migrate_1_To_2.)
            mom = cd._struct_props(sh.pkg, pr["HitMomentum"]) if "HitMomentum" in pr else {}
            scalar = float(pr.get("MomentumScalar", 0.0))
            forward = float(mom.get("ForwardMomentum", scalar))
            sideways = float(mom.get("SidewaysMomentum", 0.0))
            vertical = float(mom.get("VerticalMomentum", scalar * float(pr.get("ZOverride", 0.0))))
            dtype = obj_name(sh, pr.get("ImpactDamageType")) or "None"
            out.append(("impact", "%.4g" % t, "%.4g" % duration, socket if socket else "None", fmt(extent),
                        "%.4g" % float(pr.get("DamageOverride", 0.0)), dtype.split(".")[-1],
                        "%.6g" % forward, "%.6g" % sideways, "%.6g" % vertical))
        elif cls == "TnAnimNotify_AIFireRound":
            out.append(("round", "%.4g" % t, "1" if pr.get("AimInWeaponDirection") is True else "0"))
        elif cls == "TnAnimNotify_AIFireWeapon":
            out.append(("burst", "%.4g" % t, "%.4g" % float(pr.get("Duration", 0.0))))
        elif cls == "AnimNotify_Footstep":
            kind = FOOTSTEP_TYPES.index(pr["Type"]) if pr.get("Type") in FOOTSTEP_TYPES else 0
            foot = pr.get("FootDown", 0)
            out.append(("step", "%.4g" % t, str(int(foot) if isinstance(foot, int) else 0), str(kind),
                        "%.4g" % min_weight(pr)))
        elif cls == "HmAnimNotify_Sound":
            cue = obj_name(sh, pr.get("SoundCue"))
            if cue:
                # (On a PC its MinWeight isn't heeded: only console builds check it.)
                out.append(("sound", "%.4g" % t, cue, socket_name(pr), "0"))
        elif cls == "HmAnimNotify_SoundEvent":
            event = obj_name(sh, pr.get("SoundEvent"))
            if event:
                out.append(("event", "%.4g" % t, event, socket_name(pr), "%.4g" % min_weight(pr)))
        elif cls in ("HmAnimNotify_Dialog", "HmAnimNotify_DialogDeath"):
            event = obj_name(sh, pr.get("SoundEvent"))
            if event:
                out.append(("dialog", "%.4g" % t, event, socket_name(pr), "%.4g" % min_weight(pr),
                            "1" if cls == "HmAnimNotify_DialogDeath" else "0"))
            else:
                cue = obj_name(sh, pr.get("SoundCue"))
                if cue:
                    out.append(("voice", "%.4g" % t, cue, socket_name(pr), "%.4g" % min_weight(pr)))
        elif cls == "HmAnimNotify_PlayEffect":
            system = obj_name(sh, pr.get("ParticleSystemTemplate"))
            socket = socket_name(pr)
            if system and socket != "-":
                attach = pr.get("bAttachToMesh", True) is not False
                weight = pr.get("MinWeight", 0.0)
                weight = float(weight) if isinstance(weight, (int, float)) and not isinstance(weight, bool) else 0.0
                at = cd._vec(pr.get("RelativeLocation"))
                turn = cd._rot(pr.get("RelativeRotation"))
                out.append(("fx", "%.4g" % t, system.split(".")[-1], socket, "1" if attach else "0",
                            "%.4g" % weight, fmt(at), " ".join(str(int(x)) for x in turn),
                            effect_params(sh, pr.get("Parameters")),
                            "1" if pr.get("bAutoDeactivate") is True else "0"))
        elif cls == "TnAnimNotify_ToggleHidden":
            out.append(("toggle", "%.4g" % t, "1" if str(pr.get("Option", "")) == "Toggle_Hide" else "0"))
        elif cls == "TnAnimNotify_ToggleVehicleFx":
            out.append(("vehiclefx", "%.4g" % t, "0" if str(pr.get("Option", "")) == "Toggle_DisableFx" else "1"))
        elif cls == "TnAnimNotify_SpawnProjectile":
            proj = obj_name(sh, pr.get("ProjectileClass"))
            socket = socket_name(pr)
            if proj and socket != "-":
                cone = pr.get("ConeAngle", 0.0)
                count = pr.get("SpawnCount", 1)
                out.append(("proj", "%.4g" % t, socket, proj.split(".")[-1],
                            "%.4g" % (float(cone) if isinstance(cone, (int, float)) else 0.0),
                            str(int(count) if isinstance(count, int) and not isinstance(count, bool) else 1)))
    return out


# AnimNotify_Footstep.EFootstepType (HmFootstepComponent's sound for each).
FOOTSTEP_TYPES = ["kFootstepWalk", "kScuff", "kJump", "kLand", "kFootstepRun", "kFootstepJog",
                  "kFootstepCrouchRun", "kFootstepCrouchWalk", "kFootstepCrouchJog", "kCrouchScuff",
                  "kHardLand"]
# The sound notifies' MinWeight unless they set their own (their classes' defaults).
NOTIFY_MIN_WEIGHT = 0.25


def effect_params(mp, v):
    """An effect notify's Parameters (an HmParticleSystemParameterCollection, which
    HmAnimNotify_PlayEffect.Notify applies to the effect it starts): the colour
    parameters its TnParticleSystemParameterEnergonColor ones set to the AI's
    EnergonColor (their ParameterName, Color unless they name another), and "@team"
    for a TnParticleSystemParameterTeamIndex (its module channel by the AI's side);
    comma-separated, "-" for none."""
    out = []
    par = cd._obj(v)
    if par > 0:
        for q in cd._objs(mp.props(par).get("Parameters")):
            if q <= 0:
                continue
            cls = mp.pkg.class_name(mp.pkg.exports[q - 1])
            if cls == "TnParticleSystemParameterEnergonColor":
                name = mp.props(q).get("ParameterName", "Color")
                out.append(str(name) if name and str(name) != "None" else "Color")
            elif cls == "TnParticleSystemParameterTeamIndex":
                out.append("@team")
    return ",".join(out) or "-"


def min_weight(pr):
    w = pr.get("MinWeight", NOTIFY_MIN_WEIGHT)
    return float(w) if isinstance(w, (int, float)) and not isinstance(w, bool) else NOTIFY_MIN_WEIGHT


def socket_name(pr):
    s = pr.get("SocketName")
    return str(s) if isinstance(s, str) and s not in ("", "None") else "-"


def obj_name(mp, v):
    """A referenced object's whole name (an import's with its package)."""
    o = cd._obj(v)
    if not o:
        return None
    p = mp.pkg
    parts = []
    while o < 0:
        parts.append(p.obj_name(o))
        o = p.imports[-o - 1]["outer"]
    while o > 0:
        parts.append(p.obj_name(o))
        o = p.exports[o - 1]["outer"]
    return ".".join(reversed(parts))


def ints(v):
    """An ArrayProperty of ints."""
    if not (isinstance(v, tuple) and v[0] == "array") or len(v[1]) < 4:
        return []
    import struct
    n = struct.unpack_from("<i", v[1], 0)[0]
    return list(struct.unpack_from("<%di" % n, v[1], 4))


def tree_node(mp, tree, cls, name=None):
    """The tree's node of class `cls` (and NodeName `name`): its export, or 0."""
    for full, i in mp.by_full.items():
        if full.startswith(tree + ".") and mp.pkg.class_name(mp.pkg.exports[i - 1]) == cls:
            if name is None or mp.props(i).get("NodeName") == name:
                return i
    return 0


def root_motion(ex, sets, clip):
    """A clip's root reference's place at its end (glTF axes, metres)."""
    bones, _ = clip_tracks(ex, sets, clip)
    if not bones:
        return None
    for b, (pos, _) in bones.items():
        if "Root_Reference" in b:
            return np.asarray(pos[-1], np.float64)
    return None


def picker_lines(mp, v, seqs, prefix):
    """A TnStanceAnimationPicker's stances' animations, as `prefix` + [stance, clip]
    lines."""
    i = cd._obj(v)
    if i <= 0:
        return []
    out = []
    for e in cd._structs(mp.pkg, mp.props(i).get("CustomAnimations")):
        anim = str(e.get("Animation"))
        if anim.lower() in seqs:
            out.append("\t".join(prefix + [str(e.get("Stance")), seqs[anim.lower()][0]]))
    return out


def blend_lines(mp, tree, ex, sets, seqs, groups):
    """The tree's velocity blends and its jump blend, as ai_anims.txt's lines."""
    lines = []
    stance_of = {}
    blend = tree_node(mp, tree, "TnStanceAnimBlend")
    if blend:
        for c in cd._structs(mp.pkg, mp.props(blend).get("Children")):
            stance_of[cd._obj(c.get("Anim"))] = str(c.get("Name"))
    # (TnStraferAnimBlend: its children one unit along their ways.)
    for b in [i for full, i in mp.by_full.items() if full.startswith(tree + ".")
              and mp.pkg.class_name(mp.pkg.exports[i - 1]) == "TnStraferAnimBlend"]:
        pr = mp.props(b)
        name = str(pr.get("NodeName", "Blend%d" % b))
        speed = pr.get("_BlendSpeed", 0.2)
        lines.append("\t".join(["vblend", name, stance_of.get(b, "None")]))
        lines.append("\t".join(["strafer", name, "%.4g" % (float(speed) if isinstance(speed, float) else 0.2)]))
        ways = {"Forward": (1, 0), "Backward": (-1, 0), "Right": (0, 1), "Left": (0, -1)}
        for k, c in enumerate(cd._structs(mp.pkg, pr.get("Children"))):
            a = cd._obj(c.get("Anim"))
            cpr = mp.props(a) if a > 0 else {}
            clip, grp = seq_of(mp, a, seqs) if a > 0 else ("-", "-")
            synch = cpr.get("SynchGroupName")
            synch = str(synch) if isinstance(synch, str) and synch not in ("", "None") else "-"
            v = ways.get(str(c.get("Name")), (0, 0))
            lines.append("\t".join(["vchild", name, str(k), str(c.get("Name")), clip, grp, str(v[0]), str(v[1]), synch]))
    # (TnFlyingRobotAnimBlend: a flying robot's, its seven children by the way it
    # flies, its speeds' ranges.)
    for b in [i for full, i in mp.by_full.items() if full.startswith(tree + ".")
              and mp.pkg.class_name(mp.pkg.exports[i - 1]) == "TnFlyingRobotAnimBlend"]:
        pr = mp.props(b)
        name = fly_name(mp, b)

        def rng(key):
            v = pr.get(key)
            if isinstance(v, tuple) and v[0] == "struct":
                f = cd._struct_props(mp.pkg, v) if hasattr(cd, "_struct_props") else {}
                if "Min" in f or "Max" in f:
                    return (float(f.get("Min", 0.0)), float(f.get("Max", 0.0)))
            return FLY_DEFAULTS[key]

        def num(key):
            v = pr.get(key)
            return float(v) if isinstance(v, float) else FLY_DEFAULTS[key]
        fb, rl, ud = rng("ForwardBackwardSpeeds"), rng("RightLeftSpeeds"), rng("UpDownSpeeds")
        lines.append("\t".join(["flyblend", name, "%.4g" % num("BlendSpeed"), "%.4g" % num("MinDiagonalWeight")]
                               + ["%.4g" % x for x in fb + rl + ud]))
        for k, c in enumerate(cd._structs(mp.pkg, pr.get("Children"))):
            a = cd._obj(c.get("Anim"))
            cpr = mp.props(a) if a > 0 else {}
            clip, grp = seq_of(mp, a, seqs) if a > 0 else ("-", "-")
            synch = cpr.get("SynchGroupName")
            synch = str(synch) if isinstance(synch, str) and synch not in ("", "None") else "-"
            lines.append("\t".join(["fchild", name, str(k), str(c.get("Name")), clip, grp, synch]))
    for b in [i for full, i in mp.by_full.items() if full.startswith(tree + ".")
              and mp.pkg.class_name(mp.pkg.exports[i - 1]) == "TnLinearVelocityBlend"]:
        # (The stance profile it is under: the stance blend's child, or the
        # velocity switch's moving state under it.)
        stance = stance_of.get(b)
        if stance is None:
            stance = "None"
        pr = mp.props(b)
        name = str(pr.get("NodeName", "Blend%d" % b))
        rate = float(pr.get("RateScale", 1.0)) if isinstance(pr.get("RateScale"), float) else 1.0
        lines.append("\t".join(["vblend", name, stance] + [str(i) for i in ints(pr.get("TriangleIndices"))]))
        for k, c in enumerate(cd._structs(mp.pkg, pr.get("Children"))):
            a = cd._obj(c.get("Anim"))
            cpr = mp.props(a) if a > 0 else {}
            # (A turn-in-place node in the idle's place: its source's sequence.)
            if a > 0 and mp.pkg.class_name(mp.pkg.exports[a - 1]) == "TnAnimTurnInPlace":
                for cc in cd._structs(mp.pkg, cpr.get("Children")):
                    if str(cc.get("Name")) == "Source":
                        cpr = mp.props(cd._obj(cc.get("Anim")))
            seq = str(cpr.get("AnimSeqName", "-"))
            grp = str(cpr.get("ChooserName", "-"))
            clip = seqs[seq.lower()][0] if seq.lower() in seqs else "-"
            v = (0.0, 0.0)
            if k > 0 and clip != "-":
                _, _, sm, q = seqs[clip.lower()]
                length = float(sm.props(q).get("SequenceLength") or 1.0)
                end = root_motion(ex, sets, clip)
                if end is not None:
                    # (glTF's x, z are Unreal's x, y: metres to uu, over its length.)
                    v = (end[0] * 100.0 * rate / length, end[2] * 100.0 * rate / length)
            synch = cpr.get("SynchGroupName")
            synch = str(synch) if isinstance(synch, str) and synch not in ("", "None") else "-"
            lines.append("\t".join(["vchild", name, str(k), str(c.get("Name")), clip, grp, "%.4g" % v[0], "%.4g" % v[1],
                                     synch]))
            # (A TnStanceAnimNodeSequence: its picker's animations by stance.)
            if "StanceAnimations" in cpr:
                lines += picker_lines(mp, cpr.get("StanceAnimations"), seqs, ["vpick", name, str(k)])
    j = tree_node(mp, tree, "TnJumpTrajectoryBlend")
    if j:
        pr = mp.props(j)
        lines.append("\t".join(["jblend"] + [str(i) for i in ints(pr.get("TriangleIndices"))]))
        landings = mp.names(pr.get("LandingAnimations"))
        for k, c in enumerate(cd._structs(mp.pkg, pr.get("Children"))):
            if k == 0:
                continue
            cpr = mp.props(cd._obj(c.get("Anim")))
            seq = str(cpr.get("AnimSeqName", "-"))
            clip = seqs[seq.lower()][0] if seq.lower() in seqs else "-"
            end = root_motion(ex, sets, clip) if clip != "-" else None
            x, z = (end[0] * 100.0, end[1] * 100.0) if end is not None else (0.0, 0.0)
            land = landings[k - 1] if k - 1 < len(landings) else "-"
            lines.append("\t".join(["jchild", str(k), str(c.get("Name")), clip, "%.4g" % x, "%.4g" % z, land]))
    return lines


def node_class(mp, i):
    return mp.pkg.class_name(mp.pkg.exports[i - 1]) if i > 0 else ""


def children(mp, i):
    """A node's children: [(name, export)]."""
    return [(str(c.get("Name")), cd._obj(c.get("Anim"))) for c in cd._structs(mp.pkg, mp.props(i).get("Children"))]


def seq_of(mp, i, seqs):
    """A sequence node's clip (by its own name) and chooser group ("-" for none)."""
    pr = mp.props(i)
    seq = str(pr.get("AnimSeqName", "-"))
    clip = seqs[seq.lower()][0] if seq.lower() in seqs else "-"
    grp = pr.get("ChooserName")
    return clip, (str(grp) if isinstance(grp, str) and grp not in ("", "None") else "-")


def through(mp, i):
    """The node a wrapper stands for: a turn in place's or an aimer's source, a
    state's input, a state machine switch's first state (its default), a
    TnVelocityAnimBlend whose walk and jog are one node that node."""
    for _ in range(16):
        cls = node_class(mp, i)
        kids = children(mp, i)
        if cls in ("TnAnimTurnInPlace", "AmpAnimState", "TnAnimNodeAimer"):
            nxt = next((c for name, c in kids if name in ("Source", "In", "Input")), 0)
        elif cls == "AmpAnimSwitch":
            nxt = kids[0][1] if kids else 0
        elif cls == "TnVelocityAnimBlend" and len(kids) == 2 and kids[0][1] == kids[1][1]:
            nxt = kids[0][1]
        else:
            return i
        if nxt <= 0:
            return i
        i = nxt
    return i


def describe(mp, i, seqs):
    """A stance profile's node, as a sprofile line's kind and args."""
    if is_vehicle_form_switch(mp, i):
        return ["plane"]
    i = through(mp, i)
    cls = node_class(mp, i)
    if cls in ("AnimNodeSequence", "TnAutoPlayAnimNodeSequence", "TnStanceAnimNodeSequence"):
        return ["seq"] + list(seq_of(mp, i, seqs))
    if cls == "AnimNodeAdditiveBlending":
        kids = dict(children(mp, i))
        return ["add"] + list(seq_of(mp, kids["Base Anim"], seqs)) + list(seq_of(mp, kids["Additive Anim"], seqs))
    if cls == "TnVelocitySwitch":
        return ["switch"]
    if cls in ("TnLinearVelocityBlend", "TnStraferAnimBlend"):
        return ["vblend", str(mp.props(i).get("NodeName", "Blend%d" % i))]
    if cls == "TnFlyingRobotAnimBlend":
        return ["fly", fly_name(mp, i)]
    return ["seq", "-", "-"]


# The nodes a tree's base pose may be (the bottom of its spine; and a plane's
# vehicle form switch: is_vehicle_form_switch).
BASE_CLASSES = ("TnStanceAnimBlend", "TnVelocitySwitch", "TnLinearVelocityBlend", "TnFlyingRobotAnimBlend")
# A plane's tree's state machine switch between its hovering and its boosting
# (VEH_Plane_ANIMTREE's AmpAnimSwitch 'VehicleForm': its selector its form's
# _Boosting, false its first state, true its second).
VEHICLE_FORM_SELECTOR = "_AltForm._Boosting"


def is_vehicle_form_switch(mp, i):
    return (i > 0 and node_class(mp, i) == "AmpAnimSwitch"
            and str(mp.props(i).get("SelectorProperty", "")) == VEHICLE_FORM_SELECTOR)


def fly_name(mp, i):
    return str(mp.props(i).get("NodeName", "Fly%d" % i))


# TnFlyingRobotAnimBlend's defaults (TransGame.xxx's Default__): the speeds (uu a
# second) over which each way's clip takes over, the least each of two sideways
# ways weighs for both to play, and its weights' blend speed.
FLY_DEFAULTS = {"ForwardBackwardSpeeds": (300.0, 1500.0), "RightLeftSpeeds": (100.0, 1000.0),
                "UpDownSpeeds": (500.0, 1500.0), "MinDiagonalWeight": 0.5, "BlendSpeed": 5.0}


def spine(mp, tree):
    """The tree's nodes from its root's child down to its base pose, each node's
    first child the next (a slot's source, a blend's idle, a mirror's or an
    aimer's input): [(class, export)]."""
    root = mp.find(tree)
    kids = children(mp, root)
    i = kids[0][1] if kids else 0
    out = []
    while i > 0:
        cls = node_class(mp, i)
        out.append((cls, i))
        if cls in BASE_CLASSES or is_vehicle_form_switch(mp, i):
            break
        kids = children(mp, i)
        i = kids[0][1] if kids else 0
    return out


def chain_lines(mp, tree):
    """The tree's nodes over its base pose, from the bottom up, as chain lines, and
    its masked slots ([(slot, first bone)])."""
    lines, masked = [], []
    for cls, i in reversed(spine(mp, tree)[:-1]):
        pr = mp.props(i)
        name = str(pr.get("NodeName", ""))
        if cls == "HmAnimNodeScriptableSlot":
            logic = node_class(mp, cd._obj(pr.get("LogicTemplate")))
            if logic == "TnAcrobaticsSlotLogic":
                lines.append("chain\tacro")
        elif cls == "TnAnimNodeAimer":
            lines.append("chain\taim")
        elif cls in AIM_OFFSET_CLASSES:
            lines.append("chain\taimoffset")
        elif cls == "AnimNodeBlendMultiBone":
            target = children(mp, i)[1][1] if len(children(mp, i)) > 1 else 0
            slot = str(mp.props(target).get("NodeName", "")) if target else ""
            targets = cd._structs(mp.pkg, pr.get("BlendTargetList"))
            bone = str(targets[0].get("InitTargetStartBone")) if targets else ""
            if slot and bone:
                lines.append("\t".join(["chain", "masked", slot, bone]))
                masked.append((slot, bone))
        elif cls == "AnimNodeSlot":
            lines.append("\t".join(["chain", "slot", name]))
        elif cls == "TnJumpTrajectoryBlend":
            lines.append("chain\tjump")
        elif cls == "TnAnimNodeHandednessMirror":
            lines.append("chain\tmirror")
        elif cls == "TnAnimNodeMirror":
            lines.append("chain\tnodemirror")
    return lines, masked


def stance_lines(mp, tree, seqs):
    """The stance blend's profiles and transitions (a tree without one: its base
    pose as STANCE_None's profile), and the velocity switches' states."""
    lines = []
    base = spine(mp, tree)[-1]
    if base[0] == "TnStanceAnimBlend":
        blend = base[1]
        kids = children(mp, blend)
        profiles = [(str(p.get("Stance")), float(p.get("BlendTime", 0.0)), kids[k + 1][1] if k + 1 < len(kids) else 0,
                     cd._structs(mp.pkg, p.get("Transitions")))
                    for k, p in enumerate(cd._structs(mp.pkg, mp.props(blend).get("Profiles")))]
    else:
        profiles = [("STANCE_None", 0.0, base[1], [])]
    for k, (stance, blend_time, node, transitions) in enumerate(profiles):
        lines.append("\t".join(["sprofile", str(k), stance, "%.4g" % blend_time] + describe(mp, node, seqs)))
        for t in transitions:
            anim = str(t.get("AnimName"))
            clip = seqs[anim.lower()][0] if anim.lower() in seqs else "-"
            lines.append("\t".join(["strans", str(k), str(t.get("TargetStance")), clip,
                                     "1" if t.get("Reverse") is True else "0",
                                     "%.4g" % float(t.get("BlendInTime", 0.0)),
                                     "%.4g" % float(t.get("BlendOutTime", 0.0))]))
        if node_class(mp, node) == "TnVelocitySwitch":
            states = dict(children(mp, node))
            idle = describe(mp, states["Idle"], seqs)
            moving = describe(mp, states["Moving"], seqs)
            lines.append("\t".join(["switch", idle[1] if idle[0] == "seq" else "-",
                                     moving[1] if moving[0] == "vblend" else "-"]))
            lines += turn_lines(mp, states["Idle"], seqs)
    return lines


# HmAnimTransitionWithBlend's, TnAccelerationAnimBlend's and TnRollAnimBlend's
# defaults (their classes' Default__): the blend in and out, the top speed and
# least input, the roll at which a turn's boost clip takes over.
TRANSITION_BLEND = (0.25, 0.25)
ACCEL_DEFAULTS = {"MaxVelocity": 2000.0, "MinNormalizedInput": 0.0}
ROLL_MAX_ANGLE = 45.0


def plane_lines(mp, tree, seqs):
    """A plane's tree's vehicle form switch (VEH_Plane_ANIMTREE's 'VehicleForm':
    hovering, or boosting) as its lines:

      plane    <max velocity> <min input> <max roll>
               (the hover's TnAccelerationAnimBlend and the boost's TnRollAnimBlend)
      phover   <idle> <forward> <backward> <right> <left> <up> <down>
      padd     <clip> <group>          (the hover's additive: AnimNodeAdditiveBlending)
      pboost   <left> <centre> <right> (the boost's roll blend)
      pboostin <clip>                  (TnAnimTriggerSwitch's transition as the boost
                                        comes into play: its auto-play clip)
      pout     <state 0 hovering/1 boosting> <blend in> <blend out> <clip> <collision clip>
               (each state's transition out: HmAnimTransitionWithBlend, its
               TransitionAnim's clip by the form's BoostMalfunction, Normal or
               Collision; "-" for none)"""
    base = spine(mp, tree)[-1][1] if spine(mp, tree) else 0
    if not is_vehicle_form_switch(mp, base):
        return []

    def clip_of(i):
        i = through(mp, i)
        if i <= 0 or node_class(mp, i) not in ("AnimNodeSequence", "TnAutoPlayAnimNodeSequence"):
            return "-", "-"
        return seq_of(mp, i, seqs)

    def num(pr, key, default):
        v = pr.get(key)
        return float(v) if isinstance(v, (int, float)) and not isinstance(v, bool) else default

    states = children(mp, base)
    lines = []
    hover = boost = None
    outs = []
    for k, (name, state) in enumerate(states[:2]):
        parts = dict(children(mp, state))
        content, out = parts.get("In", 0), parts.get("Default", 0)
        if k == 0:
            hover = content
        else:
            boost = content
        blend_in, blend_out, normal, collision = TRANSITION_BLEND + ("-", "-")
        if out > 0 and node_class(mp, out) == "HmAnimTransitionWithBlend":
            pr = mp.props(out)
            blend_in = num(pr, "BlendInTime", TRANSITION_BLEND[0])
            blend_out = num(pr, "BlendOutTime", TRANSITION_BLEND[1])
            anim = dict(children(mp, out)).get("TransitionAnim", 0)
            if anim > 0 and node_class(mp, anim) == "AmpAnimSwitch":
                picks = dict(children(mp, anim))
                normal = clip_of(picks.get("Normal", 0))[0]
                collision = clip_of(picks.get("Collision", 0))[0]
            elif anim > 0:
                normal = clip_of(anim)[0]
        outs.append("\t".join(["pout", str(k), "%.4g" % blend_in, "%.4g" % blend_out, normal, collision]))
    # The hover: an additive blending of its acceleration blend and its additive.
    accel, add = 0, 0
    if hover and node_class(mp, hover) == "AnimNodeAdditiveBlending":
        kids = dict(children(mp, hover))
        accel, add = kids.get("Base Anim", 0), kids.get("Additive Anim", 0)
    elif hover:
        accel = hover
    max_roll, boost_clips, boost_in = ROLL_MAX_ANGLE, ["-"] * 3, "-"
    # The boost: its trigger switch's steady state (the roll blend) and its
    # transition (a switch on the boosting: its auto-play clip).
    if boost and node_class(mp, boost) == "TnAnimTriggerSwitch":
        kids = children(mp, boost)
        steady = through(mp, kids[0][1]) if kids else 0
        if steady > 0 and node_class(mp, steady) == "TnRollAnimBlend":
            max_roll = num(mp.props(steady), "MaxAngle", ROLL_MAX_ANGLE)
            boost_clips = [clip_of(a)[0] for _, a in children(mp, steady)][:3]
        if len(kids) > 1:
            t = through(mp, kids[1][1])
            if node_class(mp, t) == "AmpAnimSwitch":
                picks = children(mp, t)
                t = picks[1][1] if len(picks) > 1 else 0
            boost_in = clip_of(t)[0]
    pr = mp.props(accel) if accel > 0 else {}
    lines.append("\t".join(["plane", "%.4g" % num(pr, "MaxVelocity", ACCEL_DEFAULTS["MaxVelocity"]),
                            "%.4g" % num(pr, "MinNormalizedInput", ACCEL_DEFAULTS["MinNormalizedInput"]),
                            "%.4g" % max_roll]))
    hover_clips = [clip_of(a)[0] for _, a in children(mp, accel)] if accel > 0 else []
    lines.append("\t".join(["phover"] + (hover_clips + ["-"] * 7)[:7]))
    if add > 0:
        lines.append("\t".join(["padd"] + list(clip_of(add))))
    lines.append("\t".join(["pboost"] + (boost_clips + ["-"] * 3)[:3]))
    lines.append("\t".join(["pboostin", boost_in]))
    return lines + outs


# TnAnimTurnInPlace's defaults: its turns (degrees, right positive, and their
# transitions' names), its blend time, threshold angle and look ahead.
TURNS = [(90.0, "Rt_90"), (180.0, "Rt_180"), (-90.0, "Lt_90"), (-180.0, "Lt_180")]


def turn_lines(mp, i, seqs):
    """The idle state's TnAnimTurnInPlace (under its state, if it is one): each
    turn's clip, its player's transition's picker's, and its parameters."""
    for _ in range(4):
        if node_class(mp, i) != "AmpAnimState":
            break
        i = next((c for name, c in children(mp, i) if name == "In"), 0)
    if node_class(mp, i) != "TnAnimTurnInPlace":
        return []
    pr = mp.props(i)
    turns = [(float(t.get("Angle", 0.0)), str(t.get("AnimName"))) for t in cd._structs(mp.pkg, pr.get("_TransitionAnimations"))] or TURNS
    player = next((c for name, c in children(mp, i) if name != "Source"), 0)
    picks = {}
    if player > 0:
        for t in cd._structs(mp.pkg, mp.props(player).get("Transitions")):
            picks[str(t.get("TransName"))] = picker_anim(mp, t.get("Animations"))
    lines = []
    for angle, name in turns:
        anim = picks.get(name, "None")
        if anim.lower() in seqs:
            lines.append("\t".join(["turn", "%.4g" % angle, seqs[anim.lower()][0]]))
    if lines:
        lines.append("\t".join(["turnparams", "%.4g" % float(pr.get("_BlendTime", 0.2)),
                                 "%.4g" % float(pr.get("_TransitionThresholdAngle", 20.0)),
                                 "%.4g" % float(pr.get("_LookAheadTime", 0.5))]))
    return lines


def picker_anim(mp, v):
    """A TnStanceAnimationPicker's default animation."""
    i = cd._obj(v)
    return str(mp.props(i).get("DefaultAnimation", "None")) if i > 0 else "None"


def acro_lines(mp, tree, seqs):
    """The acrobatics slot's jump and fall animations."""
    lines = []
    logic = tree_node(mp, tree, "TnAcrobaticsSlotLogic")
    if not logic:
        return lines
    pr = mp.props(logic)

    def anim(struct, state, looping):
        st = cd._struct_props(mp.pkg, struct) if struct is not None else {}
        name = picker_anim(mp, st.get("Animations"))
        if name.lower() not in seqs:
            return
        lines.append("\t".join(["acro", str(state), seqs[name.lower()][0],
                                "%.4g" % float(st.get("BlendInTime", ACRO_BLEND[0])),
                                "%.4g" % float(st.get("BlendOutTime", ACRO_BLEND[1])),
                                "1" if looping else "0"]))
    jump = cd._struct_props(mp.pkg, pr["SingleJumpAnimations"]) if "SingleJumpAnimations" in pr else {}
    anim(jump.get("EnterState"), 0, False)
    anim(jump.get("LoopState"), 100, True)
    anim(pr.get("FallingAnimations"), 5, True)
    return lines


def skeleton(ref):
    """The reference skeleton: ({bone: its parent bone, None at the top}, {bone:
    (translation, rotation)} at rest)."""
    nodes = ref.g["nodes"]
    parent = {}
    for i, n in enumerate(nodes):
        for c in n.get("children", []):
            parent[c] = i
    bones = {nodes[j]["name"] for j in ref.g["skins"][0]["joints"]}
    par, rest = {}, {}
    for i, n in enumerate(nodes):
        name = n.get("name")
        if name not in bones:
            continue
        p = parent.get(i)
        par[name] = nodes[p].get("name") if p is not None and nodes[p].get("name") in bones else None
        rest[name] = (np.array(n.get("translation", [0, 0, 0]), float), np.array(n.get("rotation", [0, 0, 0, 1]), float))
    return par, rest


def bone_matrix(bone, tracks, frame, par, rest):
    """A bone's matrix at a clip's frame in the root bone's space (as the aimer
    measures: less the root bone's own turn)."""
    chain = []
    b = bone
    while b is not None and b != ROOT_REF:
        chain.append(b)
        b = par.get(b)
    m = np.eye(4)
    for b in reversed(chain):
        t, r = rest[b]
        if b in tracks:
            pos, rot = tracks[b]
            if len(pos):
                t = np.asarray(pos[min(frame, len(pos) - 1)], float)
            if len(rot):
                r = np.asarray(rot[min(frame, len(rot) - 1)], float)
        local = np.eye(4)
        local[:3, :3] = quat_to_mat(r)
        local[:3, 3] = t
        m = m @ local
    return m


def aim_of(tracks, bone, par, rest, invert=True, frame=0):
    """Where a pose points the aim bone (TnAnimNodeAimer: the bone's rotation, half
    turned if InvertAimBone, less the root bone's): the aimer's units (yaw, pitch)."""
    m = bone_matrix(bone, tracks, frame, par, rest)
    d = m[:3, :3] @ np.array([-1.0 if invert else 1.0, 0.0, 0.0])
    # (glTF's x, y up, z are Unreal's x, z up, y.)
    x, y, z = d[0], d[2], d[1]
    return (math.degrees(math.atan2(y, x)) / AIM_UNIT,
            math.degrees(math.atan2(z, math.hypot(x, y))) / AIM_UNIT)


def quat_mul(a, b):
    ax, ay, az, aw = a
    bx, by, bz, bw = b
    return np.array([aw * bx + ax * bw + ay * bz - az * by,
                     aw * by - ax * bz + ay * bw + az * bx,
                     aw * bz + ax * by - ay * bx + az * bw,
                     aw * bw - ax * bx - ay * by - az * bz])


def root_track(tracks):
    """The root bone's motion each frame from the first: (forward, right, up uu,
    yaw degrees), or None if it stays put."""
    found = [v for b, v in tracks.items() if "Root_Reference" in b]
    if not found:
        return None
    pos, rot = found[0]
    pos = np.asarray(pos, float) * 100.0
    rot = np.asarray(rot, float)
    n = max(len(pos), len(rot))
    q0 = rot[0] * np.array([-1.0, -1.0, -1.0, 1.0])
    out = []
    for k in range(n):
        p = pos[min(k, len(pos) - 1)] - pos[0]
        rel = quat_mul(rot[min(k, len(rot) - 1)], q0)
        v = quat_to_mat(rel) @ np.array([1.0, 0.0, 0.0])
        # (glTF z is Unreal's y: a yaw to the right turns x towards it.)
        out.append((p[0], p[2], p[1], math.degrees(math.atan2(v[2], v[0]))))
    out = np.array(out)
    if np.abs(out[:, :3]).max() < STILL_ROOT[0] and np.abs(out[:, 3]).max() < STILL_ROOT[1]:
        return None
    return out


def aim_lines(mp, tree, seqs, ex, sets, par, rest):
    """The aimer's lines (the main one's: TnAnimNodeAimer with a profile)."""
    for full, i in mp.by_full.items():
        if not (full.startswith(tree + ".") and node_class(mp, i) == "TnAnimNodeAimer"):
            continue
        pr = mp.props(i)
        group = cd._obj(pr.get("ProfileGroup"))
        profiles = cd._structs(mp.pkg, mp.props(group).get("Profiles")) if group > 0 else []
        if not profiles:
            continue
        p = profiles[0]
        # (The profile's aim bone, or the group's where it has none.)
        bone = str(p.get("AimBoneName", "None"))
        if bone in ("None", ""):
            bone = str(mp.props(group).get("AimBoneName"))
        ref = str(p.get("ReferencePose"))
        poses = [str(p[k]) for k in sorted(p, key=lambda k: (len(k), k)) if k.startswith("AimPoses")]
        invert = p.get("InvertAimBone") is True
        speed = pr.get("BlendSpeed", 10.0)
        stances = mp.names(pr.get("Stances"))
        lines = ["\t".join(["aim", bone, seqs[ref.lower()][0], "%.4g" % float(speed)] + stances)]
        rt, _ = clip_tracks(ex, sets, seqs[ref.lower()][0])
        rx, ry = aim_of(rt, bone, par, rest, invert)
        for k, pose in enumerate(poses):
            if pose.lower() not in seqs:
                continue
            t, _ = clip_tracks(ex, sets, seqs[pose.lower()][0])
            x, y = aim_of(t, bone, par, rest, invert)
            lines.append("\t".join(["aimpose", str(k), seqs[pose.lower()][0], "%.4g" % (x - rx), "%.4g" % (y - ry)]))
        return lines, bone, invert
    return [], None, True


# The aim offset nodes (UE3's AnimNodeAimOffset, with its offsets baked from the
# aim poses): High Moon's (the Snipers' insects': its aim the pawn's AimOffsetPct)
# and Transformers' (the Guard's, the Leaper's: that aim eased in at its
# InterpSpeed, mirrored while the pawn is left-handed, its profile by stance), and
# the latter's defaults (TransGame.xxx's Default__TnAnimNodeAimOffset).
AIM_OFFSET_CLASSES = ("TnAnimNodeAimOffset", "HmAnimNodeAimOffset")
AIM_OFFSET_DEFAULTS = {"TnAnimNodeAimOffset": (12.0, True), "HmAnimNodeAimOffset": (0.0, False)}
# An aim component's offsets, in the order its lines give them.
AIM_DIRECTIONS = ("LU", "LC", "LD", "CU", "CC", "CD", "RU", "RC", "RD")


def native(v, fmt_, n, default):
    """A native struct's floats (a Vector2D's, a Vector's, a Quat's)."""
    if isinstance(v, tuple) and len(v) > 2 and isinstance(v[2], (bytes, bytearray)) and len(v[2]) >= 4 * n:
        return struct.unpack(fmt_, v[2][:4 * n])
    return default


def aim_offset_lines(mp, tree):
    """The aim offset node's lines (the first in the tree's spine): its eases and
    mirroring, its profiles' ranges (and the pawn's aim ranges those that remap it
    take for theirs), their bones' offsets (glTF's axes: a turn put in front of the
    bone's in the mesh's space, and a move of its place there, metres) and the
    stances that pick them."""
    for cls, i in spine(mp, tree):
        if cls not in AIM_OFFSET_CLASSES:
            continue
        pr = mp.props(i)
        speed, switch = AIM_OFFSET_DEFAULTS[cls]
        if isinstance(pr.get("InterpSpeed"), float):
            speed = pr["InterpSpeed"]
        if isinstance(pr.get("SwitchAimWhenLeftHanded"), bool):
            switch = pr["SwitchAimWhenLeftHanded"]
        lines = ["\t".join(["aimoffset", cls, "%.4g" % speed, "1" if switch else "0"])]
        names = []
        for k, p in enumerate(cd._structs(mp.pkg, pr.get("Profiles"))):
            name = str(p.get("ProfileName", "Default"))
            names.append(name)
            h = native(p.get("HorizontalRange"), "<2f", 2, (-1.0, 1.0))
            v = native(p.get("VerticalRange"), "<2f", 2, (-1.0, 1.0))
            lines.append("\t".join(["aoprofile", str(k), name] + ["%.4g" % x for x in h + v]))
            if p.get("RemapPawnAimRange") is True:
                r = cd._struct_props(mp.pkg, p["PawnAimOffsetRange"]) if "PawnAimOffsetRange" in p else {}
                ph = native(r.get("HorizontalRange"), "<2f", 2, (-1.0, 1.0))
                pv = native(r.get("VerticalRange"), "<2f", 2, (-1.0, 1.0))
                lines.append("\t".join(["aoremap", str(k)] + ["%.4g" % x for x in ph + pv]))
            for comp in cd._structs(mp.pkg, p.get("AimComponents")):
                row = ["aobone", str(k), str(comp.get("BoneName"))]
                for d in AIM_DIRECTIONS:
                    t = comp.get(d)
                    sp = cd._struct_props(mp.pkg, t) if isinstance(t, tuple) else {}
                    qx, qy, qz, qw = native(sp.get("Quaternion"), "<4f", 4, (0.0, 0.0, 0.0, 1.0))
                    tx, ty, tz = native(sp.get("Translation"), "<3f", 3, (0.0, 0.0, 0.0))
                    # (Unreal's y and z swapped, a mirror: a turn's axis flips.)
                    row.append(" ".join("%.6g" % x for x in (-qx, -qz, -qy, qw, tx * 0.01, tz * 0.01, ty * 0.01)))
                lines.append("\t".join(row))
        for m in cd._structs(mp.pkg, pr.get("StanceToProfileMappingOverrides")):
            want = str(m.get("ProfileName"))
            if want in names:
                lines.append("\t".join(["aostance", str(m.get("Stance")), str(names.index(want))]))
        return lines
    return []


def fmt(v):
    return " ".join("%.4g" % x for x in v)


def aim_delta(pose, ref):
    """An aim pose as the additive offset from the reference pose: each bone's turn
    put in front of the reference's (pose = delta * ref) and its offset from the
    reference's place."""
    out = {}
    for bone, (pos, rot) in pose.items():
        if bone not in ref:
            continue
        rpos, rrot = ref[bone]
        p = np.asarray(pos, np.float64)[:1] - np.asarray(rpos, np.float64)[:1]
        q = np.asarray(rot, np.float64)[:1]
        r = np.asarray(rrot, np.float64)[:1]
        # delta = q * r^-1 (x, y, z, w quaternions).
        rx, ry, rz, rw = -r[0, 0], -r[0, 1], -r[0, 2], r[0, 3]
        qx, qy, qz, qw = q[0]
        d = np.array([[qw * rx + qx * rw + qy * rz - qz * ry,
                       qw * ry - qx * rz + qy * rw + qz * rx,
                       qw * rz + qx * ry - qy * rx + qz * rw,
                       qw * rw - qx * rx - qy * ry - qz * rz]])
        out[bone] = (p.astype(np.float32), d.astype(np.float32))
    return out


def bone_paths(w, root):
    """Each node's path of names from `root` ({name: [names]})."""
    out = {}

    def walk(i, path):
        n = w.g["nodes"][i]
        p = path + [n.get("name", "GltfNode%d" % i)]
        out[p[-1]] = p
        for c in n.get("children", []):
            walk(c, p)
    walk(root, [])
    return out


def build_set(name, type_ids, data, ex, cooked, assets, mps):
    """A set's clips and data: ai_<name>.glb, its mirror's, ai_anims_<name>.txt."""
    t = data["types"][base_ids(type_ids)[0]]
    # (The form it starts in: a Swarmer's its insect's; an other form's own.)
    form = form_for(data, type_ids[0])
    # The anim sets: the first type's, the others' own after (theirs first: the
    # first type's win a name they share).
    sets = list(form["anim_sets"])
    for other in type_ids[1:]:
        for s in form_for(data, other)["anim_sets"]:
            if s not in sets:
                sets.insert(0, s)
    tree = form["anim_tree"]
    scale = float(form["scale"] or 1.0)
    half_height = float(form["collision"][1])
    mp = mps.get(t["package"]) or cd.Mp(cooked, t["package"])
    mps[t["package"]] = mp
    sh = mps[SHARED]
    ref_pose, aim_poses, aim_set = aim_profile(mp, tree)
    # (The aimer's poses from its profile's own set, if the types haven't it.)
    if aim_set and aim_set not in sets:
        sets.insert(0, aim_set)
    seqs, groups = sequences([mp] + [m for k, m in sorted(mps.items()) if m is not mp], sets)
    names, looping = tree_names(mp, tree)
    names |= blueprint_names(assets, base_ids(type_ids))
    masks, melee = masked_clips(assets, base_ids(type_ids))
    names |= melee
    names |= SPAWN_CLIPS
    if t["behaviour"] in be.TRANSFORMING:
        half = "ROBO" if form_kind(data, type_ids[0]) == "robot" else "VEH"
        names |= {c + half for c in TRANSFORM_CLIPS}
    clips, grps = resolve(names, seqs, groups)
    loops = set()
    for n in looping:
        c, g = resolve({n}, seqs, groups)
        loops |= c
    aims = {seqs[a.lower()][0] for a in aim_poses if a.lower() in seqs}
    print(f"{name}: {len(clips)} clips, {len(grps)} chooser groups, {len(loops)} looping, {len(aims)} aim poses")
    # The skeleton, from one of the first type's parts.
    ref = Doc(be.skinned_parts(ex, form)[0][3])
    w = Writer(share=True)
    r = w.node({"name": "Root", "children": []}, root=True)
    _, _, joints = bc.copy_skeleton(w, ref, r)
    ref_names = [ref.g["nodes"][j]["name"] for j in ref.g["skins"][0]["joints"]]
    bone_nodes = {n: joints[k] for k, n in enumerate(ref_names)}
    rest = bc.robot_rest(ref)
    # Its sockets (the skeleton's, or the parts' own: the sweeps' sockets).
    sockets = {}
    if form.get("base"):
        sk = sh.find("TR_Skeletons_p." + form["base"])
        sockets = {s["name"]: (s["bone"], s["location"]) for s in (sh.sockets(sk) if sk else [])}
    for slot in form["slots"]:
        for part in slot:
            if part and "mesh" in part:
                for m in (mp, sh):
                    i = m.find(part["package"] + "." + part["mesh"]) if part.get("package") else 0
                    if not i:
                        i = next(iter(m.by_name.get(part["mesh"], [])), 0)
                    if i and i > 0:
                        for s in m.sockets(i):
                            sockets.setdefault(s["name"], (s["bone"], s["location"]))
                        break
    ref_tracks = None
    if ref_pose and ref_pose.lower() in seqs:
        ref_tracks, _ = clip_tracks(ex, sets, seqs[ref_pose.lower()][0])
    lines = ["# The %s AI clips (tools/build_ai_anims.py): see its header for the fields" % name,
             "\t".join(["types"] + type_ids)]
    par, rest_pose = skeleton(ref)
    aim, aim_bone, aim_invert = aim_lines(mp, tree, seqs, ex, sets, par, rest_pose)
    lines += aim
    lines += aim_offset_lines(mp, tree)
    kinds = []
    index = 0
    for clip in sorted(clips):
        _, s, sm, q = seqs[clip.lower()]
        pr = sm.props(q)
        length = float(pr.get("SequenceLength") or 0.0)
        rate = pr.get("RateScale")
        rate = float(rate) if isinstance(rate, float) and rate > 0 else 1.0
        bones, dt = clip_tracks(ex, sets, clip)
        if bones is None:
            print("  ! clip not exported:", clip, "(" + s + ")")
            continue
        if clip in aims and ref_tracks is not None:
            bc.additive_clip(w, bone_nodes, clip, aim_delta(bones, ref_tracks), dt)
            kind = "aim"
        elif clip.upper().startswith("ADD_"):
            bc.additive_clip(w, bone_nodes, clip, bones, dt, loop=clip in loops)
            kind = "additive"
        else:
            bc.base_clip(w, bone_nodes, clip, bones, dt, loop=clip in loops, rest=rest)
            kind = "pose"
        lines.append("\t".join(["clip", clip, "%.4g" % length, "%.4g" % rate, str(index), kind]))
        kinds.append(kind)
        for n in notifies(sm, q, scale, half_height, sockets):
            lines.append("\t".join([n[0], clip] + list(n[1:])))
        if kind == "pose":
            if aim_bone:
                x, y = aim_of(bones, aim_bone, par, rest_pose, aim_invert)
                lines.append("\t".join(["aimof", clip, "%.4g" % x, "%.4g" % y]))
            root = root_track(bones)
            if root is not None:
                lines.append("\t".join(["rootm", clip, "%.6g" % dt] + [fmt(r) for r in root]))
        index += 1
    for g in sorted(grps):
        members = [seqs[m.lower()][0] for m in grps[g] if m.lower() in seqs]
        lines.append("\t".join(["group", g] + members))
    # The tree's nodes over its base, and its masked slots' bones and clips.
    chain, masked = chain_lines(mp, tree)
    lines += chain
    paths = bone_paths(w, r)
    for slot, bone in masked:
        first = paths.get(bone)
        if not first:
            print("  ! masked slot's bone not on the skeleton:", slot, bone)
            continue
        for b, p in sorted(paths.items()):
            if p[:len(first)] == first:
                lines.append("\t".join(["mask", slot, "/".join(p)]))
        mask = SLOT_MASKS.get(slot)
        played = set()
        for n in masks.get(mask, set()):
            c, _ = resolve({n}, seqs, groups)
            played |= c
        played = sorted(c for c in played if c in clips)
        if played:
            lines.append("\t".join(["maskclips", slot] + played))
    lines += blend_lines(mp, tree, ex, sets, seqs, groups)
    lines += stance_lines(mp, tree, seqs)
    lines += plane_lines(mp, tree, seqs)
    lines += acro_lines(mp, tree, seqs)
    out = os.path.join(assets, OUT_DIR)
    path = os.path.join(out, "ai_%s.glb" % name)
    size = w.save(path)
    open(os.path.join(out, "ai_anims_%s.txt" % name), "w").write("\n".join(lines) + "\n")
    print(f"  wrote ai_{name}.glb ({index} clips, {size / 1e6:.1f} MB) and ai_anims_{name}.txt")
    if "chain\tmirror" in chain:
        size, n = mirror_anims.mirror(path, be.skinned_parts(ex, form)[0][3], os.path.join(out, "ai_%s_mirror.glb" % name),
                                      lambda i, _: kinds[i] != "pose", bones=set(ref_names))
        print(f"  wrote ai_{name}_mirror.glb ({n} mirrored clips, {size / 1e6:.1f} MB)")


def set_sequences(type_ids, data, cooked, mps):
    """A set's sequences, as build_set finds them (its types' anim sets, its aim
    profile's), for a pass over its ai_anims.txt."""
    form = form_for(data, type_ids[0])
    sets = list(form["anim_sets"])
    for other in type_ids[1:]:
        for s in form_for(data, other)["anim_sets"]:
            if s not in sets:
                sets.insert(0, s)
    t = data["types"][base_ids(type_ids)[0]]
    mp = mps.get(t["package"]) or cd.Mp(cooked, t["package"])
    mps[t["package"]] = mp
    _, _, aim_set = aim_profile(mp, form["anim_tree"])
    if aim_set and aim_set not in sets:
        sets.insert(0, aim_set)
    seqs, _ = sequences([mp] + [m for k, m in sorted(mps.items()) if m is not mp], sets)
    return seqs


def effects_pass(cooked, assets, data_path=None):
    """--effects: brings the ai_anims_<set>.txt files already built up to date with
    their effects' last field (auto off: HmAnimNotify_PlayEffect's bAutoDeactivate),
    straight from CookedPC, without building the sets again: each fx line is
    matched to its clip's notify (its time, system, socket and the rest) and given
    the field; nothing else in the file changes."""
    data = json.load(open(data_path)) if data_path and data_path != "-" else be.enemy_data.read(cooked)
    # (Every package main reads, for the same sequences.)
    mps = {SHARED: cd.Mp(cooked, SHARED)}
    for package in sorted({t["package"] for t in data["types"].values()}):
        mps[package] = cd.Mp(cooked, package)
    for name, type_ids in SETS:
        path = os.path.join(assets, OUT_DIR, "ai_anims_%s.txt" % name)
        if not os.path.exists(path):
            print("  ! %s: not built" % path)
            continue
        raw = open(path, "rb").read()
        newline = b"\r\n" if b"\r\n" in raw else b"\n"
        lines = raw.decode().split(newline.decode())
        if not any(line.startswith("fx\t") for line in lines):
            continue
        seqs = set_sequences(type_ids, data, cooked, mps)
        found = {}
        changed = missing = 0
        for i, line in enumerate(lines):
            f = line.split("\t")
            if f[0] != "fx" or len(f) < 10:
                continue
            clip = f[1]
            if clip not in found:
                _, _, sm, q = seqs.get(clip.lower(), (None, None, None, None))
                found[clip] = [n for n in notifies(sm, q, 1.0, 0.0, {}) if n[0] == "fx"] if sm else []
            match = [n for n in found[clip] if list(n[1:9]) == f[2:10]]
            if not match:
                missing += 1
                print("  ! %s: no notify for %s" % (name, line.strip()))
                continue
            new = "\t".join(f[:10] + [match[0][9]])
            if new != line:
                lines[i] = new
                changed += 1
        open(path, "wb").write(newline.decode().join(lines).encode())
        print("  %s: %d effects given their auto off%s" % (name, changed, ", %d not found" % missing if missing else ""))


def main(export_dir, cooked, assets, data_path=None, only=None):
    data = json.load(open(data_path)) if data_path and data_path != "-" else be.enemy_data.read(cooked)
    ex = bc.Exports(export_dir)
    mps = {SHARED: cd.Mp(cooked, SHARED)}
    for package in sorted({t["package"] for t in data["types"].values()}):
        mps[package] = cd.Mp(cooked, package)
    wanted = set(only.split(",")) if only else None
    for name, type_ids in SETS:
        if wanted and name not in wanted:
            continue
        build_set(name, type_ids, data, ex, cooked, assets, mps)


if __name__ == "__main__":
    if len(sys.argv) >= 4 and sys.argv[1] == "--effects":
        effects_pass(*sys.argv[2:5])
    elif len(sys.argv) < 4:
        print(__doc__)
        sys.exit(1)
    else:
        main(*sys.argv[1:6])
