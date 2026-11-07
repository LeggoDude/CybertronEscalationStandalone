"""Extract Jazz's and his weapons' sound effects from Fall of Cybertron packages.

usage: python build_sounds.py <CookedPC folder> <foc_esc/assets folder>

(Build the classes' characters first, tools/build_class.py: their robots',
vehicles' and transformations' cues are the ones their <class>.txt files name.)

Reads the SoundCues listed in CUES from the game's packages, decodes every wave they
use (FMOD FSB4 banks holding MPEG audio) to Ogg Vorbis with ffmpeg, and writes
assets/sounds/sounds.txt describing each cue the way the game plays it:

  cue <name> <volume dB> <pitch semitones> <pitch var min> <pitch var max>
      <volume var min> <volume var max> <max concurrent> <distance min m> <distance max m>
      <rolloff factor> <oldest|newest|farthest>
      (heard at full volume out to its near distance, then fading as the game fades
      it, by the inverse of the distance its rolloff factor scales: near / (near +
      rolloff * (distance - near)); its SoundNodeRoot's DistanceMin, DistanceMax and
      RolloffFactor, 1 by default, 0 for none; and which copy goes when more would
      play than its max concurrent: its InstanceLimiting, the farthest by default.
      The engine's defaults stand for what a package leaves out: 5 at once, -6 dB,
      4 m and 64 m)
  category <name>
      (the cue's mixer category, its SoundNodeRoot's Category: DX_DRY for dialogue,
      MUSIC_DRY for music, SFX_... for everything else; it decides which of the
      Audio settings' volumes the cue follows)
  mixer <preset> [local]
      (the mixer preset the cue turns on as it plays, its SoundNodeRoot's
      SoundMixerPreset; local: only the local player's own copy of it does,
      PlayMixerPresetOnlyIfLocalPlayerSound)
  layer <time s> <volume dB> <pitch> <pitch var min> <pitch var max>
      <volume var min> <volume var max> <looping 0/1> <wave> [<wave> ...]

Before the cues, the game's mixer (HM_Engine.xxx's SND_SoundConfig_p.
SoundMixerProperties and its HmSoundMixerPresets):

  mixcategory <name> <parent or ->
      (a mixer category, parents before their children: a category's volume
      scales its children's, MASTER_WET's every SFX_WET_ and DX_WET_ one's)
  preset <name> <fade in s> <fade out s> <duration s> <category> <volume dB> [...]
      (a preset the cues turn on: while it is on, each category it names is turned
      down (or up) by its volume, fading in over its fade-in time as it comes on and
      back over its fade-out time as it goes off: <duration> seconds after it came
      on, or, for 0 or -1, once the sounds that turned it on have stopped. Only its
      volumes: its categories' filters, reverbs and pitch are left out)
  channels <category> <count> <seconds> <preset>
      (a preset a category turns on while at least <count> of its sounds play: its
      ChannelCountSoundMixerPreset, ChannelCountMixerPresetThreshold and Time)

A layer plays one of its waves at random (the game's SoundNodeWaveEvent children).
Optional lines describe cues with a looping layer (music, and the looping sounds a
weapon holds on, such as the Nucleon Charge Rifle's scope), and the envelope lines
the one-off cues too, those whose layers' volume curves shape them (and FADED_CUES
whatever theirs are):

  envelope <layer index> <time s> <volume 0-1> [<time s> <volume 0-1> ...]
      (a layer's volume over the cue's time, e.g. a fade-in: its VolumeCurve)
  pitch <layer index> <time s> <semitones> [<time s> <semitones> ...]
      (a layer's pitch over the cue's time: its PitchCurve)
  random <layer index> <chance of none, %> <delay min s> <delay max s>
      (a layer that sometimes stays silent and starts a little late: its
      ChanceToPlayNone, DelayMin and DelayMax)
  loop_start <time s>   (where the cue goes back to at loop_end, if not its start)
  loop_end <time s>   (the whole cue starts over at this time)
  distance <layer index> <distance m> <volume 0-1> [<distance m> <volume 0-1> ...]
      (a layer's volume by how far from the camera the cue plays: its VolumeCurve,
      where its SoundNodeRoot's SoundParameter is SOUND_DISTANCE. A gun's shot is
      its close-up recording near the camera and its distant one past it: the
      enemy soldiers' rifle, the Photon Eliminator's SHOOT, plays its close-up
      shot out to 5 m, faded out by 7.5 m, and its MID_DIST_02 shot from 4.5 m,
      full from 8 m to 60 m)
  pan <layer index> <front left dB> <front right dB> <centre dB> <back left dB>
      <back right dB> <2d|smart>
      (a layer's speaker levels where they aren't a plain sound's (the fronts at
      0 dB, the rest silent: its SoundNodeWaveEvent's PanFrontLeft, PanFrontRight,
      PanCenter, PanBackLeft, PanBackRight), as the game plays it when it is
      heard flat, not placed round the player: 2d for a layer always so (its
      SpatializationType k2D: its root's, or its own where it overrides it), smart
      for one so when it is the player's own (kSmartPan, kSmartPan_PreferPlayer:
      a car's or a tank's boost exhaust, in none of the speakers, so not heard by
      the player in it, though by the others round it). Left out for a layer
      placed round the player (k3D), whose levels the game doesn't use)

These follow the cue's layers.

Music is streamed from separate packages (WL_MX_*.xxx); the cues that use it
refer to those waves by name and they are looked up there. So are the robot voices'
spoken lines (WL_DX_MPAUT1 to 3 and WL_DX_MPDEC1 to 3), whose sets also go into
assets/sounds/voices.txt (see VOICE_SETS).

The enemies' cues are read too (build the enemies' AI first: tools/build_ai.py's
models/enemies/ai_sounds.txt names their sound sets' cues, tools/build_ai_anims.py's
ai_anims_<set>.txt their animations', ai.txt their weapons' and projectiles', and
AI_CUES the ones their code plays; build their deaths first too, tools/build_death_sets.py
and tools/build_deaths.py, for their deaths' cues); a dialog cue as each of its speakers' lines
(ENEMY_SPEAKER), whose lengths go into voices.txt as the voices' do. Their lines
are streamed from each speaker's own package (WL_DX_SLDD01.xxx and so on).
Needs ffmpeg with libvorbis on the PATH.
"""
import math, os, struct, subprocess, sys, tempfile

sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), "ue3"))
from ue3pkg import Package, Reader  # noqa: E402
from layout import props_any  # noqa: E402
from props import read_props  # noqa: E402

# Packages searched in order for each cue (the first that has it wins).
PACKAGES = ["L03b_CutAndRun_BASE_m.xxx", "mp_shared.xxx", "ESC_Gladiator_BASE_m.xxx",
            # (Ancients' enemies' own: the Swarmer's and the Sniper's.)
            "ESC_Temple_BASE_m.xxx",
            # (Downfall's and Oblivion's doors' own, and Oblivion's combustion
            # chamber's blast, its AUDIO package's.)
            "ESC_Iacon_BASE_m.xxx", "ESC_Ark_BASE_m.xxx", "ESC_Ark_AUDIO_m.xxx",
            # The front end: its menu sounds (cues in BL_HUD_INTERFACE, recordings
            # in Startup_int) and music (streamed from the WL_MX packages).
            "BL_HUD_INTERFACE.xxx", "Startup_int.xxx", "UI_FrontEnd_m.xxx",
            "WL_MX_GLOBAL.xxx", "WL_MX_L02.xxx",
            # The rest of the campaign's music, for the stingers a match starts with.
            "WL_MX_L01.xxx", "WL_MX_L03.xxx", "WL_MX_L04.xxx", "WL_MX_L05.xxx", "WL_MX_L06.xxx",
            # The announcers' recorded lines (English) for the Escalation maps, and
            # the enemies' dialog lines (each map's own: its enemies').
            "ESC_Gladiator_BASE_m_LOC_int.xxx", "ESC_Temple_BASE_m_LOC_int.xxx",
            "ESC_Iacon_BASE_m_LOC_int.xxx", "ESC_Ark_BASE_m_LOC_int.xxx",
            # The customization garage: its ambience cues and their waves, the
            # bed's streamed from WL_AMB_BED.
            "UI_CustomCharacter_m.xxx", "WL_AMB_BED.xxx",
            # Everything shared by the game's modes, among it the Photon Burst Rifle,
            # the Scatter Blaster and the weapon pick-up sounds.
            "all_shared.xxx",
            # A multiplayer map's own: the EMP grenade's sounds are cooked into it,
            # and the robots' voices (their cues; the recorded lines, English, in
            # its _LOC_int).
            "MP_Array_BASE_m.xxx", "MP_Array_BASE_m_LOC_int.xxx",
            # Oblivion's art package, for its exploding barrel's own (last: the
            # rest are found where they were).
            "ESC_Ark_ART_m.xxx"]
# The robot voices' spoken lines are streamed from packages of their own, one per
# voice (the grunts are in MP_Array_BASE_m_LOC_int.xxx): those found are read too.
PACKAGES += ["WL_DX_MP%s%d%s.xxx" % (side, n, suffix) for side in ("AUT", "DEC") for n in (1, 2, 3)
             for suffix in ("", "_int", "_INT")]

# The multiplayer's robot voices (the Customize screen's Select Voice): each
# TnDataProvider_RobotVoice of TransCustomization.ini names an HmSoundEventSet for
# the Autobots and one for the Decepticons, SoundEvents.Multiplayer.CHR_MP_DX_*
# (the classes share them: Voice 1, 2 and 3 are the sets 01, 02 and 03), cooked into
# MP_Array_BASE_m.xxx. Each set gives the dialog events a voice plays (reloading,
# a kill, a pick-up, being hit...) the cue it plays them with. Their cues and these
# go into assets/sounds/voices.txt:
#
#   voice <set> <event> <cue>
#   length <cue> <its longest line, s>
#   event <event> <dialog group or -> <priority>
#   group <group> <parent group or -> <initial chance to play, %> <retrigger delay
#         min s> <retrigger delay max s> <instance limit>
#
# (HmSoundEvent's DialogGroup and DialogPriority; HmSoundEventDialogGroup's
# ParentDialogGroup, InitialChanceToPlay, ReTriggerDelayMin/Max and InstanceLimit,
# their defaults 100% and 1 where a group leaves them out.) The events and groups are
# read from these packages too (the game's scripts hold most of them).
VOICE_SETS = ["CHR_MP_DX_AUTOBOT_01", "CHR_MP_DX_AUTOBOT_02", "CHR_MP_DX_AUTOBOT_03",
              "CHR_MP_DX_DECEPTICON_01", "CHR_MP_DX_DECEPTICON_02", "CHR_MP_DX_DECEPTICON_03"]
VOICE_PACKAGES = ["TransGame.xxx", "all_shared.xxx", "UI_Lobby_m.xxx"]

CUES = [
    # Neutron Assault Rifle (TnWeaponEffectSound of WEP_AssaultRifleArm). Its StartFire
    # cue (GUNARM_SHOOT_START) is muted in the game (-96 dB) and the Subsonic
    # Repeater's is empty, so a burst is each shot plus the end/tail cue on release.
    "BL_WPN_GUN_NEUTRON_RIFLE.GUNARM_SHOOT",
    "BL_WPN_GUN_NEUTRON_RIFLE.GUNARM_SHOOT_END",
    "BL_WPN_GUN_NEUTRON_RIFLE.ANIM_EQUIP",
    "BL_WPN_GUN_NEUTRON_RIFLE.ANIM_HOLSTER",
    "BL_WPN_GUN_NEUTRON_RIFLE.ANIM_CHANGE_HANDS",
    "BL_WPN_GUN_NEUTRON_RIFLE.VEH_SHOOT",
    "BL_WPN_GUN_NEUTRON_RIFLE.FINE_AIM_START",
    "BL_WPN_GUN_NEUTRON_RIFLE.FINE_AIM_END",
    # Reloads, and the shot the game swaps in when the clip runs low
    # (TnWeaponEffectLowAmmoSound: 9 rounds or fewer left).
    "BL_WPN_GUN_NEUTRON_RIFLE.ANIM_RELOAD",
    "BL_WPN_GUN_NEUTRON_RIFLE.GUNARM_SHOOT_LOWAMMO",
    "BL_WPN_GUN_NEUTRON_RIFLE.VEH_ANIM_RELOAD",
    "BL_WPN_GUN_NEUTRON_RIFLE.VEH_SHOOT_LOW_AMMO",
    # Subsonic Repeater (the game's "IonBlaster" inside)
    "BL_WPN_GUN_ION_BLASTER.SHOOT",
    "BL_WPN_GUN_ION_BLASTER.SHOOT_TAIL",
    "BL_WPN_GUN_ION_BLASTER.ANIM_EQUIP",
    "BL_WPN_GUN_ION_BLASTER.ANIM_HOLSTER",
    "BL_WPN_GUN_ION_BLASTER.ANIM_CHANGE_HANDS",
    "BL_WPN_GUN_ION_BLASTER.FINE_AIM_START",
    "BL_WPN_GUN_ION_BLASTER.FINE_AIM_END",
    "BL_WPN_GUN_ION_BLASTER.ANIM_RELOAD_01",
    "BL_WPN_GUN_ION_BLASTER.SHOOT_LOW_AMMO",
    # Photon Burst Rifle (TnWeaponEffectSound of WEP_BurstRifle_p.BurstRifle_WEPMESH:
    # it borrows the Photon Eliminator's sounds). SHOOT plays once per burst
    # (StartFire), SHOOT_TAIL when it ends (StopFire); ANIM_EQUIP is used for both
    # drawing and putting it away.
    "BL_WPN_GUN_PHOTON_ELIMINATOR.SHOOT",
    "BL_WPN_GUN_PHOTON_ELIMINATOR.SHOOT_TAIL",
    "BL_WPN_GUN_PHOTON_ELIMINATOR.ANIM_EQUIP",
    "BL_WPN_GUN_PHOTON_ELIMINATOR.ANIM_CHANGE_HANDEDNESS",
    "BL_WPN_GUN_PHOTON_ELIMINATOR.ANIM_RELOAD",
    "BL_WPN_GUN_PHOTON_ELIMINATOR.FINE_AIM_START",
    "BL_WPN_GUN_PHOTON_ELIMINATOR.FINE_AIM_END",
    # Scatter Blaster (TnWeaponEffectSound of WEP_Shotgun_p.Shotgun_WEPMESH): a shot
    # (the low-ammo one with 2 rounds or fewer left), its tail when the trigger is
    # let go, drawing, putting away, reloading and switching hands; fine aim borrows
    # the shotgun's, and an empty gun clicks with the plasma dry-fire.
    "BL_WPN_GUN_SCATTER_BLASTER.AUTO_SHOTGUN_SHOOT",
    "BL_WPN_GUN_SCATTER_BLASTER.AUTO_SHOTGUN_SHOOT_LOW_AMMO",
    "BL_WPN_GUN_SCATTER_BLASTER.AUTO_SHOTGUN_SHOOT_TAIL",
    "BL_WPN_GUN_SCATTER_BLASTER.ANIM_EQUIP",
    "BL_WPN_GUN_SCATTER_BLASTER.ANIM_HOLSTER",
    "BL_WPN_GUN_SCATTER_BLASTER.ANIM_RELOAD",
    "BL_WPN_GUN_SCATTER_BLASTER.ANIM_CHANGE_HANDS",
    "BL_WPN_GUN_SHOTGUN.FINE_AIM_START",
    "BL_WPN_GUN_SHOTGUN.FINE_AIM_END",
    "BL_WPN_FOLEY.SHOOT_DRY_FIRE_PLASMA_02",
    # The Subsonic Repeater's empty click (its NoAmmoFire).
    "BL_WPN_FOLEY.SHOOT_DRY_FIRE_ELECTRICITY",
    # Picking a weapon up off the ground (TnWeaponMesh PickupSound), and a gun's
    # empty click (NoAmmoFire).
    "BL_WPN_FOLEY.PICK_UP",
    "BL_WPN_FOLEY.SHOOT_DRY_FIRE_BALLISTIC",
    # Bullet impacts and fly-bys
    "BL_WPN_GUN__IMPT_BYS.IMPT_BALLISTIC_WORLD",
    "BL_WPN_GUN__IMPT_BYS.IMPT_ENERGY_WORLD",
    "BL_WPN_GUN__IMPT_BYS.IMPT_BALLISTIC_DMG_AI",
    "BL_WPN_GUN__IMPT_BYS.IMPT_ENERGY_DMG_AI",
    "BL_WPN_GUN__IMPT_BYS.IMPT_ENERGY_DMG",
    # The Scatter Blaster's pellets striking walls and sentries.
    "BL_WPN_GUN__IMPT_BYS.IMPT_SHOTGUN_WORLD",
    "BL_WPN_GUN__IMPT_BYS.IMPT_SHOTGUN_DMG_AI",
    # Nucleon Charge Rifle (TnWeaponEffectSound of WEP_SniperRifle_p.SniperRifle_WEPMESH,
    # the game's "Null Ray"): a shot (the low-ammo one with 2 rounds or fewer left),
    # an empty click, reloading, drawing, putting away and switching hands. Its scope
    # holds three looping cues: ZOOM_LP from fine aim's start to its end (the zoom
    # whine, then a hum and static), ZOOM_CHARGE_LP while it charges (rising in
    # pitch) and ZOOM_CHARGE_COMPLETE_LP once it is fully charged (a power-up and a
    # second and a half of beeps); ZOOM_OUT plays as fine aim ends. Its rounds strike
    # with the sniper impacts. (Its SHOOT_CHARGED belongs to fire mode 9, which the
    # rifle never fires: every shot uses fire mode 0's effects.)
    "BL_WPN_GUN_NULLRAY.SHOOT",
    "BL_WPN_GUN_NULLRAY.SHOOT_LOW_AMMO",
    "BL_WPN_GUN_NULLRAY.DRY_FIRE",
    "BL_WPN_GUN_NULLRAY.ANIM_RELOAD",
    "BL_WPN_GUN_NULLRAY.ANIM_EQUIP",
    "BL_WPN_GUN_NULLRAY.ANIM_HOLSTER",
    "BL_WPN_GUN_NULLRAY.ANIM_CHANGE_HANDS",
    "BL_WPN_GUN_NULLRAY.ZOOM_LP",
    "BL_WPN_GUN_NULLRAY.ZOOM_OUT",
    "BL_WPN_GUN_NULLRAY.ZOOM_CHARGE_LP",
    "BL_WPN_GUN_NULLRAY.ZOOM_CHARGE_COMPLETE_LP",
    "BL_WPN_GUN__IMPT_BYS.IMPT_SNIPER_WORLD",
    "BL_WPN_GUN__IMPT_BYS.IMPT_SNIPER_DMG_AI",
    # Path Blaster (TnWeaponEffectSound of WEP_HeavyPistol_p.HeavyPistol_WEPMESH, the
    # game's "Battle Pistol"): a shot (the low-ammo one with 2 rounds or fewer left),
    # raising the sights as fine aim starts, reloading, drawing, putting away and
    # switching hands. Its empty click is the ballistic dry fire and its rounds
    # strike with the ballistic impacts (both above). (SHOOT_UPGRADE and
    # EXPL_UPGRADE belong to its upgraded shockwave rounds, fire modes 1 and 2.)
    "BL_WPN_GUN_BATTLE_PISTOL.SHOOT",
    "BL_WPN_GUN_BATTLE_PISTOL.SHOOT_LOW_AMMO",
    "BL_WPN_GUN_BATTLE_PISTOL.ZOOM_IN",
    "BL_WPN_GUN_BATTLE_PISTOL.ANIM_RELOAD",
    "BL_WPN_GUN_BATTLE_PISTOL.ANIM_EQUIP",
    "BL_WPN_GUN_BATTLE_PISTOL.ANIM_HOLSTER",
    "BL_WPN_GUN_BATTLE_PISTOL.ANIM_CHANGE_HANDEDNESS",
    # Its Lights Out upgrade's special shot (fire mode 2: src/specials.rs).
    "BL_WPN_GUN_BATTLE_PISTOL.SHOOT_UPGRADE",
    "BL_WPN_GUN_BATTLE_PISTOL.EXPL_UPGRADE",
    # Riot Cannon (TnWeaponEffectSound of WEP_Bazooka_p.Bazooka_WEPMESH, the game's
    # "Fusion Cannon" inside): a shot (its low-ammo shot is the same cue), raising
    # and lowering the sights, reloading, drawing, putting away and switching hands
    # (its empty click is BL_WPN_FOLEY's plasma dry fire, above); and its round
    # (Bazooka_PROJMESH): the loop it flies with (FlightSound) and going off
    # (ExplosionSound). (The _5x_DMG ones belong to its upgrade's special round.)
    "BL_WPN_GUN_FUSION_CANNON.SHOOT",
    "BL_WPN_GUN_FUSION_CANNON.FINE_AIM_START",
    "BL_WPN_GUN_FUSION_CANNON.FINE_AIM_END",
    "BL_WPN_GUN_FUSION_CANNON.ANIM_RELOAD",
    "BL_WPN_GUN_FUSION_CANNON.ANIM_EQUIP",
    "BL_WPN_GUN_FUSION_CANNON.ANIM_HOLSTER",
    "BL_WPN_GUN_FUSION_CANNON.ANIM_CHANGE_HANDEDNESS",
    "BL_WPN_GUN_FUSION_CANNON.TRAIL_LP",
    "BL_WPN_GUN_FUSION_CANNON.EXPL_IMPT_WORLD",
    # The Riot Cannon's Planet Buster round (BazookaSpecial_SP_PROJMESH: src/riot.rs).
    "BL_WPN_GUN_FUSION_CANNON.TRAIL_LP_5X_DMG",
    "BL_WPN_GUN_FUSION_CANNON.EXPL_IMPT_WORLD_5x_DMG",
    # Orbital Pistol (TnWeaponEffectSound of WEP_TracerDart_p.TracerDart_WEPMESH,
    # cooked into the multiplayer maps): a shot (its low-ammo shot is the same cue),
    # raising and lowering the sights, reloading, drawing, putting away and switching
    # hands. Its empty click is the ballistic dry fire and its shots strike with the
    # energy impacts (both above).
    "BL_WPN_GUN_TRACER_DART.SHOOT",
    "BL_WPN_GUN_TRACER_DART.FINE_AIM_START",
    "BL_WPN_GUN_TRACER_DART.FINE_AIM_END",
    "BL_WPN_GUN_TRACER_DART.ANIM_RELOAD_01",
    "BL_WPN_GUN_TRACER_DART.ANIM_EQUIP",
    "BL_WPN_GUN_TRACER_DART.ANIM_HOLSTER",
    "BL_WPN_GUN_TRACER_DART.ANIM_CHANGE_HANDS",
    # The X18 Scrapmaker (WEP_HeavyMG_p.HeavyMG_WEPMESH, on the Photon Eliminator's
    # "PULSE_RIFLE" cues): the barrels spinning up (SHOOT_WINDUP, StartCharging to
    # StartFire), a shot (SHOOT, or SHOOT_LOW_AMMO with 10 rounds or fewer left) over
    # the loop held while it fires (SHOOT_LP, StartFire to StopFire), its tail as it
    # stops (SHOOT_TAIL), sights, reload, drawing, putting away and switching hands.
    "BL_WPN_GUN_PULSE_RIFLE.SHOOT_WINDUP",
    "BL_WPN_GUN_PULSE_RIFLE.SHOOT",
    "BL_WPN_GUN_PULSE_RIFLE.SHOOT_LOW_AMMO",
    "BL_WPN_GUN_PULSE_RIFLE.SHOOT_LP",
    "BL_WPN_GUN_PULSE_RIFLE.SHOOT_TAIL",
    "BL_WPN_GUN_PULSE_RIFLE.FINE_AIM_START",
    "BL_WPN_GUN_PULSE_RIFLE.FINE_AIM_END",
    "BL_WPN_GUN_PULSE_RIFLE.ANIM_RELOAD",
    "BL_WPN_GUN_PULSE_RIFLE.ANIM_EQUIP",
    "BL_WPN_GUN_PULSE_RIFLE.ANIM_HOLSTER",
    "BL_WPN_GUN_PULSE_RIFLE.ANIM_CHANGE_HANDEDNESS",
    # The Energon Harvester (WEP_DrainGun_p.DrainGun_WEPMESH, on the shotgun's cues):
    # each shot (SHOOT, its last round SHOOT_LOW_AMMO), sights, reload, drawing,
    # putting away and switching hands; its pellets strike with the shotgun impacts
    # (above).
    "BL_WPN_GUN_SHOTGUN.SHOOT",
    "BL_WPN_GUN_SHOTGUN.SHOOT_LOW_AMMO",
    "BL_WPN_GUN_SHOTGUN.ANIM_RELOAD",
    "BL_WPN_GUN_SHOTGUN.ANIM_EQUIP",
    "BL_WPN_GUN_SHOTGUN.ANIM_HOLSTER",
    "BL_WPN_GUN_SHOTGUN.ANIM_CHANGE_HANDS",
    # The E.D.K. TechVolt (WEP_ChainBeam_p.ChainBeam_WEPMESH): the beam's loop
    # (SHOOT_LP, StartFire to StopFire), its last crackle as it stops
    # (SHOOT_LAST_SHOT), its hitting an enemy (HIT_ENEMY), its impact on the world
    # (the repair beam's IMPT_WORLD), sights (the repair beam's), reload, drawing,
    # putting away and switching hands.
    "BL_WPN_GUN_TECH_VOLT.SHOOT_LP",
    "BL_WPN_GUN_TECH_VOLT.SHOOT_LAST_SHOT",
    "BL_WPN_GUN_TECH_VOLT.HIT_ENEMY",
    "BL_WPN_GUN_REPAIR_WPN.IMPT_WORLD",
    "BL_WPN_GUN_REPAIR_WPN.FINE_AIM_START",
    "BL_WPN_GUN_REPAIR_WPN.FINE_AIM_END",
    "BL_WPN_GUN_TECH_VOLT.ANIM_RELOAD",
    "BL_WPN_GUN_TECH_VOLT.ANIM_EQUIP",
    "BL_WPN_GUN_TECH_VOLT.ANIM_HOLSTER",
    "BL_WPN_GUN_TECH_VOLT.ANIM_CHANGE_HANDEDNESS",
    # The Corrosive Slime Cannon (WEP_AoeSlow_p.AoeSlow_WEPMESH): a shot, reload,
    # drawing, putting away and switching hands; and its round (AoeSlow_PROJMESH):
    # the loop it flies with and going off.
    "BL_WPN_GUN_SLIME_CANNON.SHOOT",
    "BL_WPN_GUN_SLIME_CANNON.ANIM_RELOAD",
    "BL_WPN_GUN_SLIME_CANNON.ANIM_EQUIP",
    "BL_WPN_GUN_SLIME_CANNON.ANIM_HOLSTER",
    "BL_WPN_GUN_SLIME_CANNON.ANIM_CHANGE_HANDEDNESS",
    "BL_WPN_GUN_SLIME_CANNON.PROJ_FLIGHT_LP",
    "BL_WPN_GUN_SLIME_CANNON.PROJ_IMPT_WORLD",
    # The Gear Shredder (WEP_Sawblade_p.SawBlade_WEPMESH): a shot of one, two or three
    # blades (SoundPerFireMode), charging (CHARGE_LP, StartCharging to StopCharging)
    # with a clunk as each blade loads (CHARGE_MECH_01 / _02), reload, and its blades
    # (SawBlade_PROJMESH): the loop they fly with, bouncing and striking.
    "BL_WPN_GUN_SAW_BLADE.SHOOT",
    "BL_WPN_GUN_SAW_BLADE.SHOOT_CHRG_02_SAWS",
    "BL_WPN_GUN_SAW_BLADE.SHOOT_CHRG_03_SAWS",
    "BL_WPN_GUN_SAW_BLADE.CHARGE_LP",
    "BL_WPN_GUN_SAW_BLADE.CHARGE_MECH_01",
    "BL_WPN_GUN_SAW_BLADE.CHARGE_MECH_02",
    "BL_WPN_GUN_SAW_BLADE.ANIM_RELOAD",
    "BL_WPN_GUN_SAW_BLADE.MELEE_ATTACK",
    "BL_WPN_GUN_SAW_BLADE.PROJECTILE_LP",
    "BL_WPN_GUN_SAW_BLADE.PROJECTILE_BOUNCE",
    "BL_WPN_GUN_SAW_BLADE.PROJECTILE_IMPT",
    # The Thermo Rocket Cannon (WEP_RocketLauncher_p.RocketLauncher_WEPMESH): a shot,
    # sights, reload, drawing, putting away and switching hands (its rockets fly with
    # the rockets' loop and go off with their blast, above).
    "BL_WPN_GUN_ROCKET.SHOOT",
    "BL_WPN_GUN_ROCKET.FINE_AIM_START",
    "BL_WPN_GUN_ROCKET.ANIM_RELOAD",
    "BL_WPN_GUN_ROCKET.ANIM_EQUIP",
    "BL_WPN_GUN_ROCKET.ANIM_HOLSTER",
    "BL_WPN_GUN_ROCKET.ANIM_CHANGE_HANDEDNESS",
    # The A-4 Pulsar Cannon (WEP_GrenadeLauncher_p.GrenadeLauncher_WEPMESH, on the
    # plasma cannon's and the grenade launcher's cues): a shot, setting it off by hand
    # (and the click with nothing out to set off), sights, reload, drawing, putting
    # away and switching hands; and its round (GrenadeLauncher_PROJMESH): the loop it
    # flies with, sticking where it lands, bouncing and going off.
    "BL_WPN_GUN_PLASMA_CANNON.PULSAR_CANNON_SHOOT",
    "BL_WPN_GUN_GRENADE_LAUNCHER.SHOOT_LOW_AMMO",
    "BL_WPN_GUN_PLASMA_CANNON.PULSAR_CANNON_REMOTE_DETONATE",
    "BL_WPN_GUN_GRENADE_LAUNCHER.DETONATE_REMOTE_DRYFIRE",
    "BL_WPN_GUN_GRENADE_LAUNCHER.FINE_AIM_START",
    "BL_WPN_GUN_GRENADE_LAUNCHER.FINE_AIM_END",
    "BL_WPN_GUN_PLASMA_CANNON.ANIM_RELOAD",
    "BL_WPN_GUN_PLASMA_CANNON.ANIM_EQUIP",
    "BL_WPN_GUN_PLASMA_CANNON.ANIM_HOLSTER",
    "BL_WPN_GUN_PLASMA_CANNON.CHANGE_HANDEDNESS",
    "BL_WPN_GUN_PLASMA_CANNON.PULSAR_CANNON_SHOOT_TAIL_LP",
    "BL_WPN_GUN_PLASMA_CANNON.PULSAR_CANNON_PROJ_IMPT_STICK",
    "BL_WPN_GUN_PLASMA_CANNON.PULSAR_CANNON_PROJ_EXPL",
    "BL_WPN_GRENADE.GRENADE_FOLEY_SHELL_BOUNCE_HEAVY",
    # The Chaos-Rift Combustor (WEP_ClusterCannon_p.ClusterCannon_WEPMESH, on the
    # grenade launcher's and the rifle's cues): its bomb (ClusterCannon_PROJMESH)
    # flies and goes off with the tank shell's loop and blast (above), its clusters
    # (Clusters_PROJMESH) with their own loop, fuse and blast; setting the bomb off
    # (CLUSTER_BOMB_ENGAGE, its DefuseSound).
    "BL_WPN_GUN_GRENADE_LAUNCHER.ANIM_RELOAD",
    "BL_WPN_GUN_GRENADE_LAUNCHER.ANIM_EQUIP",
    "BL_WPN_GUN_GRENADE_LAUNCHER.ANIM_HOLSTER",
    "BL_WPN_GUN_GRENADE_LAUNCHER.ANIM_CHANGE_HANDS",
    "BL_WPN_GUN_TANK_CANNON.CLUSTER_BOMB_ENGAGE",
    "BL_WPN_GUN_TANK_CANNON.CLUSTER_BOMB_PROJ_LP",
    "BL_WPN_GUN_TANK_CANNON.CLUSTER_BOMB_FUSE",
    "BL_WPN_GUN_TANK_CANNON.CLUSTER_BOMB_PROJ_EXPL",
    # A tank's cannon (the Titan's vehicles' TankCannon: TnWeaponEffectSound and
    # TnWeaponEffectLowAmmoSound of WEP_TankCannon_p.TankCannon_Vehicle_WEPMESH): its
    # shot (always the low-ammo one, a shell being all it holds), loading the next
    # shell and its empty click; and its shells (TankShell_PROJMESH): the loop they
    # fly with (FlightSound) and going off (ExplosionSound).
    "BL_WPN_GUN_TANK_CANNON.SHOOT",
    "BL_WPN_GUN_TANK_CANNON.SHOOT_LOW_AMMO",
    "BL_WPN_GUN_TANK_CANNON.ANIM_RELOAD",
    "BL_WPN_FOLEY.SHOOT_DRY_FIRE_PLASMA_01",
    "BL_WPN_GUN_TANK_CANNON.TRAIL_LP",
    "BL_WPN_GUN_TANK_CANNON.EXPL_IMPT_WORLD",
    # A jet's homing rockets (the Scientist's vehicles' HomingRocketVehicle:
    # TnWeaponEffectSound and TnWeaponEffectLowAmmoSound of WEP_HomingRocketVh_p.
    # HomingRocket_Vehicle_WEPMESH): each rocket's shot, the reload, the empty click
    # and the tone while it locks on (the HUD's, looping from StartLockOn to
    # StopLockOn); and its rockets (HomingRocketVh_PROJMESH): the loop they fly with
    # (FlightSound) and going off (ExplosionSound).
    "BL_WPN_GUN_ROCKET.SHOOT_VEH",
    "BL_WPN_GUN_ROCKET.ANIM_RELOAD_veh",
    "BL_WPN_FOLEY.SHOOT_DRY_FIRE_BALLISTIC",
    "BL_HUD_INTERFACE.HUD_TARGET_LOCK",
    # A weapon upgrade's part glowing as it goes on in Customize
    # (TnWeaponMesh.WeaponModificatonHighlightSound).
    "BL_HUD_INTERFACE.WPN_UPGRADE_GLOW",
    # Experience (src/xp.rs): a reward's point transaction on the HUD
    # (mc_pointTransaction's MP_REWARD_DIALOG_BOX) and a level up (the HUD's
    # NotifyLevelUp, TnPlayerController's LevelUpMusic stinger).
    "BL_LVL_MP.MP_REWARD_DIALOG_BOX",
    "BL_LVL_MP.MP_LEVEL_UP_MX_STNG",
    "BL_WPN_GUN_ROCKET.TRAIL_LP",
    "BL_WPN_GUN_ROCKET.EXPL_IMPT_WORLD",
    # A truck's rockets (the Destroyer's vehicles' RocketVehicle: WEP_RocketVh_p.
    # Rocket_Vehicle_WEPMESH's shot, reload and empty click are the jets' cues
    # above), flying with the same loop; going off as Rocket_PROJMESH's
    # ExplosionSound.
    "BL_WPN_GUN_ROCKET.VEH_EXPL_IMPT_WORLD",
    # Grenade
    "BL_WPN_GRENADE.MAGMA_DEPLOY",
    "BL_WPN_GRENADE.MAGMA_EXPL_IMPT_WORLD",
    # The enemy soldiers' grenades (TnProjectileGrenade's GrenadeSkill_PROJMESH,
    # src/ai/grenade.rs): flying, and the fuse once they first land (they bounce
    # with GRENADE_FOLEY_SHELL_BOUNCE_HEAVY and go off with MAGMA_EXPL_IMPT_WORLD).
    "BL_WPN_GRENADE.MAGMA_FLIGHT_LP",
    "BL_WPN_GRENADE.MAGMA_FUSE",
    # The Infiltrator's EMP grenade (WEP_FlashBangs_p.FlashBang_PROJMESH): its flight
    # loop, bouncing, the fuse once it first lands, and going off; the speed boost
    # its Speed Redirector upgrade gives (TnBuffGrenadeIncreaseSpeed's ApplySound and
    # UnapplySound, played by the Infiltrator's sound event set as these cues); and
    # the HUD's ability meter coming full (robotAbility_mc: HUD_ABILITY_READY).
    "BL_WPN_GRENADE.EMP_FLIGHT_LP",
    "BL_WPN_GRENADE.GRENADE_FOLEY_SHELL_BOUNCE_LIGHT",
    "BL_WPN_GRENADE.EMP_FUSE_BUILD",
    "BL_WPN_GRENADE.EMP_EXPL_IMPT_WORLD",
    "BL_TRANS_POWER.SPEED_BOOST_START",
    "BL_TRANS_POWER.SPEED_BOOST_END",
    "BL_HUD_INTERFACE.HUD_ABILITY_READY",
    # Melee
    "BL_CHR_GRIMLOCK.ROBO_ATTACK_SHIELD_BASH_SERVO",
    "BL_CHR_GRIMLOCK.ROBO_ATTACK_SHIELD_BASH_SWING",
    "BL_CHR_GRIMLOCK.ROBO_FS_DEFAULT_SHIFT_MTL",
    "BL_WPN_MELEE.SWORD_SWING_LT_02",
    "BL_WPN_MELEE.MELEE_IMPT_DMG_TO_AI",
    # Jazz's body: footsteps, jumps, landings, hits, death
    "BL_FS_SML_BOT.FS_JOG_DEFAULT",
    "BL_FS_SML_BOT.FS_RUN_DEFAULT",
    "BL_FS_SML_BOT.FS_WALK_DEFAULT",
    "BL_FS_SML_BOT.FS_SPRINT_START",
    "BL_FS_SML_BOT.FS_JUMP",
    "BL_FS_SML_BOT.FS_JUMP_CHARGED",
    "BL_FS_SML_BOT.FS_LAND_DEFAULT",
    "BL_FS_SML_BOT.FS_LAND_HARD",
    "BL_FS_SML_BOT.FS_LAND_HIGH_FALL",
    "BL_FS_MED_BOT.FOLEY_SERVO_HIT_RX",
    "BL_LVL_HUD_INTERFACE.HUD_DEATH_POWER_DOWN",
    "BL_BF_BODYFALLS.BF_DEFAULT_BODY_MEDIUM",
    # Transformation
    "BL_TRANSFORM.JAZZ_BOT2VEH",
    "BL_TRANSFORM.JAZZ_VEH2BOT",
    # Jazz's car
    "BL_VEH_CAR.VEH_SPORT_01_DRIVE_ONLOAD",
    "BL_VEH_CAR.VEH_SPORT_01_BOOST_START",
    "BL_VEH_CAR.VEH_SPORT_01_BOOST_END",
    "BL_VEH_CAR.VEH_SPORT_01_TIRE_SQUEAL",
    "BL_VEH_CAR.VEH_SPORT_01_IMPT_HEAVY",
    "BL_VEH_CAR.VEH_SPORT_01_IMPT_LIGHT",
    "BL_VEH_CAR.VEH_SPORT_01_HOVER_LAND_LIGHT",
    "BL_VEH_CAR.VEH_SPORT_01_HOVER_LAND_HEAVY",
    # Driving moves (the car's SoundEventSet VEH_SPORT_01): jumping, wheel landings,
    # the hover-mode dash and hover jets.
    "BL_VEH_CAR.VEH_SPORT_01_DRIVE_JUMP_START",
    "BL_VEH_CAR.VEH_SPORT_01_BOOST_JUMP_START",
    "BL_VEH_CAR.VEH_SPORT_01_WHEELS_LAND_LIGHT",
    "BL_VEH_CAR.VEH_SPORT_01_WHEELS_LAND_HEAVY",
    "BL_VEH_CAR.VEH_SPORT_01_BOOST_DASH",
    # Its DeathSound (destroyed in it). The other characters' vehicles' sounds come
    # from their classes' files (VEHICLE_EVENTS, below).
    "BL_VEH_CAR.VEH_SPORT_01_DEATH",
    # World
    "BL_HUD_INTERFACE.INTERACT_PICKUP_ENERGON_SHARD",
    # The shield running out and filling up again (TnShieldHealth's
    # ShieldDrainedSound and ShieldFullSound).
    "BL_HUD_INTERFACE.OVERSHIELD_POWER_DEPLETED",
    "BL_HUD_INTERFACE.OVERSHIELD_POWER_RECHARGED",
    # Buying health at an Escalation health station (TnHealthPickup's PickupSound),
    # ammo at an ammo station (TnAmmoCratePickup's), and trying to without enough
    # energon (TnPickupManager's InsufficientFundsSound).
    "BL_HUD_INTERFACE.HEALTH_PU_ENERGON",
    "BL_HUD_INTERFACE.HEALTH_PU_AMMO",
    "BL_LVL_MP.ESC_INSUFFICIENT_FUNDS",
    # An Armory Recreator opening (AP_StoragePodOpen's sound notify), and an unlock
    # console's deposits (TnInteractCurrencyReceptacle: each deposit, and the last
    # one that fills it).
    "BL_TRANS_PROP_INTERACT.LOOT_BOX_OPEN",
    "BL_LVL_MP.ESC_MONEY_DEPOSIT",
    "BL_LVL_MP.ESC_MONEY_DEPOSIT_FULL",
    # The locked doors opening (src/doors.rs: the sound tracks of the Matinees the
    # consoles' purchases play, and the Optimus doors' AP_OptimusDoor_Open notifies):
    # Downfall's sliding doors, Oblivion's and Ancients' dropship doors, Ancients'
    # bunker and barricades, Ignition's shield, and the Optimus doors.
    "BL_LVL_ESC_IACON.EMIT_SLIDING_SIDE_DOOR",
    "BL_LVL_ESC_IACON.EMIT_SLIDING_SIDE_DOOR_FAST",
    "BL_LVL_ESC_ARK.DOOR_OPEN_START",
    "BL_LVL_ESC_ARK.DOOR_OPEN_END",
    "BL_LVL_ESC_TEMPLE.DOOR_OPEN_START",
    "BL_LVL_ESC_TEMPLE.DOOR_OPEN_END",
    "BL_LVL_ESC_TEMPLE.BIG_DOOR_SERVO",
    "BL_LVL_ESC_TEMPLE.BIG_DOOR_CLOSE1",
    "BL_LVL_ESC_TEMPLE.BIG_DOOR_CLOSE2",
    "BL_LVL_ESC_TEMPLE.EMIT_BARRICADE_DOOR_OPEN",
    "BL_LVL_ESC_GLADIATOR.TRAP_SHIELD_DEPLOY",
    "BL_LVL_02_OPTIPLEX.DOOR_IAC_OPTIMUS_OPEN_01",
    "BL_LVL_02_OPTIPLEX.DOOR_IAC_OPTIMUS_OPEN_02",
    "BL_LVL_02_OPTIPLEX.DOOR_IAC_OPTIMUS_OPEN_03",
    # Downfall's scaffolds coming down as the Courtyard's and the Doors' consoles are
    # bought (src/doors.rs's collapse: their destroyed state's
    # HmSoundDestructionEffectFactory, DES_ASH_ESCBeam1_DSYS's).
    "BL_DEST_MTL.DEST_ASH_COLUMN_EXPL",
    # Oblivion's exploding barrel (src/barrels.rs: DES_AutobotExplodingBarrel_DSYS's
    # HmSoundDestructionEffectFactory): its hum while it stands (its first state's)
    # and its blast (its blown-up state's).
    "BL_LVL_06_MEGATRON.EMIT_AMB_MACH_OSC_07",
    "BL_DEST_ELEC.DEST_ENERGON_TANK_01_EXPLO",
    "BL_TRANS_POWER.SENTRY_SHOOT",
    "BL_CHR_AI_SQUID_MINE.DEATH_EXPLO",
    # Main menu (FrontEnd_GFX's menu_Main and the settings menu): moving through the
    # menu, choosing an item, opening and closing a sub-menu, a slider's step, and
    # what the Audio menu's volume sliders play as they move (SettingsMenu_GFX's
    # setSettingsModified: a beep for FX Volume, a line of Optimus Prime's for
    # Dialogue Volume, a few seconds of a music sting for Music Volume).
    "BL_HUD_INTERFACE.CUST_SUBMENU_01_SCROLL_UP",
    "BL_HUD_INTERFACE.CUST_SUBMENU_01_SCROLL_DOWN",
    "BL_HUD_INTERFACE.CUST_SUBMENU_01_PRESS_IN",
    "BL_HUD_INTERFACE.CUST_SUBMENU_01_PRESS_OUT",
    "BL_HUD_INTERFACE.PAUSE_SUBMENU_ENTER",
    "BL_HUD_INTERFACE.PAUSE_SUBMENU_EXIT",
    "BL_HUD_INTERFACE.BUTTON_CALIBRATE",
    # (The Display menu's Brightness and Safe Frame screens: Accept's sound.)
    "BL_HUD_INTERFACE.CAMPAIGN_MENU_LEVEL_SET_DIFF",
    "BL_HUD_INTERFACE.SFX_SLIDER_VOLUME",
    "BL_HUD_INTERFACE.DX_SLIDER_VOLUME",
    "BL_HUD_INTERFACE.MX_SLIDER_VOLUME",
    # The Customize screen's shop (CustomCharacter_GFX): a part or armor set bought
    # (showPurchaseAlert) and too few shards to buy it (showInsufficientFunds).
    "BL_HUD_INTERFACE.UI_MONEY_DEPOSIT_SUCESS",
    "BL_HUD_INTERFACE.UI_MONEY_DEPOSIT_FAIL",
    # The pause menu (PauseMenu_GFX): opening it in a match, moving through it, and
    # going back to the game; the sub-menu sounds above go in and out of its
    # settings and Quit Game.
    "BL_HUD_INTERFACE.PAUSE_MENU_OPEN_MP",
    "BL_HUD_INTERFACE.PAUSE_MENU_SCROLL_UP",
    "BL_HUD_INTERFACE.PAUSE_MENU_SCROLL_DOWN",
    "BL_HUD_INTERFACE.BUTTON_REENTER_GAMEPLAY",
    # The Extras menu (ExtrasMenu_GFX's panel buttons, HmButton): rolling over a
    # button, and pressing one.
    "BL_HUD_INTERFACE.BUTTON_UP",
    "BL_HUD_INTERFACE.BUTTON_ACCEPT",
    # The Escalation lobby (MPLobby_GFX's menus, HmMenu): moving down a menu (up is
    # BUTTON_UP), going back, Start Game, the difficulty selector, a locked item,
    # and each second of the countdown to the match.
    "BL_HUD_INTERFACE.BUTTON_DOWN",
    "BL_HUD_INTERFACE.BUTTON_BACK",
    "BL_HUD_INTERFACE.BUTTON_START",
    "BL_HUD_INTERFACE.BUTTON_SLIDER_INC_DECREASE",
    "BL_HUD_INTERFACE.BUTTON_LOCKED_ITEM",
    "BL_HUD_INTERFACE.MULTI_MATCH_COUNTDOWN",
    # The countdown's tick on the starting scoreboard (TnGameReplicationInfo's
    # LowCountdownTickSound, MP_COUNTDOWN_TICK).
    "BL_LVL_MP.MP_COUNTDOWN_BEEP",
    # The Customize screen (CustomCharacter_GFX): the class select opening and
    # closing and moving between classes, the editor's pages and option lists, a
    # locked option, the metallic slider, and buying (not used by the demo).
    "BL_HUD_INTERFACE.CUST_CHARACTER_SELECT_PRESS_IN",
    "BL_HUD_INTERFACE.CUST_CHARACTER_SELECT_PRESS_OUT",
    "BL_HUD_INTERFACE.CUST_CHARACTER_SELECT_SCROLL_LEFT",
    "BL_HUD_INTERFACE.CUST_CHARACTER_SELECT_SCROLL_RIGHT",
    "BL_HUD_INTERFACE.CUST_SUBMENU_02_PRESS_IN",
    "BL_HUD_INTERFACE.CUST_SUBMENU_02_PRESS_OUT",
    "BL_HUD_INTERFACE.CUST_SUBMENU_02_SCROLL_UP",
    "BL_HUD_INTERFACE.CUST_SUBMENU_02_SCROLL_DOWN",
    "BL_HUD_INTERFACE.CUST_TAB_CHANGE",
    # The garage's sounds (UI_CustomCharacter_m's Kismet): its ambience while the
    # previews are shown (a looping bed, two computer screens, and beeps, servos,
    # whispers and a synth fly-by now and then), and the dissolve a preview robot
    # shows and hides with (TnSeqAct_DeresCustomizationPawn).
    "BL_LVL_HUD_INTERFACE.MP_CUSTOMIZATION_AMB_BED",
    "BL_LVL_HUD_INTERFACE.MP_CUSTOMIZATION_EMIT_COMP_SCREEN_01",
    "BL_LVL_HUD_INTERFACE.MP_CUSTOMIZATION_EMIT_COMP_SCREEN_02",
    "BL_LVL_HUD_INTERFACE.MP_CUSTOMIZATION_PP_BEEPS",
    "BL_LVL_HUD_INTERFACE.MP_CUSTOMIZATION_PP_MACH_SERVO",
    "BL_LVL_HUD_INTERFACE.MP_CUSTOMIZATION_PP_MTL_WHISPER",
    "BL_LVL_HUD_INTERFACE.MP_CUSTOMIZATION_BYS_SYNTH",
    "BL_DEATH.DERES_DISSOLVE_REMAINS",
    # Dying: the death sets' sounds (TnDeathActionPlaySound: TR_Deaths_p.Sets.
    # Basic_DEATHSET's, which the players' robots and Escalation's soldiers die by,
    # and the other enemies' own sets'), and the explosions their kills set off.
    "BL_DEATH.DERES_INITIAL_POWER_DOWN",
    "BL_DEATH.GIB_DEBRIS_ELEC_SIZZLE_OUT",
    "BL_DEATH.Headshot",
    "BL_DEATH.ELECTROCUTE_BUILDUP",
    "BL_DEATH.CORROSION",
    "BL_DEATH.EXPL_ELEC_FIRE",
    "BL_DEATH.EXPL_ELEC_TRANS",
    "BL_DEATH.INSTA_GIB_MTL",
    "BL_DEATH.DERES_HOLO_RES_OUT",
    "BL_CHR_AI_GUARD.GUARD_DEATH_EXPLO",
    "BL_CHR_AI_INSCT_SWARMER.DEATH_VOX_FULL_SCREAM",
    "BL_CHR_AI_LEAPER.DEATH_EXPL",
    "BL_CHR_AI_SNIPER_DEC.SNIPER_DEATH_RX",
    "BL_VEH_JET.VEH_JET_FLYER_01_DEATH",
    # (The Titan's drones': their vehicle form's DeathSound, TnVehicleForm.OnPlayDeath.)
    "BL_CHR_AI_SENTINEL.SENTINEL_DEATH",
    "BL_WPN_GUN_TANK_CANNON.CLUSTER_BOMB_PROJ_EXPL",
    # The front end's music: the Transformers theme, then "Metroplex Revealed"
    # fading in and looping.
    "BL_LVL_HUD_INTERFACE.MP_FRONT_END",
    # The announcer naming the game type as a match starts (TnGameTypeMessageSV's
    # GameTypeDialog, MP_GameTypeEscalationDialog, which the CHR_ANNOUNCER sound
    # event set plays as this cue): "Escalation".
    "BL_DX_SWITCHBOARD.SC009019",
    # Escalation's downed players (src/revive.rs): the announcer's "An ally was
    # destroyed" as a player dies (TnPlayerPawnSurvival.PlayDying's
    # MP_ESC_ALLY_DESTROYED, which the CHR_ANNOUNCER set plays as this cue), and
    # the stinger as the team loses a life (TnSurvivalTimeMessage's WaveFailMusic).
    "BL_DX_SWITCHBOARD.SC019358",
    "BL_LVL_MP.STNG_ESCALATION_WAVE_OVER_FAIL",
    # Escalation's waves (src/waves.rs): the announcer's lines (TnSurvivalTimeMessage's
    # DialogMessages, which the CHR_ANNOUNCER set plays as these cues: "Wave begins
    # in", "Five" to "One" (MP_ESC_WAVE_COUNTDOWN_START, _5 to _1), "Wave complete",
    # "Three enemies remaining" and "One enemy remaining" (MP_ESC_3_KILLS_ and
    # MP_ESC_1_KILL_FROM_VICTORY)); the stingers as a wave begins and is won (its
    # WaveBeginMusic and WaveCompleteMusic); and the scan as the last enemies show
    # (TnBuffSeeEnemyObjectiveMarkers' ApplySound, ABILITY_ORBITAL_BEACON_SCAN).
    "BL_DX_SWITCHBOARD.SC017783",
    "BL_DX_SWITCHBOARD.SC017785",
    "BL_DX_SWITCHBOARD.SC017787",
    "BL_DX_SWITCHBOARD.SC017789",
    "BL_DX_SWITCHBOARD.SC017791",
    "BL_DX_SWITCHBOARD.SC017793",
    "BL_DX_SWITCHBOARD.SC017781",
    "BL_DX_SWITCHBOARD.SC019361",
    "BL_DX_SWITCHBOARD.SC019363",
    "BL_LVL_MP.STNG_ESCALATION_WAVE_BEGIN",
    "BL_LVL_MP.STNG_ESCALATION_WAVE_OVER_SUCCESS",
    "BL_TRANS_POWER.ORBITAL_BEACON_ONLINE_SCAN",
    # The music a match starts with (TnGameTypeMessageSV's GameTypeMusic): one of 13
    # stingers from the campaign's score, picked at random, a second in.
    "BL_LVL_MP.MX_MATCH_BEGIN",
    # Escalation's shop (src/shop.rs, main.rs): buying heavy ammo, and the support
    # items' pick-up sounds (their TnEquipmentPickups' PickupSound).
    "BL_HUD_INTERFACE.HEALTH_PU_AMMO_HEAVY",
    "BL_TRANS_POWER.BARRIER_PU",
    "BL_TRANS_POWER.ENERGON_RECHARGER_PU",
    "BL_TRANS_POWER.INTERCOOLER_PU",
    # The support items in use (src/equipment.rs): the Diffraction Barrier going up,
    # standing and going (TnAbilityBarrier's ABILITY_BARRIER_DEPLOY, TnBarrierSpawnable's
    # loop and retract), the Energon Recharger's and the Intercooler's loops while
    # they last and their ends (their buffs' ..._LP_START and _LP_STOP events), and
    # the Thermo Mine Payload's mines: dropped (ABILITY_KAMIKAZE_DEPLOY), hovering,
    # homing in, their fuse and their blast (TnProjectileKamikazeMine).
    "BL_TRANS_POWER.BARRIER_DEPLOY",
    "BL_TRANS_POWER.BARRIER_LP",
    "BL_TRANS_POWER.BARRIER_RETRACT",
    "BL_TRANS_POWER.ENERGON_RECHARGER_START_LP",
    "BL_TRANS_POWER.ENERGON_RECHARGER_END",
    "BL_TRANS_POWER.INTERCOOLER_START_LP",
    "BL_TRANS_POWER.INTERCOOLER_END",
    "BL_WPN_GRENADE.KAMIKAZE_DEPLOY",
    "BL_WPN_GRENADE.KAMIKAZE_FLIGHT_LP_IDLE",
    "BL_WPN_GRENADE.KAMIKAZE_FLIGHT_LP_TRACKING",
    "BL_WPN_GRENADE.KAMIKAZE_FUSE_START",
    "BL_WPN_GRENADE.KAMIKAZE_EXPL_IMPT_WORLD",
    # The Dimensional Decimator: bought (its PickupSound), thrown (the robot's
    # ABILITY_GRAVITY_GRENADE_DEPLOY), its ball rolling, its fuse and blast, and the
    # black hole's loop and collapse (TnRollerMineGravityAura, TnGravityAura).
    "BL_WPN_FOLEY.WPN_GRENADE_PICKUP",
    "BL_WPN_GRENADE.GRAVITY_GRENADE_DEPLOY_ROBOT",
    "BL_WPN_GRENADE.GRAVITY_GRENADE_DEPLOY",
    "BL_WPN_GRENADE.GRAVITY_GRENADE_FLIGHT_LP",
    "BL_WPN_GRENADE.GRAVITY_GRENADE_FUSE",
    "BL_WPN_GRENADE.GRAVITY_GRENADE_IMPT_WORLD",
    "BL_WPN_GRENADE.GRAVITY_GRENADE_CYCLONE_LP",
    "BL_WPN_GRENADE.GRAVITY_GRENADE_CYCLONE_END_EXPLO",
    # The classes' abilities (src/abilities.rs). The Cloak: its loop as it starts
    # and its end, by faction (TnBuffCloak's ABILITY_CLOAK_LP_START and _LP_STOP),
    # and its warning as it runs low (TnCloakMaterialOverrider's CloakFadeOutSound,
    # ABILITY_CLOAK_POWER_OUT: CHR_MP_INFILTRATOR_1's, the only one there is).
    "BL_TRANS_POWER.CLOAK_AUTOBOT_START_LP",
    "BL_TRANS_POWER.CLOAK_AUTOBOT_OFF",
    "BL_TRANS_POWER.CLOAK_DECEPTICON_START_LP",
    "BL_TRANS_POWER.CLOAK_DECEPTICON_OFF",
    "BL_TRANS_POWER.CLOAK_AUTOBOT_POWER_OUT",
    # The Whirlwind: its start and end, its swish (Transform_Whirlwind_ROBO's
    # notifies) and the blade's whooshes.
    "BL_TRANS_POWER.WHIRLWIND_START",
    "BL_TRANS_POWER.WHIRLWIND_END",
    "BL_TRANS_POWER.WHIRLWIND_COMPLETE",
    "BL_TRANS_POWER.WHIRLWIND_WHOOSH_SINGLE",
    # The Heal Beam: no one to heal, a reward, the beam's loop on its target, its
    # buff's loop and its retracting (TnAbilityHealBeamBase).
    "BL_TRANS_POWER.HEAL_FAIL",
    "BL_TRANS_POWER.HEAL_SUCCESS",
    "BL_TRANS_POWER.HEAL_INITIALIZE_LP",
    "BL_TRANS_POWER.HEAL_ACTIVE_LP",
    "BL_TRANS_POWER.HEAL_RETRACT",
    # The Flak Shield: raised and lowered (the shield mesh's TnWeaponEventEquip and
    # UnEquip), and broken.
    "BL_CHR_GRIMLOCK.SHIELD_ANIM_EQUIP",
    "BL_CHR_GRIMLOCK.SHIELD_ANIM_RETRACT",
    "BL_WPN_MELEE.SHIELD_ROBO_AUTOBOT_BREAK",
    # The Hover Slam: the hover's loop and its end (TnAcrobaticsManager's
    # ABILITY_HOVERJUMP_LP_START and _LP_STOP), the slam, and a heavy landing.
    "BL_TRANS_POWER.HOVER_JUMP_START_LP",
    "BL_TRANS_POWER.HOVER_JUMP_END",
    "BL_TRANS_POWER.MEGATRON_HOVER_SMASH",
    "BL_VEH_TRUCK.VEH_TRUCK_02_HOVER_LAND_HEAVY",
    # The Battle Sentry: deployed, its idle loop, a target acquired, destroyed, and
    # the Rust Sentry's beam (BL_WPN_GUN_REPAIR_WPN, TnWeaponBeam's).
    "BL_TRANS_POWER.SENTRY_ACTIVATE",
    "BL_TRANS_POWER.SENTRY_IDLE_LP",
    "BL_TRANS_POWER.SENTRY_TARGET_AQUIRED",
    "BL_TRANS_POWER.SENTRY_EXPL",
    "BL_WPN_GUN_REPAIR_WPN.SHOOT_LP",
    "BL_WPN_GUN_REPAIR_WPN.SHOOT_DMG_LP",
    "BL_WPN_GUN_REPAIR_WPN.SHOOT_LAST_SHOT",
    "BL_WPN_GUN_REPAIR_WPN.IMPT_WORLD",
    # The maps' traps (src/traps.rs; levels/<map>_doors.txt names the cues their
    # Kismet and Matinees play). Ignition's flame jets: their vents' warning, the
    # flames starting and stopping (the steam vents' Matinees' sound tracks) and
    # their roar while they burn (the Kismet's, along each jet's
    # HmAmbientSoundLineEmitter); its GunShip Support coming in, hovering and
    # leaving.
    "BL_LVL_ESC_GLADIATOR.TRAP_FIRE_HAZARD_TELL",
    "BL_LVL_ESC_GLADIATOR.TRAP_FIRE_HAZARD_START",
    "BL_LVL_ESC_GLADIATOR.TRAP_FIRE_HAZARD_STOP",
    "BL_LVL_ESC_GLADIATOR.TRAP_FIRE_HAZARD_LP",
    "BL_LVL_ESC_GLADIATOR.TRAP_GUNSHIP_APPROACH",
    "BL_LVL_ESC_GLADIATOR.TRAP_GUNSHIP_IDLE",
    "BL_LVL_ESC_GLADIATOR.TRAP_GUNSHIP_EXIT",
    # Oblivion's combustion chamber: its alarm (at its light) and its blast (the
    # map's AUDIO package's Kismet, ESC_Ark_AUDIO_m).
    "BL_LVL_ESC_ARK.TRAP_AIRLOCK_ALARM_QUAD",
    "BL_LVL_ESC_ARK.TRAP_AIRLOCK_FIRE_BLAST",
    # Ancients' electric fences: their crackle switching on and off (the Kismet's,
    # along the fence's HmAmbientSoundLineEmitter; the game's own spelling).
    "BL_LVL_ESC_TEMPLE.EMIT_TRAP_ELECRTIC_GATES",
    "BL_LVL_ESC_TEMPLE.EMIT_TRAP_ELECRTIC_GATES_OFF",
    # The sentries (TnSentryPawn): unfolding and folding up (their Activate and
    # DeActivate clips' notifies); the GunShip's guns firing (MGInteract_Sentry_WEPMESH's
    # TnWeaponEffectSound: a loop from StartFire, its end on StopFire) and their rounds
    # striking; Downfall's rocket sentries firing (RocketInteract_Sentry_WEPMESH's) -
    # their rockets' flight and blast are the Thermo Rocket Cannon's,
    # BL_WPN_GUN_ROCKET's TRAIL_LP and EXPL_IMPT_WORLD.
    "BL_WPN_GUN_AI_SENTRY.AI_TURRET_ENGAGE",
    "BL_WPN_GUN_AI_SENTRY.AI_TURRET_DISENGAGE",
    "BL_WPN_GUN_AI_SENTRY.ESCALATION_GUNSHIP_SHOOT_START",
    "BL_WPN_GUN_AI_SENTRY.AI_TURRET_MG_SHOOT_END",
    "BL_WPN_GUN__IMPT_BYS.IMPT_BALLISTIC_WORLD_ESC_GUNSHIP",
    "BL_WPN_GUN_ROCKET.SHOOT",
]

# The classes' characters' own sounds, as tools/build_class.py lists them in
# models/<class>/<class>.txt: each vehicle's cues for the events the demo plays (its
# form's sound event set: the engine, boosting (its start and end, its engine's loops
# while it boosts, HmPlayerVehicleAudioComponent's BoostSounds, and its wheels' on the
# ground, BoostWheelsSound), the tyres, crashes, jumps and landings, and dying in it;
# a jet's JET_EVENTS), every transformation's, those the vehicles' own clips
# play (Snarl's and Slug's footsteps, tails and growls), the robot's (its class's
# sound event set: footsteps, jumps, landings and body falls) and those its own
# clips play (the Destroyer's sword swing).
CLASSES = ["infiltrator", "destroyer", "titan", "scientist"]
VEHICLE_EVENTS = {
    "Auto_Engine_Gear_1_OnLoad", "Auto_Boost_Start", "Auto_Boost_End", "Auto_Tire_Squeal_Default",
    "Auto_Boost_Loop", "Auto_Boost_Loop_Exhaust", "Auto_Boost_Wheels",
    "BF_DEFAULT_ROBOT_HEAVY", "BF_DEFAULT_ROBOT_LIGHT", "Auto_Jump_Start", "Auto_Boost_Jump",
    "Auto_Land_Wheels_Light", "Auto_Land_Wheels_Heavy", "Auto_Land_Hover_Light",
    "Auto_Land_Hover_Heavy", "Death",
}
# A jet's too (the Scientist's: its HmPlayerVehicleAudioComponent's): its hover's
# engine at rest and the whine under load, its flight's loops, its hover jets'
# wash, climbing and diving and their stops, the engine's start and stop as it
# moves off and comes to rest, and the gunship's dash.
JET_EVENTS = {
    "Auto_Engine_Gear_1_OffLoad", "Auto_Onload_Whine", "Auto_Boost_Loop", "Auto_Boost_Whine",
    "Auto_Hover_Boosters", "Auto_Engine_Hover_Ascend", "Auto_Engine_Hover_Ascend_Stop",
    "Auto_Engine_Hover_Descend", "Auto_Engine_Hover_Descend_Stop", "Auto_Engine_Gear_1_OnLoad_Oneshot",
    "Auto_Engine_Gear_1_OffLoad_Oneshot", "Auto_Boost_Dash",
}


# The cues the enemies' code plays by name (src/ai): their guns' (GUN_FX: their
# weapon meshes' TnWeaponEffects), the Guard's charge and its scanner's decloak, the
# Titans' overheating, the grenades'.
AI_CUES = [
    "BL_WPN_GUN_PHOTON_ELIMINATOR.SHOOT", "BL_WPN_GUN_PHOTON_ELIMINATOR.SHOOT_TAIL",
    "BL_WPN_GUN__IMPT_BYS.IMPT_BALLISTIC_WORLD", "BL_WPN_GUN__IMPT_BYS.IMPT_SHOTGUN_WORLD",
    "BL_WPN_GUN__IMPT_BYS.IMPT_SNIPER_WORLD",
    "BL_WPN_GUN_SCATTER_BLASTER.AUTO_SHOTGUN_SHOOT", "BL_WPN_GUN_SCATTER_BLASTER.AUTO_SHOTGUN_SHOOT_TAIL",
    "BL_WPN_GUN_SCATTER_BLASTER.ANIM_RELOAD",
    "BL_WPN_GUN_ROCKET.SHOOT", "BL_WPN_GUN_ROCKET.ANIM_EQUIP", "BL_WPN_GUN_ROCKET.ANIM_HOLSTER",
    "BL_WPN_GUN_ROCKET.ANIM_RELOAD",
    "BL_WPN_GUN_AI_SNIPER.AI_LASER_SNIPER_SHOOT",
    "BL_WPN_GUN_AI_TITAN.SHOOT", "BL_WPN_GUN_AI_TITAN.SHOOT_LP", "BL_WPN_GUN_AI_TITAN.SHOOT_TAIL",
    "BL_CHR_AI_TITAN.TITAN_GUN_OVERHEAT",
    "BL_WPN_GUN_AI_SENTRY.AI_GUARDIAN_BLASTER_SHOOT", "BL_CHR_AI_GUARD.GUARD_CHARGE_SHOOT_LP",
    "BL_CHR_AI_GUARD.GUARD_SCANNER_DECLOAK",
    "BL_WPN_GRENADE.GRENADE_FOLEY_SHELL_BOUNCE_HEAVY", "BL_WPN_GRENADE.MAGMA_EXPL_IMPT_WORLD",
    "BL_WPN_GRENADE.MAGMA_FLIGHT_LP", "BL_WPN_GRENADE.MAGMA_FUSE",
    # The jets' rockets' shot (FlyerRockets_AI_WEPMESH's TnWeaponEffectSound) and
    # their dogfight gun's (Rocket_Vehicle_WEPMESH's, the trucks' too).
    "BL_WPN_GUN_AI_PLANE.FLYER_ROCKETS_SHOOT", "BL_WPN_GUN_ROCKET.SHOOT_VEH",
    # The soldiers' rounds flying past a player (BurstRifle_AI_WEPMESH's
    # BulletBySound: HmPawn.NotifyNearlyShot), and the Snipers' and sharpshooters'
    # (Sniper_WEPMESH's and SharpShot_WEPMESH's).
    "BL_WPN_GUN__IMPT_BYS.BY_BALLISTIC", "BL_WPN_GUN__IMPT_BYS.BY_SNIPER",
]
# The AI data's properties that name cues (models/enemies/ai.txt: its weapons' and
# projectiles' flight, blast, fuse and detach sounds, the sharpshooters' scan).
AI_CUE_KEYS = ("FlightSound", "ExplosionSound", "FuseSound", "DetachAudioCue", "Sound")


def enemy_cues(assets):
    """The enemies' cues, each with the dialog characters that may speak it: their
    sound sets' (tools/build_ai.py's models/enemies/ai_sounds.txt: a set's dialog
    cues are spoken by its dialog characters), their animations' own
    (tools/build_ai_anims.py's ai_anims_<set>.txt sound and voice lines), their weapons' and
    projectiles' (ai.txt's AI_CUE_KEYS), the ones their code plays (AI_CUES) and their
    deaths' and the players' (tools/build_death_sets.py's deaths.txt and
    tools/build_deaths.py's robot_deaths.txt: their Sound actions' cues, their
    deressers' and their gibs')."""
    out = {}
    folder = os.path.join(assets, "models", "enemies")
    path = os.path.join(folder, "ai_sounds.txt")
    if os.path.exists(path):
        rows = [line.rstrip("\n").split("\t") for line in open(path, encoding="utf-8")]
        sets = {f[1]: f[2:] for f in rows if f[0] == "set" and len(f) >= 2}
        for f in rows:
            if f[0] == "cue" and len(f) >= 4:
                out.setdefault(f[3], set()).update(sets.get(f[1], []))
    for name in sorted(os.listdir(folder)) if os.path.isdir(folder) else []:
        if name.startswith("ai_anims") and name.endswith(".txt"):
            for line in open(os.path.join(folder, name), encoding="utf-8"):
                f = line.rstrip("\n").split("\t")
                if f[0] in ("sound", "voice") and len(f) >= 4:
                    out.setdefault(f[3], set())
    path = os.path.join(folder, "ai.txt")
    if os.path.exists(path):
        for line in open(path, encoding="utf-8"):
            f = line.rstrip("\n").split("\t")
            if len(f) >= 3 and f[-2].split(".")[-1] in AI_CUE_KEYS and "." in f[-1] and f[-1].startswith("BL_"):
                out.setdefault(f[-1], set())

    for cue in death_cues(assets):
        out.setdefault(cue, set())
    for cue in AI_CUES:
        out.setdefault(cue, set())
    return out


def death_cues(assets):
    """The cues the enemies' and the players' deaths play (tools/build_death_sets.py's
    deaths.txt and tools/build_deaths.py's robot_deaths.txt: their Sound actions',
    their deressers' and their gibs'), by their own names (a dialog cue among them,
    the Leaper's death cry, with all its lines)."""
    out = []
    for path in (os.path.join(assets, "models", "enemies", "deaths.txt"),
                 os.path.join(assets, "models", "robot_deaths.txt")):
        if not os.path.exists(path):
            continue
        for line in open(path, encoding="utf-8"):
            for field in line.rstrip("\n").split("\t"):
                cue = field.split("=", 1)[1] if field.startswith(("cue=", "sound=")) else field
                if cue.startswith("BL_") and "." in cue and cue not in out:
                    out.append(cue)
    return out


def character_cues(assets):
    """The cues the classes' files name for VEHICLE_EVENTS, the transformations, the
    vehicles' clips, the robots and their own clips."""
    out = []
    for cls in CLASSES:
        path = os.path.join(assets, "models", cls, cls + ".txt")
        if not os.path.exists(path):
            continue
        for line in open(path, encoding="utf-8"):
            f = line.rstrip("\n").split("\t")
            jet = cls == "scientist"
            if f[0] == "vsound" and len(f) >= 4 and (f[2] in VEHICLE_EVENTS or (jet and f[2] in JET_EVENTS)):
                out.append(f[3])
            elif f[0] in ("vtsound", "vcsound") and len(f) >= 5:
                out.append(f[4])
            elif f[0] == "csound" and len(f) >= 3:
                out.append(f[2])
            elif f[0] == "rcsound" and len(f) >= 4:
                out.append(f[3])
    return out


# Dialog cues hold one line per speaker (the game picks the one for the player's
# side). Each speaker's line is written as a cue of its own: (speaker, the name it
# is written under). Optimus Prime announces for the Autobots under the cue's own
# name, Megatron for the Decepticons (on their maps: Downfall and Oblivion) under
# the name with _MGTRON added.
DIALOG_SPEAKERS = {
    "BL_DX_SWITCHBOARD.SC009019": [
        ("OPRIME", "BL_DX_SWITCHBOARD.SC009019"),
        ("MGTRON", "BL_DX_SWITCHBOARD.SC009019_MGTRON"),
    ],
    "BL_DX_SWITCHBOARD.SC019358": [
        ("OPRIME", "BL_DX_SWITCHBOARD.SC019358"),
        ("MGTRON", "BL_DX_SWITCHBOARD.SC019358_MGTRON"),
    ],
}
# (Escalation's waves' announcer lines: src/waves.rs.)
for _cue in ["SC017781", "SC017783", "SC017785", "SC017787", "SC017789", "SC017791", "SC017793",
             "SC019361", "SC019363"]:
    DIALOG_SPEAKERS["BL_DX_SWITCHBOARD." + _cue] = [
        ("OPRIME", "BL_DX_SWITCHBOARD." + _cue),
        ("MGTRON", "BL_DX_SWITCHBOARD.%s_MGTRON" % _cue),
    ]

# The enemies' dialog cues (tools/build_ai.py's sets) hold a line for each of the
# characters who speak them (a soldier type's two voices, SLDD01 and SLDD02): each
# speaker's line is written as a cue of its own, named so (cue, speaker).
ENEMY_SPEAKER = "%s@%s"

# Layers quieter than this are inaudible in practice and are left out.
SILENT_DB = -60.0
# A layer's speaker levels' properties and their defaults (SoundNodeWaveEvent's):
# the fronts at 0 dB, the centre and the backs silent (the LFE is left out).
PAN_DEFAULTS = (("PanFrontLeft", 0.0), ("PanFrontRight", 0.0), ("PanCenter", -96.0),
                ("PanBackLeft", -96.0), ("PanBackRight", -96.0))

# One-off cues whose layers' volume curves are written (and followed by the demo):
# the Music Volume slider's sample is a long music sting its curve fades out after
# a second and a half. Other one-off cues play their recordings as they are.
FADED_CUES = {"BL_HUD_INTERFACE.MX_SLIDER_VOLUME"}


def obj_list(prop):
    if not (isinstance(prop, tuple) and prop[0] == "array"):
        return []
    r = Reader(prop[1])
    n = r.i32()
    if len(prop[1]) != 4 + 4 * n:
        return []
    return [r.i32() for _ in range(n)]


def num(pr, key, default=0.0):
    v = pr.get(key, default)
    return float(v) if isinstance(v, (int, float)) and not isinstance(v, bool) else default


class Extractor:
    def __init__(self, cooked, more=()):
        self.pkgs = []
        for name in PACKAGES + list(more):
            path = os.path.join(cooked, name)
            if os.path.exists(path):
                print("loading", name)
                pkg = Package(path)
                pkg.path = path
                self.pkgs.append(pkg)
        self.waves = {}  # wave name -> (pkg, export index)

    def find(self, full, cls="SoundCue"):
        for p in self.pkgs:
            # Objects at the top of their own package (BL_HUD_INTERFACE.xxx holds
            # its cues directly) are named without the package.
            own = os.path.splitext(os.path.basename(p.path))[0] + "."
            for i, e in enumerate(p.exports):
                if p.class_name(e) != cls:
                    continue
                name = p.full_name(i + 1)
                if name == full or own + name == full:
                    return p, i + 1
        return None, None

    def props(self, p, idx):
        return props_any(p, p.exports[idx - 1], (4, 0, 8, 12))

    def wave_nodes(self, p, idx, depth=0):
        """All SoundNodeWaveEx exports under a node (through any mixers/randoms),
        as (package, export index)."""
        if idx < 0:
            # A wave in another package (streamed music).
            imp = p.imports[-idx - 1]
            if imp["class"].startswith("SoundNodeWave"):
                q, j = self.find(p.full_name(idx), imp["class"])
                return [(q, j)] if q else []
            return []
        if idx == 0 or depth > 8:
            return []
        e = p.exports[idx - 1]
        if p.class_name(e).startswith("SoundNodeWave") and p.class_name(e) != "SoundNodeWaveEvent":
            return [(p, idx)]
        pr, _ = self.props(p, idx)
        out = []
        for c in obj_list(pr.get("ChildNodes")):
            out += self.wave_nodes(p, c, depth + 1)
        return out

    def speakers(self, full):
        """The dialog characters a dialog cue has lines for (its root's
        SoundNodeWaveEvents' DialogCharacters, by name): none for other cues."""
        p, idx = self.find(full)
        if p is None:
            return set()
        pr, _ = self.props(p, idx)
        root_ref = pr.get("FirstNode")
        root = root_ref[1] if isinstance(root_ref, tuple) else 0
        if root <= 0:
            return set()
        rp, _ = self.props(p, root)
        out = set()
        for child in obj_list(rp.get("ChildNodes")):
            if child <= 0 or p.class_name(p.exports[child - 1]) != "SoundNodeWaveEvent":
                continue
            who = self.props(p, child)[0].get("DialogCharacter")
            if isinstance(who, tuple) and who[0] == "obj" and who[1]:
                out.add(p.full_name(who[1]).split(".")[-1])
        return out

    def cue(self, full, speaker=None):
        """Cue `full` (of a dialog cue, only `speaker`'s line)."""
        p, idx = self.find(full)
        if p is None:
            print("  ! cue not found:", full)
            return None
        pr, _ = self.props(p, idx)
        # (A property left at its class's default isn't in the package: the engine's
        # defaults, SoundCue's and High Moon's SoundNodeRoot's, stand for those
        # missing: 5 at once, the farthest cut off; -6 dB, heard full out to 4 m and
        # up to 64 m. Taken for 16, 0 dB, 35 m and 350 m, the cues that left them so,
        # the big robots' footsteps among them, played 6 dB too loud, over three
        # times as many at once and five times as far.)
        max_conc = int(num(pr, "MaxConcurrentPlayCount", 5)) or 5
        limiting = {"kKillOldest": "oldest", "kKillNewest": "newest"}.get(
            str(pr.get("InstanceLimiting", "kKillFarthest")), "farthest")
        root_ref = pr.get("FirstNode")
        root = root_ref[1] if isinstance(root_ref, tuple) else 0
        rp, _ = self.props(p, root)
        # Its mixer category (a SoundMixerCategoryName object under the root).
        category = None
        cat_ref = rp.get("Category")
        if isinstance(cat_ref, tuple) and cat_ref[0] == "obj" and cat_ref[1] > 0:
            category = self.props(p, cat_ref[1])[0].get("CategoryName")
        # The mixer preset it turns on as it plays (its root's SoundMixerPreset, an
        # HmSoundMixerPreset of SND_SoundConfig_p's, by name), and whether only the
        # local player's own copy does (PlayMixerPresetOnlyIfLocalPlayerSound).
        preset = None
        preset_ref = rp.get("SoundMixerPreset")
        if isinstance(preset_ref, tuple) and preset_ref[0] == "obj" and preset_ref[1] != 0:
            preset = p.full_name(preset_ref[1]).split(".")[-1]
        # Whether its layers' own VolumeCurves (not their Envelope's) follow the
        # distance from the camera (SoundParameter SOUND_DISTANCE): their keys are
        # distances, in Unreal units. (Other parameters, a vehicle's speed, a
        # sniper's spread, are driven by the demo's code where it has them.)
        param = rp.get("SoundParameter")
        by_distance = (isinstance(param, tuple) and param[0] == "obj" and param[1] != 0
                       and p.full_name(param[1]).split(".")[-1] == "SOUND_DISTANCE")
        # How it is heard: placed round the player (k3D, the default), flat (k2D),
        # or flat while it is the player's own (the smart pans).
        root_spatial = str(rp.get("SpatializationType", "k3D"))
        cue = {
            "category": category,
            "preset": preset,
            "preset_local": bool(rp.get("PlayMixerPresetOnlyIfLocalPlayerSound", False)),
            "vol": num(rp, "Volume", -6.0), "pitch": num(rp, "Pitch"),
            "pv": tuple(sorted((num(rp, "PitchVariationMin"), num(rp, "PitchVariationMax")))),
            "vv": (num(rp, "VolumeVariationMin"), num(rp, "VolumeVariationMax")),
            "max": max_conc,
            "limiting": limiting,
            "dist": (num(rp, "DistanceMin", 400.0) * 0.01, num(rp, "DistanceMax", 6400.0) * 0.01),
            "rolloff": num(rp, "RolloffFactor", 1.0),
            "layers": [],
            "loop_start": num(rp, "LoopStart"),
            "loop_end": num(rp, "LoopEnd"),
        }
        for child in obj_list(rp.get("ChildNodes")):
            ce = p.exports[child - 1] if child > 0 else None
            if ce is None:
                continue
            lp, _ = self.props(p, child)
            if p.class_name(ce) == "SoundNodeWaveEvent":
                who = lp.get("DialogCharacter")
                if speaker and isinstance(who, tuple) and not p.full_name(who[1]).endswith("." + speaker):
                    continue
                waves = self.wave_nodes(p, child)
            else:
                waves = self.wave_nodes(p, child)
                lp = {}
            layer = {
                "time": num(lp, "Time"), "vol": num(lp, "Volume"), "pitch": num(lp, "Pitch"),
                "pv": tuple(sorted((num(lp, "PitchVariationMin"), num(lp, "PitchVariationMax")))),
                "vv": (num(lp, "VolumeVariationMin"), num(lp, "VolumeVariationMax")),
                "loop": bool(lp.get("bLooping", False)),
                "chance_none": num(lp, "ChanceToPlayNone"),
                "delay": (num(lp, "DelayMin"), num(lp, "DelayMax")),
                "waves": [],
            }
            if layer["vol"] <= SILENT_DB:
                continue
            # Its speaker levels (a plain sound's: both fronts at 0 dB, the rest
            # silent, SoundNodeWaveEvent's defaults), where it is heard flat.
            spatial = (str(lp.get("SpatializationType", "k3D"))
                       if lp.get("OverrideSpatialization", False) else root_spatial)
            pans = tuple(num(lp, k, d) for k, d in PAN_DEFAULTS)
            if spatial != "k3D" and pans != tuple(d for _, d in PAN_DEFAULTS):
                layer["pan"] = (pans, "2d" if spatial == "k2D" else "smart")
            env = lp.get("Envelope")
            if isinstance(env, tuple) and env[0] == "struct":
                curves = envelope(p, env[2])
                layer["envelope"] = curves.get("VolumeCurve", [])
                layer["pitch_curve"] = curves.get("PitchCurve", [])
            if by_distance:
                keys = curve_keys(lp.get("VolumeCurve"))
                if keys:
                    layer["distance"] = [(d * 0.01, v) for d, v in keys]
            for wp, w in waves:
                name = wp.exports[w - 1]["name"]
                self.waves.setdefault(name, (wp, w))
                layer["waves"].append(name)
            if layer["waves"]:
                cue["layers"].append(layer)
        return cue

    def streamed(self, name):
        """The copy of a wave that holds its audio, from any loaded package."""
        for p in self.pkgs:
            for i, e in enumerate(p.exports):
                if e["name"] == name and p.class_name(e).startswith("SoundNodeWave") and self.fsb(p, i + 1):
                    return p, i + 1
        return None, None

    def duration(self, p, idx):
        try:
            pr, _ = self.props(p, idx)
            return num(pr, "Duration") or None
        except ValueError:
            return None

    def fsb(self, p, idx):
        e = p.exports[idx - 1]
        data = p.data[e["offset"]: e["offset"] + e["size"]]
        at = data.find(b"FSB4")
        if at < 0:
            return None
        n, shdr, dsize = struct.unpack_from("<iii", data, at + 4)
        bank = data[at: at + 48 + shdr + dsize]
        return bank, n, shdr, dsize


def envelope(pkg, data):
    """An HmSoundParameterCurve's curves: {"VolumeCurve": [(time, volume) keys],
    "PitchCurve": [(time, semitones) keys]}, each there if the layer has it (none if
    it can't be read). The struct is a property list; each curve an array of keys."""
    try:
        pr = read_props(pkg, Reader(data), len(data))
    except (ValueError, struct.error, IndexError):
        return {}
    out = {}
    for name in ("VolumeCurve", "PitchCurve"):
        keys = curve_keys(pr.get(name))
        if keys:
            out[name] = keys
    return out


def curve_keys(v):
    """A curve property's (array of (in, out) float pairs) keys, or none."""
    if not (isinstance(v, tuple) and v[0] == "array"):
        return []
    raw = v[1]
    if len(raw) < 4:
        return []
    n = struct.unpack_from("<i", raw, 0)[0]
    if 0 < n <= 32 and len(raw) >= 4 + 8 * n:
        return [struct.unpack_from("<ff", raw, 4 + 8 * i) for i in range(n)]
    return []


def mpeg_frames(audio):
    """The MPEG audio frames in an FSB bank's data (FMOD pads them to 16 bytes)."""
    rates = {2: (0, 32, 48, 56, 64, 80, 96, 112, 128, 160, 192, 224, 256, 320, 384, 0),  # layer II
             1: (0, 32, 40, 48, 56, 64, 80, 96, 112, 128, 160, 192, 224, 256, 320, 0)}  # layer III
    hz = (44100, 48000, 32000, 0)

    def length(j):
        if audio[j] != 0xFF or (audio[j + 1] & 0xE0) != 0xE0 or (audio[j + 1] >> 3) & 3 != 3:
            return 0
        layer = (audio[j + 1] >> 1) & 3
        br, sr = audio[j + 2] >> 4, (audio[j + 2] >> 2) & 3
        if layer not in rates or not rates[layer][br] or not hz[sr]:
            return 0
        return 144 * rates[layer][br] * 1000 // hz[sr] + ((audio[j + 2] >> 1) & 1)

    frames, j = [], 0
    while j < len(audio) - 4:
        n = length(j)
        if n:
            frames.append(audio[j:j + n])
            j += n
        else:
            j += 1
    return frames


def decode(bank, shdr, dsize, out_path, duration=None):
    """FSB4 with one MPEG sample: the audio is the frames after the headers. The
    MPEG data is padded to whole frames, so it is cut to the wave's real length
    (which keeps looping sounds seamless). Music with more than two channels is
    stored as stereo streams whose frames take turns (front, centre/LFE, rear for
    5.1); those are separated and mixed down to stereo."""
    mode = struct.unpack_from("<I", bank, 48 + 0x40 + 8)[0] if len(bank) > 48 + 0x50 else 0
    channels = struct.unpack_from("<H", bank, 48 + 62)[0] if len(bank) > 48 + 64 else 2
    audio = bank[48 + shdr: 48 + shdr + dsize]
    streams = [audio]
    if channels > 2:
        frames = mpeg_frames(audio)
        n = (channels + 1) // 2
        streams = [b"".join(frames[i::n]) for i in range(n)]
    tmps = []
    for data in streams:
        with tempfile.NamedTemporaryFile(suffix=".mp3", delete=False) as f:
            f.write(data)
            tmps.append(f.name)
    try:
        cut = ["-t", "%.4f" % duration] if duration else []
        inputs = []
        for t in tmps:
            inputs += ["-i", t]
        mix = []
        if len(tmps) == 3:
            # FL FR | FC LFE | SL SR -> stereo (the game's 5.1 music, folded down).
            mix = ["-filter_complex",
                   "[0:a][1:a][2:a]amerge=inputs=3,"
                   "pan=stereo|c0=0.8*c0+0.57*c2+0.57*c4|c1=0.8*c1+0.57*c2+0.57*c5"]
        r = subprocess.run(["ffmpeg", "-v", "error", "-y"] + inputs + mix + cut +
                           ["-c:a", "libvorbis", "-q:a", "5", out_path], capture_output=True)
        if r.returncode != 0:
            print("  ! ffmpeg failed for", os.path.basename(out_path), r.stderr.decode()[:200], hex(mode))
            return False
        return True
    finally:
        for t in tmps:
            os.unlink(t)


def voices(ex, cooked):
    """The voice sets' lines for voices.txt (see VOICE_SETS), and their cues."""
    pkgs = list(ex.pkgs)
    for name in VOICE_PACKAGES:
        path = os.path.join(cooked, name)
        if os.path.exists(path):
            pkg = Package(path)
            pkg.path = path
            pkgs.append(pkg)

    def obj(full, cls):
        """An object's properties, from the first package with them (a package
        that only refers to it, or holds an empty copy, is passed over)."""
        for p in pkgs:
            for i, e in enumerate(p.exports):
                if p.class_name(e) == cls and p.full_name(i + 1) == full:
                    try:
                        pr, _ = props_any(p, e, (4, 0, 8, 12))
                    except ValueError:
                        continue
                    if pr:
                        return p, pr
        return None, {}

    def ref(p, v):
        return p.full_name(v[1]) if isinstance(v, tuple) and v[0] == "obj" and v[1] else None

    lines = ["# Made by tools/build_sounds.py: the multiplayer's robot voices. See that file for the format."]
    cues = []
    events = {}
    for name in VOICE_SETS:
        p, pr = obj("SoundEvents.Multiplayer." + name, "HmSoundEventSet")
        v = pr.get("SoundEventSet")
        if not (isinstance(v, tuple) and v[0] == "array"):
            print("  ! no voice set", name)
            continue
        data = v[1]
        r = Reader(data, 4)
        for _ in range(struct.unpack_from("<i", data, 0)[0]):
            ev = read_props(p, r, len(data))
            event, cue = ref(p, ev.get("Event")), ref(p, ev.get("Sound"))
            if event and cue:
                event = event.split(".")[-1]
                lines.append("voice %s %s %s" % (name, event, cue))
                cues.append(cue)
                events[event] = None
    groups = {}
    for event in events:
        p, pr = obj("SoundEvents_Dialog." + event, "HmSoundEvent")
        group = ref(p, pr.get("DialogGroup")) if p else None
        priority = num(pr, "DialogPriority")
        lines.append("event %s %s %g" % (event, group.split(".")[-1] if group else "-", priority))
        while group and group not in groups:
            gp, gpr = obj(group, "HmSoundEventDialogGroup")
            parent = ref(gp, gpr.get("ParentDialogGroup")) if gp else None
            groups[group] = (parent, int(num(gpr, "InitialChanceToPlay", 100)),
                             num(gpr, "ReTriggerDelayMin"), num(gpr, "ReTriggerDelayMax"),
                             int(num(gpr, "InstanceLimit", 1)))
            group = parent
    for group, (parent, chance, lo, hi, limit) in groups.items():
        lines.append("group %s %s %d %g %g %d" % (
            group.split(".")[-1], parent.split(".")[-1] if parent else "-", chance, lo, hi, limit))
    return lines, cues


# The game's mixer: its categories and presets (HmSoundMixerPreset), all in
# HM_Engine.xxx (SND_SoundConfig_p.SoundMixerProperties and the presets' own
# objects, which hold their fades; SND_SoundConfig_p.xxx itself is a stub).
MIXER_PACKAGE = "HM_Engine.xxx"
MIXER_PROPERTIES = "SND_SoundConfig_p.SoundMixerProperties"


def _struct_array(pkg, v):
    """An array of structs' property lists."""
    if not (isinstance(v, tuple) and v[0] == "array") or len(v[1]) < 4:
        return []
    r = Reader(v[1])
    out = []
    for _ in range(r.i32()):
        try:
            out.append(read_props(pkg, r, len(v[1])))
        except Exception:
            break
    return out


def _struct(pkg, v):
    return read_props(pkg, Reader(v[2]), len(v[2])) if isinstance(v, tuple) and v[0] == "struct" else {}


def mixer_setup(cooked):
    """The game's mixer categories, parents first: [(name, parent or None)]; its
    presets' volumes: {preset: {category: linear volume}} (only the volumes that
    aren't 1: a preset also sets the categories' filters, reverbs and pitch, which
    the demo leaves out); its presets' fades: {preset: (fade in, fade out, duration,
    props)}; and the presets categories turn on with many of their sounds playing:
    [(category, count, seconds, preset)] (ChannelCountSoundMixerPreset)."""
    pkg = Package(os.path.join(cooked, MIXER_PACKAGE))
    idx = next(i for i, e in enumerate(pkg.exports, 1) if pkg.full_name(i) == MIXER_PROPERTIES)
    pr = props_any(pkg, pkg.exports[idx - 1], (4, 0, 8, 12))[0]
    cats = _struct_array(pkg, pr.get("SoundCategories"))
    children = {}
    for c in cats:
        d = c.get("ChildCategories", ("array", b"\0\0\0\0"))[1]
        n = struct.unpack_from("<i", d, 0)[0]
        children[c["Name"]] = [pkg.names[struct.unpack_from("<i", d, 4 + 8 * k)[0]] for k in range(n)]
    parent = {kid: name for name, kids in children.items() for kid in kids}
    order = []

    def walk(name):
        order.append((name, parent.get(name)))
        for kid in children.get(name, []):
            walk(kid)
    for c in cats:
        if c["Name"] not in parent:
            walk(c["Name"])
    volumes, channels = {}, []
    for c in cats:
        for s in _struct_array(pkg, c.get("DSPPresets")):
            ref = s.get("SoundMixerPreset")
            if not (isinstance(ref, tuple) and ref[1]):
                continue
            vol = _struct(pkg, s.get("Volume")).get("Volume", 1.0)
            if abs(vol - 1.0) > 1e-4:
                volumes.setdefault(pkg.obj_name(ref[1]), {})[c["Name"]] = vol
        ref = c.get("ChannelCountSoundMixerPreset")
        if isinstance(ref, tuple) and ref[1]:
            channels.append((c["Name"], int(c.get("ChannelCountMixerPresetThreshold", 0)),
                             float(c.get("ChannelCountMixerPresetTime", 0.0)), pkg.obj_name(ref[1])))
    fades = {}
    for i, e in enumerate(pkg.exports, 1):
        if pkg.class_name(e) == "HmSoundMixerPreset" and pkg.full_name(i).startswith("SND_SoundConfig_p."):
            pp = props_any(pkg, e, (4, 0, 8, 12))[0]
            fades[e["name"]] = (num(pp, "FadeInTime"), num(pp, "FadeOutTime"), num(pp, "Duration"), pp)
    return order, volumes, fades, channels


def decibels(v):
    """A linear volume in dB (-100 for silence)."""
    return round(20 * math.log10(v), 2) if v > 1e-5 else -100


def mixer_lines(cooked, cues):
    """sounds.txt's mixer lines (`mixcategory`, `preset`, `channels`) for the
    presets `cues` turn on and those their categories do: those that change a
    volume."""
    order, volumes, fades, channels = mixer_setup(cooked)
    used = {c["category"] for c in cues.values() if c["category"]}
    channels = [ch for ch in channels if ch[0] in used and ch[3] in volumes]
    presets = sorted({c["preset"] for c in cues.values() if c["preset"] in volumes} |
                     {ch[3] for ch in channels})
    lines = ["mixcategory %s %s" % (name, up or "-") for name, up in order]
    for name in presets:
        fade_in, fade_out, duration, pp = fades.get(name, (0.0, 0.0, 0.0, {}))
        for odd in ("DelayMin", "DelayMax", "OnlyForPlayerCamera", "Priority"):
            if pp.get(odd):
                print("  ! mixer preset %s has %s %s (left out)" % (name, odd, pp[odd]))
        lines.append("preset %s %g %g %g %s" % (name, round(fade_in, 4), round(fade_out, 4), round(duration, 4),
                                                " ".join("%s %g" % (cat, decibels(v))
                                                         for cat, v in sorted(volumes[name].items()))))
    for cat, count, secs, name in channels:
        lines.append("channels %s %d %g %s" % (cat, count, secs, name))
    return lines, set(presets)


def main():
    cooked, assets = sys.argv[1], sys.argv[2]
    out_dir = os.path.join(assets, "sounds")
    os.makedirs(out_dir, exist_ok=True)
    enemy = enemy_cues(assets)
    # (The enemies' dialog lines are streamed from their speakers' own packages,
    # WL_DX_<speaker>.xxx, as the multiplayer voices' are.)
    speakers = sorted({s for v in enemy.values() for s in v})
    ex = Extractor(cooked, ["WL_DX_%s.xxx" % s for s in speakers])
    lines = ["# Made by tools/build_sounds.py from the game's SoundCues. See that file for the format."]
    voice_lines, voice_cues = voices(ex, cooked)
    cues = {}
    # (The enemies' dialog cues, as each of their speakers' lines: ENEMY_SPEAKER.)
    enemy_dialog = []
    plain = set(CUES + character_cues(assets) + voice_cues + death_cues(assets))
    for full in dict.fromkeys(CUES + character_cues(assets) + voice_cues + sorted(enemy)):
        speakers = DIALOG_SPEAKERS.get(full)
        if speakers is None and enemy.get(full):
            have = ex.speakers(full)
            if have:
                speakers = [(s, ENEMY_SPEAKER % (full, s)) for s in sorted(enemy[full]) if s in have]
                enemy_dialog += [name for _, name in speakers]
                # (One the demo plays by its own name too: all its lines, as well.)
                if full in plain:
                    speakers.append((None, full))
        for speaker, name in speakers or [(None, full)]:
            c = ex.cue(full, speaker)
            if c:
                cues[name] = c
    ok_waves = set()
    for name, (p, idx) in sorted(ex.waves.items()):
        out = os.path.join(out_dir, name + ".ogg")
        if os.path.exists(out) and os.path.getsize(out) > 0:
            ok_waves.add(name)
            continue
        got = ex.fsb(p, idx)
        if not got:
            # A streamed wave (music): the cue's package holds only a stub, the
            # audio is in the wave's own package.
            p, idx = ex.streamed(name)
            got = ex.fsb(p, idx) if p else None
        if not got:
            print("  ! no FSB bank in", name)
            continue
        bank, n, shdr, dsize = got
        if decode(bank, shdr, dsize, out, ex.duration(p, idx)):
            ok_waves.add(name)
    # The game's mixer: its categories, and the presets the cues turn on (their
    # volume dips: an announcer's line turns the effects and music down 9 dB).
    mix, presets = mixer_lines(cooked, cues)
    lines += mix
    for full, c in cues.items():
        layers = []
        for l in c["layers"]:
            waves = [w for w in l["waves"] if w in ok_waves]
            if waves:
                layers.append((l, waves))
        if not layers:
            continue
        lines.append("cue %s %g %g %g %g %g %g %d %g %g %g %s" % (
            full, c["vol"], c["pitch"], c["pv"][0], c["pv"][1], c["vv"][0], c["vv"][1], c["max"],
            c["dist"][0], c["dist"][1], c["rolloff"], c["limiting"]))
        if c["category"]:
            lines.append("category %s" % c["category"])
        if c["preset"] in presets:
            lines.append("mixer %s%s" % (c["preset"], " local" if c["preset_local"] else ""))
        for l, waves in layers:
            lines.append("layer %g %g %g %g %g %g %g %d %s" % (
                l["time"], l["vol"], l["pitch"], l["pv"][0], l["pv"][1], l["vv"][0], l["vv"][1],
                int(l["loop"]), " ".join(waves)))
        # Music and other looping cues: the layers' volume and pitch curves, the
        # layers that play only now and then, and where the cue loops; the one-off
        # cues: their layers' volume curves that shape them (the game plays them
        # through them: the AI heavy guns' shots cut to 0.4 s, a jump's servo
        # fading in), and FADED_CUES' whatever they are.
        music = any(l["loop"] for l, _ in layers)
        for i, (l, waves) in enumerate(layers):
            env = l.get("envelope")
            shapes = env and not all(abs(v - 1.0) < 1e-3 for _, v in env)
            if env and (music or full in FADED_CUES or shapes):
                lines.append("envelope %d %s" % (i, " ".join("%g %g" % k for k in env)))
            if music:
                if l.get("pitch_curve"):
                    lines.append("pitch %d %s" % (i, " ".join("%g %g" % k for k in l["pitch_curve"])))
                if l["chance_none"] > 0 or l["delay"][1] > 0:
                    lines.append("random %d %g %g %g" % (i, l["chance_none"], l["delay"][0],
                                                         max(l["delay"])))
            if l.get("distance"):
                lines.append("distance %d %s" % (i, " ".join("%g %g" % k for k in l["distance"])))
            if l.get("pan"):
                pans, how = l["pan"]
                lines.append("pan %d %s %s" % (i, " ".join("%g" % v for v in pans), how))
        if music and c["loop_end"] > 0:
            if 0 < c["loop_start"] < c["loop_end"]:
                lines.append("loop_start %g" % c["loop_start"])
            lines.append("loop_end %g" % c["loop_end"])
    with open(os.path.join(out_dir, "sounds.txt"), "w") as f:
        f.write("\n".join(lines) + "\n")
    # Each voice cue's longest line (its waves' Duration): how long a character
    # speaking it is busy (voice.rs says one line at a time).
    for full in dict.fromkeys(voice_cues + enemy_dialog):
        c = cues.get(full)
        if not c:
            continue
        secs = [ex.duration(*ex.waves[w]) or 0.0 for l in c["layers"] for w in l["waves"] if w in ex.waves]
        if secs:
            voice_lines.append("length %s %.3f" % (full, max(secs)))
    with open(os.path.join(out_dir, "voices.txt"), "w") as f:
        f.write("\n".join(voice_lines) + "\n")
    print("wrote %d cues, %d waves to %s" % (len(cues), len(ok_waves), out_dir))


if __name__ == "__main__":
    main()
