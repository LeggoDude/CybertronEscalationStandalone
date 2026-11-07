"""The death sets the enemies and the players die by, read from the game for
src/deaths.rs: how each death plays out (TnDeathTimeline: its keyframes, each a
list of actions started together, the next one starting once all of this one's
are done) and the destructibles the deaths spawn (their gibs, blasts, decals and
sounds).

Every Escalation enemy's chassis names its death set (enemy_data.py's forms:
DeathSet): the soldiers', the jets' robots' and the players' robots'
TR_Deaths_p.Sets.Basic_DEATHSET, the Titan's and the Heavy Gunners'
Titan_DEATHSET, the drones' TitanExplosiveDrone_DEATHSET, the jets' planes'
Flyer_DEATHSET, the Snipers', the Guard's, the Swarmer's and the Leaper's own. A
death set is a list of deaths, each with the death types a kill must have for it
(RequiredTypes); TnDeathPlayer.Pick takes the first whose types the kill has
(TnDeathContextFactory and its robots' and snipers' subclasses gather them: the
damage type's DamageDeathTypes, Robot or Vehicle, the side the blow came from, a
headshot, a gib...). An entry may be a conditional set (TnConditionalDeathSet),
which picks one of its deaths at random by their weights (ChooseWeightedDeath: its
deaths' own required types aren't looked at).

Each action is written with its class's defaults where the death doesn't set its
own (Default__<class> in TransGame.xxx; a struct's fields one by one), its delay
(Delay: at random between two, seconds) and what the demo needs of it:

  anim     PlayAnimation: its clip (AnimName), PlayRate, Looping, CutoffTime (done
           after that long, 0: when it ends)
  ragdoll  Ragdoll: it takes over (UseVelocityTransferTime: `before` seconds before
           the clip ends, -1 at once), done `settle` s after (MaximumRagdollSettleTime:
           not when it comes to rest), the kill's momentum times `momentum` (at
           random between two: MomentumImpulseVariance), its RagdollActions: a radial
           impulse (radial=<lo>,<hi>,<radius uu>,<falloff 0 constant/1 linear>,<origin
           (ETnRagdollImpulseOrigin) 0 the pawn 1 where it was struck 2 a bone 3 the
           bone struck>,<offset x,y,z in the pawn's frame>,<1: a velocity change>,
           <delay lo,hi>[,<the bone, for an origin at one>]),
           halt (its bodies stopped), pieces severed (sever=<bones>;<trail>;
           <probability>;<that many at random, 0: the bones'>;<only those within this
           of where it was struck, uu, or ->), impulses
           (impulse=<x,y,z>;<angular x,y,z>;<target (TnRagdollDeathImpulseTarget): 0
           the bone named, 1 the bone struck>;<bone, - none: the root body>;<1: an
           impulse, 0: a velocity>;<direction variance lo,hi>;<where it is applied
           (UsePointImpulse's PointSource, as a radial's origin, - for its centre of
           mass)>,<offset x,y,z in the pawn's frame>: turned by the kill's
           momentum's direction)
  (any)    hold=<seconds>: its AfterActionWait, the wait once it is done before it
           counts as done
  wait     Wait: WaitTime
  deres    Highlight: its Deresser (Duration, OverlayMaterial, NormalizedTimeToMake-
           MeshInvisible, its Sound), CutoffTime, StartingRampValue, DelayUntilRamp-
           Start, radius: what the overlay's reach (the mesh's bounds' radius
           times 1.2) is scaled by (RadiusAdditionalScale, if UseRadiusAdditional-
           Scale; no death sets RadiusSource, PositionSource's offset or a radius of
           its own), and tiling: its Deresser's MaterialTiling (the overlay's noise)
  sound    PlaySound: its cue (as tools/build_sounds.py names it) or its sound event
  effect   PlayEffect: systems=<system@socket,...> (each attached to its socket, or
           its bone of that name)
  shake    CameraShake: CameraShakeRadius and its ScreenShakeStruct
  dsys     SpawnDestructible: the destructible (one of them at random)
  light    TriggerLight: its RampLightProperties
  gravity  SetGravityFactor
  spin     ApplyAngularForce: roll, pitch and yaw (at random between two), every
           tick or once
  collide  CollisionTimer: done that long after it starts (DurationBeforeCollision),
           or after it hits something (DurationAfterCollision)
  boost    PlaneMotionControl: the plane keeps boosting
  hide     SetMeshHidden
  bone     TnDeathModifierControlBone: a SkelControlSingleBone's BoneScale
  other    anything else (named), done at once

A destructible is its pieces' effects in the state the death's notification takes
it to (HmNotificationDestructionTrigger):

  dsys <name> particle <system> <x y z>                 (at that offset, uu, in the
                                                          dying pawn's frame)
  dsys <name> gibs <meshes,...> <count> <volume x,y,z> <speed lo,hi> <spin lo,hi>
                <cone degrees> <lifespan> <trail system> <x y z> <pitch yaw roll>
                <deres secs> <active sound> <deres sound>
  dsys <name> sound <cue>
  dsys <name> decal <material> <width> <height> <width range> <height range>
                <thickness> <lifetime> <x y z>
  dsys <name> damage <damage type> <damage> <radius> <momentum> <x y z>

usage: python build_death_sets.py <CookedPC> <foc_esc/assets>
"""
import os
import struct
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.path.join(HERE, "ue3"))
sys.path.insert(0, HERE)

import class_data as cd  # noqa: E402
from props import read_props  # noqa: E402
from ue3pkg import Reader  # noqa: E402

# The death sets, and the packages they are found in (each map's own enemies').
SETS = [
    "TR_Deaths_p.Sets.Basic_DEATHSET",
    "TR_Titan_AI_p.Deaths.Titan_DEATHSET",
    "TR_TitanSentryOrb_ROBODEF_p.TitanExplosiveDrone_DEATHSET",
    "TR_FlyerAut_AI_p.Sets.Flyer_DEATHSET",
    "TR_Sniper_AI_p.Death.Sniper_DEATHSET",
    "TR_Guard_AI_p.Deaths.Guardian_DEATHSET",
    "TR_Swarmer_p.Death.Swarmer_DEATHSET",
    "TR_Leaper_AI_p.Death.Leaper_DEATHSET",
]
PACKAGES = ["ESC_Iacon_BASE_m.xxx", "ESC_Temple_BASE_m.xxx", "ESC_Gladiator_BASE_m.xxx", "ESC_Ark_BASE_m.xxx"]
TRANSGAME = "TransGame.xxx"
OUT = os.path.join("models", "enemies", "deaths.txt")


def fmt(x):
    return "%.6g" % x


class Reading:
    """The packages, and the action classes' defaults (TransGame's class default
    objects)."""

    def __init__(self, cooked):
        self.cooked = cooked
        self.tg = cd.Mp(cooked, TRANSGAME)
        self.mps = {}
        self._cdo = {}

    def mp(self, package):
        if package not in self.mps:
            self.mps[package] = cd.Mp(self.cooked, package)
        return self.mps[package]

    def cdo(self, cls):
        """A class's default object's properties, structs parsed (in TransGame's
        names)."""
        if cls not in self._cdo:
            i = self.tg.find("TransGame.Default__" + cls) or self.tg.find("Default__" + cls)
            self._cdo[cls] = parsed(self.tg, self.tg.props(i)) if i else {}
        return self._cdo[cls]


def parsed(mp, pr):
    """Properties with their structs read into dicts (natively serialized ones, the
    vectors and colours, left as their bytes)."""
    out = {}
    for k, v in pr.items():
        if isinstance(v, tuple) and v[0] == "struct":
            inner = None
            try:
                inner = read_props(mp.pkg, Reader(v[2]), len(v[2])) if len(v[2]) > 12 else None
            except Exception:
                inner = None
            out[k] = ("struct", v[1], parsed(mp, inner)) if inner else v
        else:
            out[k] = v
    return out


def merged(defaults, own):
    """A death action's properties: its own over its class's (a struct field by
    field)."""
    out = dict(defaults)
    for k, v in own.items():
        d = out.get(k)
        if (isinstance(v, tuple) and v[0] == "struct" and isinstance(v[2], dict)
                and isinstance(d, tuple) and d[0] == "struct" and isinstance(d[2], dict)):
            out[k] = ("struct", v[1], merged(d[2], v[2]))
        else:
            out[k] = v
    return out


# TnDeathActionRagdoll's enums: ETnRagdollImpulseOrigin (a radial impulse's
# ExplosionOrigin, an impulse's PointSource) and TnRagdollDeathImpulseTarget.
IMPULSE_ORIGINS = {"ETRIO_LocalPosition": 0, "ETRIO_AtHitPostion": 1, "ETRIO_AtSpecificBone": 2,
                   "ETRIO_AtHitBone": 3}
IMPULSE_TARGETS = {"ETRDIT_SpecificBone": 0, "ETRDIT_HitBone": 1}


def impulse_origin(v):
    """An ETnRagdollImpulseOrigin by number (0, ETRIO_LocalPosition: the pawn's
    place, when not set)."""
    return v if isinstance(v, int) else IMPULSE_ORIGINS.get(str(v), 0)


def sget(v, key, default=None):
    """A field of a parsed struct."""
    if isinstance(v, tuple) and v[0] == "struct" and isinstance(v[2], dict):
        return v[2].get(key, default)
    return default


def num(v, default=0.0):
    return float(v) if isinstance(v, (int, float)) and not isinstance(v, bool) else default


def frange(v, default=(0.0, 0.0)):
    """A FloatRange's (Min, Max)."""
    if isinstance(v, tuple) and v[0] == "struct" and isinstance(v[2], dict):
        return (num(v[2].get("Min"), default[0]), num(v[2].get("Max"), default[1]))
    return default


def vec(v, default=(0.0, 0.0, 0.0)):
    if isinstance(v, tuple) and v[0] == "struct" and not isinstance(v[2], dict) and len(v[2]) == 12:
        return struct.unpack("<3f", v[2])
    return default


def rot(v):
    if isinstance(v, tuple) and v[0] == "struct" and not isinstance(v[2], dict) and len(v[2]) == 12:
        return struct.unpack("<3i", v[2])
    return (0, 0, 0)


def color(v, default=(255, 255, 255, 255)):
    """A Color's (R, G, B, A) (stored B, G, R, A)."""
    if isinstance(v, tuple) and v[0] == "struct" and not isinstance(v[2], dict) and len(v[2]) == 4:
        b, g, r, a = v[2]
        return (r, g, b, a)
    return default


def ref(mp, v):
    """A referenced object's full name, an import's from its package on."""
    o = cd._obj(v)
    if not o:
        return None
    return mp.pkg.full_name(o)


def cue(mp, v):
    """A sound cue as tools/build_sounds.py names it: its whole name."""
    return ref(mp, v)


def last(name):
    return name.split(".")[-1] if name else name


class Deaths:
    def __init__(self, cooked):
        self.r = Reading(cooked)
        self.lines = ["# The death sets (tools/build_death_sets.py): see its header for the fields"]
        self.dsys = {}
        self.done = set()

    def find(self, full):
        for package in PACKAGES:
            mp = self.r.mp(package)
            i = mp.find(full)
            if i:
                return mp, i
        return None, 0

    def run(self):
        for full in SETS:
            mp, i = self.find(full)
            if not i:
                print("  ! death set %s not found" % full)
                continue
            name = last(full)
            for d in cd._objs(mp.props(i).get("Deaths")):
                if d <= 0:
                    continue
                pr = mp.props(d)
                types = [last(mp.name(o)).replace("TnDeathType", "") for o in cd._objs(pr.get("RequiredTypes")) if o]
                entry = mp.full(d)
                self.lines.append("set\t%s\t%s\t%s" % (name, entry, ",".join(types) if types else "-"))
                if mp.pkg.class_name(mp.pkg.exports[d - 1]) == "TnConditionalDeathSet":
                    for e in cd._structs(mp.pkg, pr.get("Deaths")):
                        member = cd._obj(e.get("Death"))
                        if member > 0:
                            weight = int(num(e.get("Weight"), 1.0))
                            self.lines.append("member\t%s\t%s\t%d\t%s" % (name, entry, weight, mp.full(member)))
                            self.death(mp, member)
                else:
                    self.death(mp, d)
        for name, (mp, i) in sorted(self.dsys.items()):
            self.destructible(name, mp, i)

    def death(self, mp, d):
        full = mp.full(d)
        if full in self.done:
            return
        self.done.add(full)
        tl = cd._obj(mp.props(d).get("Timeline"))
        if tl <= 0:
            print("  ! %s has no timeline" % full)
            return
        # (RagdollOnWallCollision: it falls as a ragdoll as it runs into a wall.)
        if mp.props(d).get("RagdollOnWallCollision") is True:
            self.lines.append("death\t%s\twall" % full)
        for k, key in enumerate(cd._structs(mp.pkg, mp.props(tl).get("Keyframes"))):
            for a in cd._objs(key.get("Actions")):
                if a > 0:
                    self.action(mp, full, k, a)

    def action(self, mp, death, key, a):
        cls = mp.pkg.class_name(mp.pkg.exports[a - 1])
        pr = merged(self.r.cdo(cls), parsed(mp, mp.props(a)))
        lo, hi = frange(pr.get("Delay"))
        kind, fields = self.describe(mp, cls, pr)
        # (AfterActionWait: it waits that long more once it is done.)
        after = num(pr.get("AfterActionWait"))
        if after > 0:
            fields.append("hold=%s" % fmt(after))
        self.lines.append("\t".join(["act", death, str(key), fmt(lo), fmt(hi), kind] + fields))

    def describe(self, mp, cls, pr):
        """An action's kind and its fields (k=v)."""
        short = cls.replace("TnDeathAction", "").replace("TnDeathModifier", "")
        if cls == "TnDeathActionPlayAnimation":
            an = pr.get("Animation")
            return "anim", ["name=%s" % sget(an, "AnimName", "None"), "rate=%s" % fmt(num(sget(an, "PlayRate"), 1.0)),
                            "loop=%d" % (1 if sget(an, "Looping") is True else 0),
                            "cutoff=%s" % fmt(num(pr.get("CutoffTime"))),
                            "blend=%s" % fmt(num(sget(an, "BlendInTime"), 0.1))]
        if cls == "TnDeathActionRagdoll":
            before = num(pr.get("VelocityTransferTimeBeforeAnimEnds"), 0.1) if pr.get("UseVelocityTransferTime", True) else -1.0
            mo = frange(pr.get("MomentumImpulseVariance"), (1.0, 1.0))
            di = frange(pr.get("DirectionImpulseVariance"))
            fields = ["settle=%s" % fmt(num(pr.get("MaximumRagdollSettleTime"), 10.0)), "before=%s" % fmt(before),
                      "blend=%s" % fmt(num(pr.get("RagdollBlendTime"), 0.3)),
                      "momentum=%s,%s" % (fmt(mo[0]), fmt(mo[1])), "direction=%s,%s" % (fmt(di[0]), fmt(di[1]))]
            if pr.get("ExplodeRagdoll") is True:
                fields.append("explode=1")
                off = vec(pr.get("ExplodeRagdollOriginOffset"))
                fields.append("explode_offset=%s" % ",".join(fmt(x) for x in off))
            for ra in cd._objs(pr.get("RagdollActions")):
                if ra <= 0:
                    continue
                rcls = mp.pkg.class_name(mp.pkg.exports[ra - 1])
                rp = merged(self.r.cdo(rcls), parsed(mp, mp.props(ra)))
                delay = frange(rp.get("DelayRange")) if rp.get("_UseDelay") is True else (0.0, 0.0)
                if rcls == "TnRagdollActionRadialImpulse":
                    m = frange(rp.get("ImpulseMagnitudeVariance"), (1.0, 1.0))
                    off = vec(rp.get("ExplosionOriginOffset"))
                    origin = impulse_origin(rp.get("ExplosionOrigin", 0))
                    falloff = rp.get("FalloffType", 0)
                    falloff = falloff if isinstance(falloff, int) else (1 if "Linear" in str(falloff) else 0)
                    # (An origin at a bone (2): its BoneName after the numbers.)
                    bone = rp.get("BoneName")
                    bone = "," + bone if origin == 2 and isinstance(bone, str) and bone not in ("", "None") else ""
                    fields.append("radial=%s,%s,%s,%d,%d,%s,%d,%s,%s%s" % (
                        fmt(m[0]), fmt(m[1]), fmt(num(rp.get("FalloffRadius"), 200.0)), falloff, origin,
                        ",".join(fmt(x) for x in off), 0 if rp.get("ConsiderBodyMass", True) else 1,
                        fmt(delay[0]), fmt(delay[1]), bone))
                elif rcls == "TnRagdollActionHaltBodies":
                    fields.append("halt=%s,%s" % (fmt(delay[0]), fmt(delay[1])))
                elif rcls in ("TnRagdollActionSeverPieces", "TnRagdollActionSeverRandomPieces"):
                    # (Those bones' pieces, or NumSeveredPieces at random:
                    # Mutilation.SeverPieces, SeverRandomLimbs; only those within
                    # MinDistanceFromHit of where it was struck, if it says.)
                    bones = [n for n in names(mp, rp.get("SeveredPieces"))]
                    trail = ref(mp, rp.get("SeveredPieceParticleTemplate"))
                    count = int(num(rp.get("NumSeveredPieces"), 0)) if rcls.endswith("RandomPieces") else 0
                    away = fmt(num(rp.get("MinDistanceFromHit"))) if rp.get("UseMinDistanceFromHit") is True else "-"
                    fields.append("sever=%s;%s;%s;%d;%s" % (
                        ",".join(bones) or "-", last(trail) or "-",
                        fmt(num(rp.get("SeveredPieceEffectProbability"), 1.0)), count, away))
                elif rcls == "TnRagdollActionAddImpulse":
                    # (Each turned by the kill's momentum's direction; on the bone it
                    # names (target 0: ETRDIT_SpecificBone, its ConstraintBoneName;
                    # none, the root body) or the bone struck (1: ETRDIT_HitBone); an
                    # impulse or (ConsiderBodyMass off) a velocity; at its centre of
                    # mass, or at a point (UsePointImpulse: its PointSource, as a
                    # radial's origin, and PointOffset in the pawn's frame).)
                    dv = frange(rp.get("DirectionImpulseVariance"))
                    # (ConsiderBodyMass: not set in the class's defaults, so off
                    # unless the action sets it; UsePointImpulse likewise.)
                    mass = 1 if rp.get("ConsiderBodyMass") is True else 0
                    if rp.get("UsePointImpulse") is True:
                        point = "%d,%s" % (impulse_origin(rp.get("PointSource", 0)),
                                           ",".join(fmt(x) for x in vec(rp.get("PointOffset"))))
                    else:
                        point = "-"
                    for e in cd._structs(mp.pkg, rp.get("Impulses")):
                        v = vec(e.get("ImpulseRelativeToMomentum"))
                        a = vec(e.get("AngularVelocityRelativeToMomentum"))
                        target = e.get("Target", 0)
                        target = target if isinstance(target, int) else IMPULSE_TARGETS.get(str(target), 0)
                        bone = e.get("ConstraintBoneName") if target == 0 else None
                        bone = bone if isinstance(bone, str) and bone and bone != "None" else "-"
                        fields.append("impulse=%s;%s;%d;%s;%d;%s,%s;%s" % (
                            ",".join(fmt(x) for x in v), ",".join(fmt(x) for x in a), target,
                            bone, mass, fmt(dv[0]), fmt(dv[1]), point))
                else:
                    fields.append("ragdoll_action=%s" % rcls)
            return "ragdoll", fields
        if cls == "TnDeathActionWait":
            lo, hi = frange(pr.get("WaitTime"))
            return "wait", ["secs=%s,%s" % (fmt(lo), fmt(hi))]
        if cls == "TnDeathActionHighlight":
            der = cd._obj(pr.get("Deresser"))
            dp = merged(self.r.cdo("TnDeresser"), parsed(mp, mp.props(der))) if der > 0 else self.r.cdo("TnDeresser")
            # (Its Deresser's archetype is the action's own template's, Deresser0: 3 s.)
            secs = num(parsed(mp, mp.props(der)).get("Duration"), 3.0) if der > 0 else 3.0
            overlay = ref(mp, dp.get("OverlayMaterial")) if der > 0 and "OverlayMaterial" in mp.props(der) else None
            # (Its Sound plays as it starts: TnDeresser.StartEffect.)
            # (The class's default is in TransGame's names, its archetype's in the
            # death's package's.)
            snd = cue(mp, mp.props(der).get("Sound")) if der > 0 and "Sound" in mp.props(der) else None
            snd = snd or cue(self.r.tg, self.r.cdo("TnDeresser").get("Sound"))
            return "deres", ["secs=%s" % fmt(secs), "overlay=%s" % (last(overlay) or "TR_DefragOverlay_MATINST"),
                             "invisible=%s" % fmt(num(dp.get("NormalizedTimeToMakeMeshInvisible"), 0.39685)),
                             "cutoff=%s" % fmt(num(pr.get("CutoffTime"))),
                             "start=%s" % fmt(num(pr.get("StartingRampValue"))),
                             "ramp_delay=%s" % fmt(num(pr.get("DelayUntilRampStart"))),
                             "sound=%s" % (snd or "-"),
                             "radius=%s" % fmt(num(pr.get("RadiusAdditionalScale"), 1.0)
                                               if pr.get("UseRadiusAdditionalScale") else 1.0),
                             "tiling=%s" % fmt(num(dp.get("MaterialTiling"), 3.0))]
        if cls == "TnDeathActionPlaySound":
            c = cue(mp, pr.get("SoundCue"))
            if c:
                return "sound", ["cue=%s" % c]
            ev = ref(mp, pr.get("SoundEvent"))
            return "sound", ["event=%s" % (last(ev) or "-")]
        if cls == "TnDeathActionPlayEffect":
            out = []
            for e in cd._structs(mp.pkg, pr.get("Effects")):
                t = ref(mp, e.get("Template"))
                if t:
                    out.append("%s@%s" % (last(t), e.get("SocketName", "None")))
            fields = ["systems=%s" % ",".join(out)]
            if pr.get("PlayType") not in (None, 0, "ETnPlayEffectsType_PlayAll"):
                fields.append("random=1")
            return "effect", fields
        if cls == "TnDeathActionCameraShake":
            s = pr.get("CameraShake")
            def v3(k):
                return ",".join(fmt(x) for x in vec(sget(s, k)))
            return "shake", ["radius=%s" % fmt(num(pr.get("CameraShakeRadius"))),
                             "secs=%s" % fmt(num(sget(s, "TimeDuration"), 1.0)),
                             "rot=%s" % v3("RotAmplitude"), "rotf=%s" % v3("RotFrequency"),
                             "loc=%s" % v3("LocAmplitude"), "locf=%s" % v3("LocFrequency"),
                             "fov=%s,%s" % (fmt(num(sget(s, "FOVAmplitude"), 2.0)), fmt(num(sget(s, "FOVFrequency"), 5.0)))]
        if cls == "TnDeathActionSpawnDestructible":
            names_ = []
            for o in cd._objs(pr.get("Destructibles")):
                if o:
                    full = mp.full(o)
                    names_.append(last(full))
                    self.want_dsys(mp, o)
            return "dsys", ["names=%s" % ",".join(names_)]
        if cls == "TnDeathActionTriggerLight":
            lp = pr.get("LightProperties")
            c = color(sget(lp, "LightColor"))
            return "light", ["brightness=%s" % fmt(num(sget(lp, "Brightness"), 25.0)),
                             "radius=%s" % fmt(num(sget(lp, "Radius"), 512.0)),
                             "falloff=%s" % fmt(num(sget(lp, "FalloffExponent"), 10.0)),
                             "color=%d,%d,%d,%d" % c,
                             "ramp_in=%s" % fmt(num(sget(lp, "RampInTime"), 0.5)),
                             "ramp_out=%s" % fmt(num(sget(lp, "RampOutTime"), 0.0)),
                             "secs=%s" % fmt(num(sget(lp, "LightTime"), 10.0)),
                             "sync=%d" % (1 if sget(lp, "SyncPositionWithBounds") is True else 0),
                             "at=%s" % ",".join(fmt(x) for x in vec(sget(lp, "Translation")))]
        if cls == "TnDeathActionSetGravityFactor":
            return "gravity", ["factor=%s" % fmt(num(pr.get("GravityFactor"), 1.0))]
        if cls == "TnDeathActionApplyAngularForce":
            r_, p_, y_ = frange(pr.get("RollForce")), frange(pr.get("PitchForce")), frange(pr.get("YawForce"))
            return "spin", ["roll=%s,%s" % (fmt(r_[0]), fmt(r_[1])), "pitch=%s,%s" % (fmt(p_[0]), fmt(p_[1])),
                            "yaw=%s,%s" % (fmt(y_[0]), fmt(y_[1])),
                            "every=%d" % (1 if pr.get("ApplyContinuousAcceleration") is True else 0)]
        if cls == "TnDeathActionCollisionTimer":
            b, a = frange(pr.get("DurationBeforeCollision")), frange(pr.get("DurationAfterCollision"))
            fields = ["before=%s,%s" % (fmt(b[0]), fmt(b[1])), "after=%s,%s" % (fmt(a[0]), fmt(a[1]))]
            # (A contact counts only with its normal so far along the kill's
            # momentum (UseContactNormalCheck), or on one body (UseBodyCheck).)
            if pr.get("UseContactNormalCheck") is True:
                n = frange(pr.get("ContactNormalCheckRange"))
                fields.append("normal=%s,%s" % (fmt(n[0]), fmt(n[1])))
            if pr.get("UseBodyCheck") is True:
                bc_ = pr.get("BodyCheck")
                fields.append("body=%s,%s" % (sget(bc_, "BoneTarget", 0), sget(bc_, "SpecificBoneName", "None")))
            return "collide", fields
        if cls == "TnDeathActionPlaneMotionControl":
            return "boost", []
        if cls == "TnDeathActionSetMeshHidden":
            return "hide", []
        if cls == "TnDeathModifierControlBone":
            return "bone", ["node=%s" % pr.get("NodeName", "None"), "scale=%s" % fmt(num(pr.get("Scale"), 1.0))]
        return "other", ["class=%s" % short]

    def want_dsys(self, mp, o):
        full = mp.full(o)
        name = last(full)
        if name in self.dsys:
            return
        if o > 0:
            self.dsys[name] = (mp, o)
            return
        # (An import: the map packages that cook the destructible hold it.)
        for package in PACKAGES:
            m = self.r.mp(package)
            i = m.find(full)
            if i:
                self.dsys[name] = (m, i)
                return
        print("  ! destructible %s not found" % full)

    def destructible(self, name, mp, i):
        """A destructible's effects: its pieces' states that hold effect factories."""
        for piece in cd._objs(mp.props(i).get("PieceBlueprints")):
            if piece <= 0:
                continue
            ppr = mp.props(piece)
            ptm = transform(mp, ppr.get("Transform"))
            for st in cd._objs(ppr.get("StateBlueprints")):
                if st <= 0:
                    continue
                for f in cd._objs(mp.props(st).get("EffectFactories")):
                    if f > 0:
                        self.factory(name, mp, f, ptm)

    def factory(self, name, mp, f, ptm):
        cls = mp.pkg.class_name(mp.pkg.exports[f - 1])
        pr = parsed(mp, mp.props(f))
        at, turn = transform(mp, pr.get("Transform"))
        at = tuple(a + b for a, b in zip(at, ptm[0]))
        where = " ".join(fmt(x) for x in at)
        if cls == "HmParticleDestructionEffectFactory":
            t = ref(mp, pr.get("Template"))
            if t:
                self.lines.append("dsys\t%s\tparticle\t%s\t%s" % (name, last(t), where))
        elif cls in ("HmGibsDestructionEffectFactory", "HmDebrisDestructionEffectFactory"):
            meshes = [last(ref(mp, ("obj", o))) for o in [cd._obj(pr.get("MeshTemplate"))]
                      + cd._objs(pr.get("MeshTemplates")) if o]
            vol = vec(pr.get("SpawnVolumeScale"))
            lin, ang = frange(pr.get("LinearSpeed")), frange(pr.get("AngularSpeed"))
            der = pr.get("DeresInfo")
            aud = pr.get("AudioInfo")
            self.lines.append("\t".join([
                "dsys", name, "gibs", ",".join(meshes), str(int(num(pr.get("NumChunks"), 1))),
                ",".join(fmt(x) for x in vol), "%s,%s" % (fmt(lin[0]), fmt(lin[1])),
                "%s,%s" % (fmt(ang[0]), fmt(ang[1])), fmt(num(pr.get("VelocityConeAngle"), 360.0)),
                fmt(num(pr.get("ChunkLifespan"), 10.0)), last(ref(mp, pr.get("Particles"))) or "-", where,
                " ".join(str(x) for x in turn), fmt(num(sget(der, "Duration"))),
                cue(mp, sget(aud, "ActiveSoundCue")) or "-", cue(mp, sget(der, "DeresSound")) or "-",
                fmt(num(pr.get("TransferVelocityMultiplier"), 1.0))]))
        elif cls == "HmSoundDestructionEffectFactory":
            c = cue(mp, pr.get("Cue"))
            if c:
                self.lines.append("dsys\t%s\tsound\t%s" % (name, c))
        elif cls == "TnDecalDestructionEffectFactory":
            t = pr.get("Template")
            m = ref(mp, sget(t, "Material"))
            self.lines.append("\t".join([
                "dsys", name, "decal", last(m) or "-", fmt(num(sget(t, "MinWidth"))), fmt(num(sget(t, "MinHeight"))),
                fmt(num(sget(t, "WidthRange"))), fmt(num(sget(t, "HeightRange"))), fmt(num(sget(t, "Thickness"))),
                fmt(num(sget(t, "Lifetime"))), where]))
        elif cls == "HmRadialDamageDestructionEffectFactory":
            self.lines.append("\t".join([
                "dsys", name, "damage", last(ref(mp, pr.get("DamageType"))) or "-", fmt(num(pr.get("Damage"))),
                fmt(num(pr.get("DamageRadius"))), fmt(num(pr.get("DamageMomentum"))), where]))
        else:
            self.lines.append("dsys\t%s\tother\t%s" % (name, cls))


def names(mp, v):
    """An array of names."""
    if not (isinstance(v, tuple) and v[0] == "array") or len(v[1]) < 4:
        return []
    r = Reader(v[1])
    n = r.i32()
    out = []
    for _ in range(n):
        out.append(mp.pkg.fname(r))
    return out


def transform(mp, v):
    """An AffineTransform's Position and Orientation."""
    if isinstance(v, tuple) and v[0] == "struct" and isinstance(v[2], dict):
        return vec(v[2].get("Position")), rot(v[2].get("Orientation"))
    if isinstance(v, tuple) and v[0] == "struct":
        try:
            pr = read_props(mp.pkg, Reader(v[2]), len(v[2]))
            return vec(pr.get("Position")), rot(pr.get("Orientation"))
        except Exception:
            pass
    return (0.0, 0.0, 0.0), (0, 0, 0)


def main(cooked, assets):
    d = Deaths(cooked)
    d.run()
    path = os.path.join(assets, OUT)
    os.makedirs(os.path.dirname(path), exist_ok=True)
    with open(path, "w", encoding="utf-8") as f:
        f.write("\n".join(d.lines) + "\n")
    print("wrote %s: %d lines, %d deaths, %d destructibles" % (path, len(d.lines), len(d.done), len(d.dsys)))


if __name__ == "__main__":
    if len(sys.argv) != 3:
        sys.exit(__doc__)
    main(*sys.argv[1:])
