"""Build one class's characters for the demo: every armor set the game offers it in
multiplayer, its bonus heads and the wheels it can fit, with each chest's vehicle,
the transformations, the parts' own animations and the class's own way of moving.

  infiltrator  (the game's Scout): Bumblebee, Breakdown, Sideswipe, Deadend, Jazz,
               Dragstrip and the DLC Wheeljack and Snarl, in cars
  destroyer    (Leader): Swindle, Optimus Prime, Ratchet, Ironhide, Soundwave,
               Onslaught and the DLC Zeta Prime, G1 Optimus, Ultra Magnus, Hound
               and Slug, in trucks
  titan        (Soldier): Megatron, Warpath, Brawl, Scattershot, Demolisher, Quake
               and the DLC Perceptor, Hardshell and Grimlock, in hover tanks (and
               Grimlock's walking dino), their sixth part a cannon
  scientist    (Scientist): Jetfire, Starscream, Air Raid, Vortex, Silverbolt,
               Shockwave and the DLC Blastoff, Sharpshot, Kickback and Swoop, in jets
               (Vortex's helicopter, the Insecticons' beetles, Swoop's pterodactyl),
               their sixth part the wings

How the game builds a character (class_data.py reads it from the game's data):
every part of every class is a mesh on the one robot skeleton (TR_Skeletons_p.
RB_Iconic_SKEL) that robot.glb already has with all of Jazz's clips, so a part is
drawn by skinning its mesh to robot.glb's bones; the wheels hang on the parts'
"Wheel" sockets; the chest decides the vehicle and the transformation's robot half;
the forearms and upper arms add their own animation sets (the forearm pieces
folding away as the gun moves to the other arm, the shoulder pads flipping as a gun
comes out). A class's robot is drawn at its own size (its robot definition's
RobotMeshMergeScale: the Infiltrator 0.6, the Destroyer 0.8) and moves with its own
animation sets where they differ from the Infiltrator's (a later set of its
definition's AnimSets overriding an earlier one, as UE3's FindAnimSequence looks
them up): the Destroyer's jog, sprint, dashes, jump and fine-aim walk are
Shared_Truck_ROBO_ANIM's heavy ones, its melee its melee set's sword swing
(Destroyer_ROBO_ANIM's Melee_Sword_Destroyer). Its camera is its camera set's on
foot (Truck_MP_CAMSET's) and in each vehicle Escalation's (class_data.
vehicle_camera_set: Truck_Optimus_CAMSET's strategies driving and hovering for
Optimus Prime's truck) and its vehicles drive
by the physics blueprints Escalation gives their forms (SKILL_INDEX: Truck_Physics,
HoverTruck_Physics..., not the multiplayer matches' Truck_MP and HoverTruck_MP).
The Titan's tanks only hover (TnHoverTankSimulation, HoverTank_Physics): no car
guns on them, their cannon's reload in the guns' place among the clips, their
cannon bone, muzzle and effect sockets written for src/car.rs.
The Destroyer's trucks carry rocket launchers on the weapon sockets in the car
guns' place (the class's vehicle weapon, class_data.py's vehicle_weapons:
RocketVehicle; ROCKET_LAUNCHER), their shots leaving from the launchers' muzzles.
The Scientist's jets (TnPlaneForm) hover (TnHoverPlaneSimulation, HoverPlane_
Physics) and fly as they boost (TnPlaneSimulation, Plane_Physics), with no
gravity either way: VEH_Plane_ANIMTREE's clips (their own spellings of the hover
transitions, and four more: the banking turns, the climb and the crash), no car
guns (their homing rockets leave from the weapon sockets), their effect sockets
as the tanks' and their flight written for src/car.rs and main.rs.
This writes, under <assets>/models/<class>:

  parts.glb         every part mesh (skinned to the robot skeleton, by bone name)
                    and every wheel mesh (a jet's wings two: the robot's, standing
                    up on its back, and the jet's own), their materials on the
                    textures below
  clips.glb         the robot's clips the parts bring, on robot.glb's skeleton (the
                    same node names from its Root, so they drive its bones):
                    each vehicle's Transform_ToVehicle_ROBO / ToRobot_ROBO, each
                    forearm set's ADD_Weapon_MirrorReverse_ARM, each shoulder set's
                    ADD_Weapon_Arm_Shoulder_Mirror_ARM and _EquipUnequip_ARM
                    (additive), in the order <class>.txt gives
  clips_mirror.glb  the same clips mirrored left for right (mirror_anims.py), for the
                    gun in the left hand
  robot_clips.glb   the class's own versions of robot.glb's clips (for the
                    Infiltrator, whose they are, only its melee: its melee set's
                    sword swing for Jazz's shield bash) and its own idle holding a gun
                    for the Customize preview, and robot_clips_mirror.glb mirrored
  vehicle_<name>.glb each chest's vehicle: its mesh, the car guns on its weapon
                    sockets (a truck's rocket launchers; none on a tank or a jet)
                    and its clips in car.rs's order (vehicle_clips)
  textures/*.ktx2   the texture sets, block-compressed with mips (ktx2_io.py): the
                    colour with the paint mask in its alpha (the game's
                    Masks_NormY blue: 0.5 unpainted, over it the primary colour,
                    under it the secondary), normals, metal-roughness, glow
                    (the part's own light in red, its energon in green: see
                    GLOW_COVERAGE)
  <class>.txt       the class, its parts, sets, sockets, vehicles and timings (below)

The game paints the parts with the build's colours (Cust_Color_A, Cust_COLOR_B):
colour * primary where the mask is over 0.5, * secondary under it, each weighted
by how far it is from 0.5 (src/character.rs does the same with
assets/shaders/paint.wgsl). Materials painted that way have the extra
{"paint": true}.

usage: python build_class.py <class> <umodel export dir> <CookedPC> <TransGame> <foc_esc/assets>
       python build_class.py --umodel <class> <CookedPC> <TransGame> [<umodel export dir>]
           prints the umodel commands below, object lists and all (the textures once
           the meshes are exported to that folder)
       python build_class.py --glow <class> <CookedPC> <foc_esc/assets>
           gives a class already built its glow (see GLOW_COVERAGE) straight from
           CookedPC: rewrites its glow textures and points the materials that
           glow at theirs
       python build_class.py --drive <class> <CookedPC> <TransGame> <foc_esc/assets>
           brings a class already built up to date with how its cars and trucks
           drive and how the camera follows them (drive_pass): its vhandling,
           vplane, vdrive, vcam, vground and vcamera lines
       python build_class.py --ground <class> <CookedPC> <TransGame> <foc_esc/assets>
           gives a class already built how its cars', trucks' and tanks' bodies
           follow the ground (ground_pass): its vground lines
       python build_class.py --fx <class> <CookedPC> <TransGame> <foc_esc/assets>
           brings a class already built up to date with its vehicles' effects
           (fx_pass): its vfx and vfxland lines
       python build_class.py --sounds <class> <CookedPC> <TransGame> <foc_esc/assets>
           brings a class already built up to date with its vehicles'
           transformations' footsteps, clip sounds and the events their audio
           components play (sounds_pass): its vtstep, vcsound and vaudio lines
       python build_class.py --decals <class> <CookedPC> <TransGame> <foc_esc/assets>
           brings a class already built up to date with its vehicles' decal sockets
           (decals_pass): its vdecal lines
       python build_class.py --parts <class> <umodel export dir> <CookedPC> <TransGame> <foc_esc/assets>
           brings a class already built up to date with its parts (parts_pass): its
           parts.glb and textures, and its part lines (then --glow, for the glow
           textures that pass makes)
       python build_class.py --robotclips <class> <umodel export dir> <CookedPC> <TransGame> <foc_esc/assets>
           brings a class already built up to date with robot.glb's clips and its own
           melee (robot_clips_pass): its robot_clips.glb (and mirrored), its robotclip,
           rcsound and custpose lines and its class line's melee seconds
       python build_class.py --interiors <class> <CookedPC> <foc_esc/assets>
           gives a class already built its vehicles' insides (interiors_pass): the
           materials its vehicle models drew as glass for want of a texture set
           (VH_Interior_MAT, a name several of the game's materials share, and the
           insides with pictures or plain materials) made again from CookedPC
       python build_class.py --transforms <class> <foc_esc/assets>
           brings a class already built up to date with where its cars are drawn as
           they change form (transforms_pass), from its clips.glb and vehicle models:
           its vtransform lines
(build_infiltrator.py is this for the Infiltrator.)

<umodel export dir> holds umodel's exports, a folder per package as umodel writes
them (meshes and textures as glTF and PNG, animation sets as .psa):
  umodel -game=trans -export -gltf -path=<CookedPC> -out=<dir> MP_Array_BASE_m.xxx -obj=<mesh>...
      every part, wheel and vehicle mesh class_data.py lists, and
      WEP_AssaultRifleVH_SKEL (the car guns)
  umodel -game=trans -export -gltf -path=<CookedPC> -out=<dir> mp_shared.xxx -obj=WEP_RocketLauncherVH_SKEL
      the trucks' rocket launcher (the Destroyer's only; its animation set,
      WEP_RocketLauncherVH_ANIM, with the shared ones below)
  umodel -game=trans -export -png -path=<CookedPC> -out=<dir> MP_Array_BASE_m.xxx -obj=<texture>...
      each texture set's <set>_Color_NormX and _Masks_NormY (the full-size mips
      come from the TR_*_ROBO_p / _VEH_p packages next to it, which CookedPC needs);
      sets that aren't cooked into the map (the Infiltrator's
      RB_Bumblebee_ArmLo_TEXSET, from TR_BumbleBee_ROBO_p.xxx...) from their
      character's own package
  umodel -game=trans -export -path=<CookedPC> -out=<dir> MP_Array_BASE_m.xxx -obj=<anim set>...
      each set the parts and vehicles name and the class's own (Destroyer_ROBO_ANIM;
      the Infiltrator's only its melee set, Infiltrator_ROBO_ANIM), and those of
      mp_shared.xxx (Jazz_ROBO_ANIM_NEW, Shared_Truck_ROBO_ANIM)
  umodel -game=trans -export -png -path=<CookedPC> -out=<dir> WEP_AssaultRifleArm_p.xxx
      WEP_ArmUp_TEXSET (the gun arm robot.glb's weapons have, painted too)
The guns' texture sets (robot.glb's weapons and the car guns, painted too) are read
straight from <CookedPC> (all_shared.xxx and the weapons' WEP_*_p.xxx); without
them the car guns' is taken from models/vehicle.glb, unpainted.

<class>.txt (tab-separated):
  class    <name> <robot scale> <half height> <radius> <jog stride> <sprint stride> <aim stride>
           <take-off secs> <melee secs>   (the robot's size: its RobotMeshMergeScale and
           collision, Unreal units; metres it covers per footstep jogging, sprinting
           and in fine aim, at the demo's speeds; its jump's Nav_TakeOff_01 and its
           melee clip, seconds)
  camera   <orbit> <pitch min> <pitch max> <ahead side up>x3 <scope side up back>   (the
           class's camera on foot, metres and degrees: camera_line; none for the
           Infiltrator, whose camera is the demo's own)
  vweapon  <weapon>   (the weapon its vehicles carry, TransWeapons.ini's unique id:
           AssaultRifleVehicle, RocketVehicle, TankCannon or HomingRocketVehicle)
  csound   <event> <cue>   (the robot's sounds: its sound event set's footsteps, jumps,
           landings and body falls, FS_DEFAULT_RUN...)
  robotclip <robot.glb clip> <robot_clips.glb clip>   (a clip of robot.glb's the class
           has its own of: main.rs's ANIM_* numbers)
  rcsound  <robot.glb clip> <time> <cue>   (a sound one of those plays: its melee's swing)
  custpose weapon <robot_clips.glb clip>   (the class's own idle holding a gun, for the
           Customize preview: its Nav_Idle_Shoot, where its sets have one of their own)
  part     <id> <type> <name> <dlc 0/1> <meshes,> <wheel mesh|-> <vehicle|-> <arm set|-> <shoulder set|-> <disabled types,|->
           <attachment socket|-> [<vehicle's wheel mesh|->]   (the sixth part (Wheels, Cannon, Wings) hangs its
           static mesh on every socket whose name starts with its AttachmentSocket: Wheel, Cannon,
           WingSocket; the last is the vehicle's where it has one of its own, part_line: the jet's
           wings, the robot's standing up on its back)
  set      <id> <name> <dlc 0/1> <part ids,>
  socket   <mesh> <socket> <bone> <tx ty tz> <qx qy qz qw> <sx sy sz>   (glTF axes, metres before scaling)
  vehicle  <form> <model> <clip to vehicle> <clip to robot> <robot hides> <robot clip secs> <robot shows> <robot unfold secs>
           <car shows> <car fold secs> <car hides> <car unfold secs> <wheel radius> <height> <legs 0/1>
           <lift>   (how much higher a walker's model is drawn, to stand on its feet: negative lowers it)
  vtransform <form> <folding up lift> <unfolding lift>   (how much higher than its clips put it
           the car is drawn while it and the robot both show, among the robot's pieces it
           comes out of or goes into, metres: transform_fit.py, src/car.rs's transform_heights)
  vhandling <form> <top speed> <acceleration> <jump up> <jump ahead> <jump nose-up> <jump gravity>
           <hover speed> <hover acceleration> <hover jump> <hover jump nose-up> <drift secs>
           <hover jump gravity> <hover height> <wheels' drop>   (how it drives: its car physics'
           MaxSpeed, MaxAcceleration, JumpLinearVelocity, JumpAngularVelocity, JumpingGravityScale,
           its hover's MaxLinearSpeed, MaxLinearAcceleration, JumpLinearSpeed, JumpAngularSpeed,
           DriftDuration and JumpingGravityScale, how much higher it floats hovering than on its
           wheels and how far its wheels drop in the air; metres and seconds: handling_line)
  vbody    <form> <half width> <reach ahead> <reach behind>   (its body against walls, metres)
  vmiddle  <form> <up> <ahead>   (its middle, where the game's pawn is and its cameras anchor:
           how high over the ground it stands on and how far ahead of its origin, metres:
           vehicle_middle)
  vhull    <form> <ahead> <behind> <half width> <bottom> <top>   (its rigid body's box: hull_line)
  vdrive   <form> <angular damping> <front axle ahead> <back axle behind> <yaw gyration²>
           <max steering> <front grip> <back grip> <bank> <bank speed> <roll pull> <roll damping>
           <air turn> <air strafe> <jump shake x4> <landing shake x4>   (a car's or truck's
           driving on its wheels: drive_line)
  vground  <form> <wheel across ahead>x4 <pitch pull> <pitch damping> <rise pull> <rise damping>
           <hover radius> <hover centre across ahead> <hover pitch pull damping> <hover roll pull damping>
           <hover rise pull damping> <hover roll hold>   (a car's, truck's or tank's body on its
           springs over the ground: ground_line; metres, per second squared, per second, a share a frame)
  vcam     <form> <drive|hover> <property> <states> <values...>   (a car's or truck's camera,
           its class's camera set's driving and hovering strategies: vehicle_cam_lines)
  vcamera  <form> <drive orbit> <drive height> <hover orbit> <hover height> [<drive fov> <hover fov>]
           (its camera, metres and degrees: vehicle_camera_line, the fields of view for
           Escalation's cameras; none for the Infiltrator's; a car's or truck's is its
           vcam lines')
  vsocket  <form> <socket> <bone> <tx ty tz> <qx qy qz qw> <sx sy sz>  (the vehicle's sockets its class's sixth
           part hangs on: its wheels, a tank's cannons)
  vdecal   <form> <socket> <bone> <tx ty tz> <qx qy qz qw> <sx sy sz>  (the vehicle's decal sockets, every
           one whose name starts with Decal: a decal set hangs its decals on them, tools/build_decals.py;
           the robot's are among the parts' socket lines, on the chests)
  vtank    <form> <cannon bone> <muzzle bone> <tx ty tz> <qx qy qz qw> <sx sy sz>   (a tank: the bone
           its cannon aims with and the socket its shells leave from, as vsocket's)
  vplane   <form> <climb speed> <climb secs> <dash speed> <dash secs> <drag> <lean>
           <pitch lead> <yaw lead> <roll lead> <lead lerp> <full pitch> <max pitch> <hover pitch>
           <hover fov> <fly fov> <hover pitch min> <hover pitch max> <fly pitch min> <fly pitch max>
           <fly turn speed> <fly turn accel> <fly turn damping> <fly pitch speed>
           <fly pitch accel> <fly pitch damping> <fly camera roll> <take-off secs>
           <hover secs> <crash secs>   (a jet: its hover's
           VerticalDashSpeed and Duration (Ascend and Descend) and DashSpeed and Duration,
           metres and seconds; its flight's DragCoefficient (Unreal's, on its sideways and
           vertical speed) and its hover's MotionPhysicsScale (how far it leans into its
           moves); its flight's lead into turns, PitchDueToPitchValue, YawDueToYawValue and
           RollDueToYawValue, degrees, ExtraRotationLerpValue (a frame's share), and the
           body pitch over which its pitch lead fades out (FullPitchThreshold to
           MaxPitchValue, degrees); how far above the view it hovers pitched
           (PitchOffsetFromCamera, degrees); its cameras' fields of view and pitch ranges
           hovering and flying, degrees (plane_line); its flying camera's turn: the view's
           top yaw speed, acceleration and damping, its pitch's, degrees a second, and its
           roll per degree a second of yaw: TnFlyingFollowCameraBehavior; and its clips'
           lengths taking off, dropping back to hovering and crashing, seconds)
  vfx      <form> <kind> <particle system> <bone> <tx ty tz> <pitch yaw roll> <sx sy sz>
           (its effects on their sockets: a tank's or a jet's, TANK_FX (hover, boost, jump,
           land), a car's or a truck's, CAR_FX (hover, boost, jump, land_low, land_high,
           hover_land_low, hover_land_high); the socket's place in glTF axes, metres, its
           rotation and its scale, Unreal's: hover jets are as big as their push times it,
           TnPlaneForm's and TnCarForm's Hovering.CalculateBoosterFxAmount)
  vfxland  <form> <secs>   (a car's or a truck's landings are its low ones under this long
           in the air: its LowLandingAirTimeThreshold)
  vpoint   <form> <kind> <x y z>   (the hover jets, jump jet, boosters and the guns' muzzles (by turns:
           left, right, left, right; a truck's launchers', left, right; a jet's and a tank's, theirs): metres,
           x right, y up, -z ahead; the jets where they are as it hovers, the rest as it drives,
           the guns out)
  vsound   <form> <event> <cue>   (its sounds: each event of its sound event set with a cue,
           Auto_Boost_Start..., and Death, its DeathSound; tools/build_sounds.py takes the cues)
  vtsound  <form> <to_vehicle|to_robot> <time> <cue>   (the transformation's sounds: the
           robot's and the car's clips' sound notifies, both clips starting as it begins)
  vcsound  <form> <clip> <time> <cue>   (a sound one of its looping clips plays, by its
           index in its model, car.rs's CAR_*: Snarl's footsteps, tail and growls)
  armset   <set> <clip>
  switch   <arm set> <bone> <time> <from> <to> <ramp>   (ScaleBone notifies of its MirrorReverse clip)
  shoulderset <set> <change hands clip> <equip clip>
  gunpaint <robot.glb material> <its colour texture, with the paint mask>   (the gun
           arm and the guns: GUN_ARM, WEAPON_PAINT; the Infiltrator's file only, as
           robot.glb's guns are every class's)
Clip numbers are indices into clips.glb; times are seconds of the clips as the game
plays them (main.rs plays the transformations faster, at TRANSFORM_SPEED).
"""
import hashlib
import json
import math
import os
import sys

import numpy as np
from PIL import Image

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
sys.path.insert(0, os.path.join(HERE, "ue3"))
import class_data  # noqa: E402
import ktx2_io  # noqa: E402
import mirror_anims  # noqa: E402
import transform_fit  # noqa: E402
from gltf_io import Doc, Writer, node_globals, quat_to_mat  # noqa: E402
from psa_io import Psa  # noqa: E402

# Robot.glb's model turn (build_assets.py's FOC_YAW): umodel's +X ahead to Bevy's -Z.
YAW = 90.0
# The vehicles are built 2.2 times their size, as vehicle.glb is (main.rs and car.rs
# draw the car model at 1 / 2.2), times their mesh set's MeshScale.
VEHICLE_BUILD_SCALE = 2.2
# Material names never drawn: the faction logo decals (the demo draws no decals).
DROP = {"TR_FactionLogo_Mat_INST"}
# Parts whose material isn't cooked into the map (an import nothing provides):
# the texture set they use, by mesh.
MATERIAL_FALLBACK = {"RB_Bumblebee_CHARSKEL_ArmLoL": "RB_Bumblebee_ArmLo_TEXSET",
                     "RB_Bumblebee_CHARSKEL_ArmLoR": "RB_Bumblebee_ArmLo_TEXSET"}
# The pictures of a material made of separate ones instead of a texture set
# (Iconic_Trucks_MAT's, Soundwave's wheels): by role, their parameters' names.
PLAIN_TEXTURES = {"color": "Diffuse Map", "normal": "Normal_XY_Map", "cust": "CustA_Ruff_CustB",
                  "glow": "GlowA_Coeff_GlowB"}
# Window and interior materials (no texture set): dark, glossy glass.
GLASS = {"pbrMetallicRoughness": {"baseColorFactor": [0.015, 0.017, 0.02, 1.0], "metallicFactor": 0.85,
                                  "roughnessFactor": 0.12}}
GUN_PREFIXES = ("GunL_", "GunR_")
GUN_MATERIAL = "WEP_AssaultRifle_MATINST"
# The arm built into robot.glb's weapon meshes (the gun arm's upper arm) is a
# customizable part too: its material there, and its texture set (from
# WEP_AssaultRifleArm_p.xxx), painted like the parts.
GUN_ARM = ("RB_ArmUp_Cust_MatINST_INST", "WEP_ArmUp_TEXSET")
# The guns are painted too: the game puts the character's colours on the weapon in
# hand, robot's or vehicle's (TnCharacterApplier.ApplyColorsToCharacter ->
# TnWeapon.ApplyCharacterColorsToMeshes), whose materials are children of
# Iconic_Cars_TEXSET_Mat as the parts' are (Cust_Color_A, Cust_COLOR_B). robot.glb's
# weapon materials and their texture sets, read straight from CookedPC
# (WEAPON_PACKAGES, the full-size mips from each weapon's WEP_*_p.xxx); the car
# guns' (GUN_MATERIAL) is GUN_TEXSET.
WEAPON_PAINT = (
    ("WEP_AssaultRifle_MATINST", "WEP_AssaultRifle_TEXSET"),
    ("WEP_SubsonicRepeater_MATINST", "WEP_SubsonicRepeater_TEXSET"),
    ("WEP_BurstRifle_MATINST", "WEP_BurstRifle_TEXSET"),
    ("WEP_Shotgun_MatINST", "WEP_Shotgun_TEXSET"),
    ("WEP_SniperRifle_MatINST", "WEP_SniperRifle_TEXSET"),
    ("WEP_IonBlaster_MATINST", "WEP_IonBlaster_TEXSET"),
    ("WEP_RiotCannon_MATINST", "WEP_RiotCannon_TEXSET"),
    ("WEP_SysCorruptor_MATINST", "WEP_SysCorrupter_TEXSET"),
    # The eight weapons added later (build_assets.py's NEW_WEAPONS).
    ("WEP_HeavyMG_MATINST", "WEP_HeavyMG_TEXSET"),
    ("WEP_EMPShotgun_MATINST", "WEP_EMPShotgun_TEXSET"),
    ("WEP_TechvoltEmitter_MatINST", "WEP_TechvoltEmitter_TEXSET"),
    ("WEP_RustCannon_MATINST", "WEP_RustCannon_TEXSET"),
    ("WEP_Sawblade_MATINST", "WEP_SawBlade_TEXSET"),
    ("WEP_RocketLauncher_MATINST", "WEP_RocketLauncher_TEXSET"),
    ("WEP_GrenadeLauncher_MATINST", "WEP_A4PulsarCannon_TEXSET"),
    ("WEP_ClusterCannon_MATINST", "WEP_ClusterCannon_TEXSET"),
)
GUN_TEXSET = "WEP_AssaultRifle_TEXSET"
WEAPON_PACKAGES = ("all_shared.xxx",)
# A tank's effects (TnTankForm, its TnVehicleFxPlayer: each of the form's effects
# on its socket): the hover jets while it floats (HoverFX), the boosters while it
# boosts (BoostFx), the jump (JumpFX) and the landing (LandFX).
TANK_FX = (("hover", "HoverFX"), ("boost", "BoostFx"), ("jump", "JumpFX"), ("land", "LandFX"))
# A car's or a truck's (TnCarForm, its TnVehicleFxPlayer): the hover jets while it
# hovers (HoverFX: Hovering.PlayHoverFx), the boosters and trails while it drives on
# its wheels (BoostFx: Driving.PlayBoostFx), the jump (JumpFX: OnBeginJump) and its
# landings, on its wheels (Driving.OnLandComplete: LowLandingFX after less than its
# LowLandingAirTimeThreshold in the air, else HighLandingFX) or hovering
# (Hovering.OnLandComplete: LowLandingHoverFX, HighLandingHoverFX).
CAR_FX = (("hover", "HoverFX"), ("boost", "BoostFx"), ("jump", "JumpFX"), ("land_low", "LowLandingFX"),
          ("land_high", "HighLandingFX"), ("hover_land_low", "LowLandingHoverFX"),
          ("hover_land_high", "HighLandingHoverFX"))
# How far a tank hovers off the ground, its lowest point in its hover pose (metres):
# the game's tanks in its Customize garage (where they float on their hover
# suspension as in play: TnTankForm.set_CustomizationMode) hover from 0.3 to 0.9 m
# up in the reference video of every armor set; the game's suspension physics
# (springs at each body's centre of mass, some tanks carrying a second body that
# weighs them down) isn't modelled. A tank whose hover pose reaches more than
# TANK_LEGS below its root stands on its legs instead (Hardshell's beetle).
TANK_CLEARANCE = 0.6
TANK_LEGS = 0.5
# A tank's cannon (TnTankFormBlueprint's CannonBoneName) and the socket its shells
# leave from (TankCannon_Vehicle_WEPMESH's).
TANK_CANNON_BONE = "C_Cannon_XB"
TANK_MUZZLE = "WeaponSocket_TankCannon"
# The car's clips, in car.rs's order (CAR_*): the clip names in its sets, a
# replacement for sets without one, and whether it loops.
VEHICLE_CLIPS = [
    ("drive", ["Nav_Boost_VEH"], True),
    ("to_vehicle", ["Transform_ToVehicle_VEH"], False),
    ("to_robot", ["Transform_ToRobot_VEH"], False),
    ("hover", ["Nav_Hover_VEH", "Nav_Hover_Pose_VEH"], True),
    ("hover_f", ["Nav_Hover_F_VEH"], True),
    ("hover_b", ["Nav_Hover_B_VEH"], True),
    ("hover_l", ["Nav_Hover_L_VEH"], True),
    ("hover_r", ["Nav_Hover_R_VEH"], True),
    # (A walker without these goes straight to its other state's clip.)
    ("drive_to_hover", ["Nav_BoostToHover_VEH", "Nav_Boost_to_Hover_VEH", "Nav_Hover_VEH"], False),
    ("hover_to_drive", ["Nav_HoverToBoost_VEH", "Nav_Hover_to_Boost_VEH", "Nav_Boost_VEH"], False),
]
# A jet's own (VEH_Plane_ANIMTREE's), after the additive ones (car.rs's CAR_BANK_L...):
# flying, its TnRollAnimBlend's banking turns to the left and right (the level
# flight's Nav_Boost_VEH where a set has none); hovering, its TnAccelerationAnimBlend's
# Up child, the climb; and the crash out of flying (BoostMalfunction's Collision,
# Nav_Collision_VEH: the ordinary drop back to hovering where a set has none).
PLANE_CLIPS = [
    ("bank_l", ["Nav_Boost_L_VEH", "Nav_Boost_VEH"], True),
    ("bank_r", ["Nav_Boost_R_VEH", "Nav_Boost_VEH"], True),
    ("hover_up", ["Nav_Hover_Ascent_VEH", "Nav_Hover_Pose_VEH", "Nav_Hover_VEH"], True),
    ("crash", ["Nav_Collision_VEH", "Nav_Boost_to_Hover_VEH", "Nav_Hover_VEH"], False),
]
# A walker's own (the Dinobots': Snarl, Slug, Grimlock), after the additive ones in
# a jet's places (car.rs's CAR_JUMP...): the clip its form plays as it jumps
# (HoverJumpAnim, looping while it is in the air: PlayAnimation(.., true)) and as it
# lands, on the move (HoverLandAnim) or standing (HoverIdleLandAnim: TnTankForm's
# UpdateJumping and OnLandComplete, TnCarForm's Hovering state's). Boosting it jumps
# and lands with BoostJumpAnim and BoostLandAnim, the same clips for every Dinobot.
WALKER_CLIPS = [("jump", "hover_jump", True), ("land", "hover_land", False), ("land_idle", "idle_land", False)]
# Where they are among the vehicle model's clips (after VEHICLE_CLIPS, GUN_CLIPS and
# the additive ones: car.rs's CAR_JUMP), for their sounds' vcsound lines.
WALKER_FIRST_CLIP = 16
GUN_CLIPS = [("gun_equip", "Weapon_Equip", False), ("gun_fire", "Weapon_Fire", True),
             ("gun_unequip", "Weapon_UnEquip", False)]
VEHICLE_ADDITIVE = [("hover_bob", "ADD_Nav_Hover_VEH", True), ("guns_out", "ADD_Weapon_Equip_VEH", False),
                    ("guns_in", "ADD_Weapon_Unequip_VEH", False)]
# A tank's, in the same places: its cannon's reload (the TankCannon's
# TnWeaponEffectPlayAnimation) where the car guns slide out.
TANK_ADDITIVE = [("hover_bob", "ADD_Nav_Hover_VEH", True), ("cannon_reload", "ADD_TankShooting_Reload", False),
                 ("guns_in", "ADD_Weapon_Unequip_VEH", False)]
# A jet's: its hover bob alone (VEH_Plane_ANIMTREE's hovering adds ADD_Nav_Hover_VEH;
# it has no guns to slide out).
PLANE_ADDITIVE = [("hover_bob", "ADD_Nav_Hover_VEH", True), ("guns_out", None, False), ("guns_in", None, False)]
# A jet's homing rockets leave from its weapon sockets by turns (HomingRocket_
# Vehicle_WEPMESH's muzzles).
PLANE_MUZZLES = ("WeaponSocket_Primary", "WeaponSocket_Primary2")
# The vehicle weapons with meshes on the weapon sockets (the weapon's
# _AttachmentSockets: WeaponSocket_Primary and Primary2), by the class's vehicle
# weapon: the cars' guns (AssaultRifleVehicle's AssaultRifle_Vehicle_WEPMESH:
# WEP_AssaultRifleVH_SKEL, in MP_Array_BASE_m.xxx) and the Destroyer's trucks'
# rocket launchers (RocketVehicle's Rocket_Vehicle_WEPMESH: WEP_RocketLauncherVH_SKEL,
# in mp_shared.xxx; ROCKETS). Each: its mesh, its animation set and its material's
# texture set (WEAPON_PAINT). The rockets leave from the launchers' muzzles
# (MuzzleSockets: MuzzleFlash) by turns (TnWeaponEffectIncrementMuzzleSocket); the
# launcher's only clip is Weapon_Fire, as each one fires, which the demo doesn't
# play: they are held as it starts (GUN_CLIPS, gun_clips).
CAR_GUNS = {"mesh": "WEP_AssaultRifleVH_SKEL", "anims": "WEP_AssaultRifleVH_ANIM",
            "material": GUN_MATERIAL, "texset": GUN_TEXSET}
ROCKETS = "RocketVehicle"
ROCKET_LAUNCHERS = {"mesh": "WEP_RocketLauncherVH_SKEL", "anims": "WEP_RocketLauncherVH_ANIM",
                    "material": "WEP_RocketLauncher_MATINST", "texset": "WEP_RocketLauncher_TEXSET"}
VEHICLE_GUN_TEXSETS = {g["material"]: g["texset"] for g in (CAR_GUNS, ROCKET_LAUNCHERS)}
# The transformations' frame where they meet the car's own clips, and the clip
# met: folding up ends where it hovers (the car comes out hovering), unfolding
# starts where it drives (build_vehicle moves each so the car's body bone is just
# there).
TRANSFORM_CLIPS = {"to_vehicle": (-1, "hover"), "to_robot": (0, "drive")}
ANCHOR_BONE = "C_Body_XB"
# Points the car's effects come from (car.rs): the sockets its blueprint puts these
# effects on (HoverFX, JumpFX, BoostFx; of the boost ones, the jets: not the trail
# or the ram's effect, nor Snarl's dust).
VEHICLE_POINTS = {"hover": "HoverFX", "jump": "JumpFX", "boost": "BoostFx"}
# A vehicle form's name less these is its file's (SwindleTruckForm: vehicle_swindle).
FORM_SUFFIXES = ("VehicleFormTank", "VehicleForm", "TruckForm", "TankForm", "PlaneForm", "JetForm", "Form")
# The robot sounds of a class's sound event set the demo plays (main.rs: its
# footsteps, jumps, landings and body falls).
ROBOT_SOUND_EVENTS = ("FS_DEFAULT_", "BF_DEFAULT_")

# robot.glb's clips the classes have their own of (build_assets.py writes them;
# main.rs's ANIM_* numbers): (number, name, the game's clip, the set robot.glb's is
# from, loops). A class plays its own where its definition's animation sets give a
# clip a set the Infiltrator's don't (UE3's FindAnimSequence: the last of the
# robot definition's AnimSets that has the clip), and its melee set's attack in
# place of the melee.
NAV, SCOUT, ROBO = "Shared_ROBO_NAV_ANIM", "Shared_SCOUT_ROBO_ANIM", "Shared_ROBO_ANIM"
STRAFES = ("FL", "FR", "L", "R", "BL", "BR")
# robot.glb's first heavy-stance clip (main.rs's ANIM_HEAVY_IDLE).
HEAVY_SLOT = 171
ROBOT_SLOTS = (
    [(0, "idle", "Aim_Rest_F_C", NAV, True)]
    + [(1 + i, f"jog_{d.lower()}", f"Nav_StrafeJog_Rest_{d}", SCOUT, True) for i, d in enumerate(STRAFES)]
    + [(7, "sprint", "Nav_Sprint_F", NAV, True), (10, "land", "Nav_Land", NAV, False)]
    + [(11 + i, f"boost_{d.lower()}", f"Nav_Boost_{d}", NAV, False) for i, d in enumerate("FBLR")]
    + [(15, "melee", "Melee_ShieldBash", ROBO, False), (16, "melee_boost", "Melee_Boost", ROBO, False),
       (17, "air_boost_l", "Nav_Air_Boost_L", NAV, False), (18, "air_boost_r", "Nav_Air_Boost_R", NAV, False),
       (19, "land_hard", "Nav_Land_Hard_01", NAV, False), (20, "throw", "Grenade_Throw_MP", ROBO, False),
       (21, "hit", "HitReaction_Descent", ROBO, False), (22, "death", "Death_FrontShot_05", "Death_Shared_ANIM", False),
       (23, "air_boost_f", "Nav_Air_Boost_F", NAV, False), (24, "air_boost_b", "Nav_Air_Boost_B", NAV, False),
       (25, "shoot_idle", "Aim_Shoot_F_C", NAV, True)]
    + [(26 + i, f"shoot_move_{d.lower()}", f"Nav_StrafeJog_Shoot_{d}", SCOUT, True) for i, d in enumerate(STRAFES)]
    + [(32, "aim_idle", "Aim_FineAim_F_C", NAV, True)]
    + [(33 + i, f"aim_move_{d.lower()}", f"Nav_StrafeWalk_FineAim_{d}", SCOUT, True) for i, d in enumerate(STRAFES)]
    + [(39, "jump", "Nav_TakeOff_01", NAV, False), (40, "jump_descent", "Nav_Jump_Descent", NAV, False),
       (41, "fall", "Nav_Jump_Descent_02", NAV, False)]
    + [(67 + 4 * k + i, f"{stance}_{d.lower()}", prefix + d, SCOUT, True)
       for k, (stance, prefix, dirs) in enumerate((("jog", "Nav_StrafeJog_Rest_", ("F", "b", "LB", "RB")),
                                                   ("shoot", "Nav_StrafeJog_Shoot_", ("F", "B", "LB", "RB")),
                                                   ("aim", "Nav_StrafeWalk_FineAim_", ("F", "B", "LB", "RB"))))
       for i, d in enumerate(dirs)]
    + [(79 + 2 * k + i, f"{stance}_pivot_{d.lower()}", f"Nav_{game}_IdlePivot90_{d}", NAV, False)
       for k, (stance, game) in enumerate((("rest", "Rest"), ("shoot", "Shoot"), ("aim", "FineAim")))
       for i, d in enumerate("LR")]
    + [(85 + i, f"shoot_pivot180_{d.lower()}", f"Nav_Shoot_IdlePivot180_{d}", NAV, False) for i, d in enumerate("LR")]
    # The heavy weapons' stance (build_assets.py's new_weapon_clips, after the eight
    # weapons added later): standing, jogging and walking in the ten strafe
    # directions, and turning on the spot.
    + [(HEAVY_SLOT, "heavy_idle", "Nav_Idle_HeavyWep_Shoot", SCOUT, True)]
    + [(HEAVY_SLOT + 1 + k * 10 + i, f"heavy_{gait}_{d.lower()}", f"Nav_Strafe{game}_HeavyWep_Shoot_{d}", SCOUT, True)
       for k, (gait, game) in enumerate((("jog", "Jog"), ("walk", "Walk")))
       for i, d in enumerate(("FL", "FR", "L", "R", "BL", "BR", "F", "B", "LB", "RB"))]
    + [(HEAVY_SLOT + 21 + i, f"heavy_pivot_{d.lower()}", f"Nav_Shoot_HeavyWep_IdlePivot90_{d}", SCOUT, False)
       for i, d in enumerate("LR")]
    + [(HEAVY_SLOT + 23 + i, f"heavy_pivot180_{d.lower()}", f"Nav_Shoot_HeavyWep_IdlePivot180_{d}", SCOUT, False)
       for i, d in enumerate("LR")]
)
MELEE_SLOT = 15
# The Customize preview's idle holding a gun (build_customize.py's cust_weapon).
CUST_WEAPON = "Nav_Idle_Shoot"
# Jazz's own (robot.glb's, made from the Infiltrator's clips): the metres he covers
# per footstep jogging, sprinting and walking in fine aim (main.rs's JOG_STRIDE,
# SPRINT_STRIDE, AIM_STRIDE) and the seconds between his footsteps in those clips
# (two to a cycle: the jog's and the walk's whole clip, the sprint's stride window).
JAZZ_STRIDES = {"jog": (4.2, 0.7 / 2), "sprint": (7.0, 21 / 30 / 2), "aim": (2.8, 1.1333 / 2)}
# Jazz's own jump take-off (Nav_TakeOff_01) and melee (Melee_ShieldBash, as main.rs
# plays it) last this long.
JAZZ_TAKEOFF_SECS = 0.67
JAZZ_MELEE_SECS = 1.10


def file_name(form):
    """A vehicle form's file name: its name less its kind, lower case."""
    base = form.split(".")[-1]
    for s in FORM_SUFFIXES:
        if base.endswith(s):
            base = base[: -len(s)]
            break
    return base.lower()


def effect_sockets(v, kind):
    """The sockets of a vehicle's effects of one kind, in its blueprint's order (of
    the boost ones, the jets: Booster_*, *_boost_*, and the afterburners of some
    trucks and the tanks)."""
    out = []
    for socket, fx in v["fx"].get(VEHICLE_POINTS[kind], []):
        if kind == "boost" and not any(k in (fx or "").lower() for k in ("boost", "afterburner")):
            continue
        if socket not in out:
            out.append(socket)
    return out


def wheel_sockets(v):
    return [s for s in v["sockets"] if s["name"].lower().startswith("wheel")]


def attach_sockets(data):
    """The socket names the class's vehicle parts hang on (their AttachmentSocket:
    "Wheel", the Titan's "Cannon", the Scientist's "WingSocket"), lower case."""
    return tuple(sorted({p["wheel"]["socket"].lower() for p in data["parts"] if p.get("wheel")})) or ("wheel",)


def wheel_meshes(data):
    """The static meshes the class's parts hang on sockets: the robot's, and the
    vehicle's where they differ (a jet's wings: class_data.py)."""
    return sorted({m for p in data["parts"] if p.get("wheel")
                   for m in (p["wheel"]["mesh"], p["wheel"]["vehicle_mesh"]) if m})


def part_line(p, data):
    """A part's line (see the top). The sixth part's static mesh is the robot's,
    on its parts' sockets; where the vehicle has its own on its sockets, a last
    field names it (a jet's wings: the robot's, RB_Airraid_STAT_Wing, stand up on
    its back, the jet's are VH_Airraid_STAT_Wing; "-" where the vehicle has none)."""
    w = p.get("wheel") or {}
    wheel = w.get("mesh") or "-"
    vehicle = p["vehicle"].split(".")[-1] if p.get("vehicle") else "-"
    arm = next((a for a in p["anims"] if a in data["arm_sets"]), "-") if p["type"] == "ArmLo" else "-"
    shoulder = next((a for a in p["anims"] if "Shoulder" in a), "-") if p["type"] == "ArmUp" else "-"
    socket = w.get("socket", "-")
    fields = ["part", p["id"], p["type"], p["name"], str(int(p["dlc"])), ",".join(p["meshes"]) or "-",
              wheel, vehicle, arm, shoulder, ",".join(p["disable"]) or "-", socket]
    if w and w["vehicle_mesh"] != w["mesh"]:
        fields.append(w["vehicle_mesh"] or "-")
    return "\t".join(fields)


def part_sockets(v, attach):
    """A vehicle's sockets its class's vehicle part hangs on (every one whose name
    starts with the part's AttachmentSocket: its wheels, a tank's cannons)."""
    return [s for s in v["sockets"] if s["name"].lower().startswith(attach)]


# The start of the decal sockets' names (a decal set's AttachmentSockets all start so:
# DecalBack01, DecalFront02...; tools/build_decals.py).
DECAL_SOCKET = "Decal"


def decal_lines(form, v):
    """A vehicle's vdecal lines: its decal sockets."""
    lines = []
    for s in v["sockets"]:
        if s["name"].startswith(DECAL_SOCKET):
            t, q, sc = gltf_socket(s)
            lines.append("\t".join(["vdecal", form, s["name"], s["bone"], fmt(t), fmt(q), fmt(sc)]))
    return lines


def is_tank(v):
    """A tank (TnTankFormBlueprint: the Titan's): it only hovers, faster as it
    boosts, and fires its cannon."""
    return v.get("kind") == "TnTankFormBlueprint"


def is_plane(v):
    """A jet (TnPlaneFormBlueprint: the Scientist's): it hovers and flies, without
    gravity, and fires homing rockets."""
    return v.get("kind") == "TnPlaneFormBlueprint"


def walks(v):
    """A walker (Snarl, Slug, Grimlock): a Dinobot on its legs (Dinobot_ANIMTREE), or
    with no wheels to turn and nowhere to hang them. (Soundwave's truck turns none
    either but has its wheel sockets; a tank hovers.)"""
    if v.get("anim_tree"):
        return "Dinobot" in v["anim_tree"]
    return not v["visual_wheels"] and not wheel_sockets(v) and not is_tank(v) and not is_plane(v)


# ---------------------------------------------------------------------------
# Inputs
# ---------------------------------------------------------------------------

class Exports:
    """umodel's export folders (one per package): files found by kind and name."""

    def __init__(self, root):
        self.root = root
        # The map's own folder first.
        pkgs = sorted(os.listdir(root))
        self.pkgs = [p for p in pkgs if p.startswith("MP_")] + [p for p in pkgs if not p.startswith("MP_")]

    def find(self, kind, name):
        for p in self.pkgs:
            path = os.path.join(self.root, p, kind, name)
            if os.path.exists(path):
                return path
        return None


def ue_rotator_quat(pitch, yaw, roll):
    """Unreal rotator (65536 units a turn) to a quaternion in Unreal's axes."""
    k = math.pi / 32768.0 / 2.0
    sp, cp = math.sin(pitch * k), math.cos(pitch * k)
    sy, cy = math.sin(yaw * k), math.cos(yaw * k)
    sr, cr = math.sin(roll * k), math.cos(roll * k)
    return (cr * sp * sy - sr * cp * cy, -cr * sp * cy - sr * cp * sy,
            cr * cp * sy - sr * sp * cy, cr * cp * cy + sr * sp * sy)


def gltf_socket(s):
    """A socket's transform in umodel's glTF axes (Unreal's Y and Z swapped, a
    mirror, so a rotation's axis flips with it), metres."""
    loc, rot, sc = s["location"], s["rotation"], s["scale"]
    qx, qy, qz, qw = ue_rotator_quat(*rot)
    q = np.array([qx, qz, qy, -qw])
    q = q * (1 if q[3] >= 0 else -1)
    return ([loc[0] * 0.01, loc[2] * 0.01, loc[1] * 0.01], q.tolist(), [sc[0], sc[2], sc[1]])


def fmt(v):
    return " ".join(f"{x:.6g}" for x in v)


# ---------------------------------------------------------------------------
# Textures
# ---------------------------------------------------------------------------

# The glow. The characters' materials (TR_BRDF_CustChar_TEXSET_MAT under
# Iconic_Cars_TEXSET_Mat and the rest; ENV_BRDF_A_MAT's graph) light two things
# with the texture set's Masks_NormY red: the part's own light (TR_Emissive_MF:
# the painted colour times DiffuseIntensityScalar and Emissive_Intensity) and its
# energon (TR_Energon_MF: the team's EnergonColor, pulsing, times
# Energon_Intensity). The sets keep that red one of two ways: resting at half (the
# material's two-channel red: from .45 down to nothing its own light, from .55 up
# to full its energon, as Jazz's head has his visor and his eyes) or resting at
# nothing (the red is the light, own and energon alike, as Breakdown's wheels' rims).
# The glow texture (<set>_e) holds the part's own light in red and its energon in
# green, stored so that the sRGB decode an emissive texture gets gives them back;
# src/character.rs lights them (assets/shaders/paint.wgsl). A material of separate
# pictures (Soundwave's wheels) has them in its GlowA_Coeff_GlowB: its own light in
# red, the energon in blue.
GLOW_COVERAGE = 0.0005
# (The energon's pulse runs over fractalMaps, which the demo takes from the energon
# cube's model: tools/build_assets.py.)
# Where the glow pass (--glow) looks for the texture sets' pictures, in this order.
GLOW_PACKAGES = (class_data.MP_PACKAGE, "mp_shared.xxx", "all_shared.xxx")


def srgb_to_linear(x):
    return np.where(x <= 0.04045, x / 12.92, ((x + 0.055) / 1.055) ** 2.4)


def linear_to_srgb(x):
    return np.where(x <= 0.0031308, x * 12.92, 1.055 * np.power(np.maximum(x, 0), 1 / 2.4) - 0.055)


def glow_masks(red):
    """A texture set's own light and energon (0-1 arrays) from its Masks_NormY red
    (0-1, as stored): two-channel when it rests at half, else the red as sampled."""
    if float(np.median(red)) > 0.25:
        return np.clip(0.45 - red, 0, 1) * 2, np.clip(red - 0.55, 0, 1) * 2
    lit = srgb_to_linear(red)
    return lit, lit


def glow_picture(own, energon):
    """The glow texture's pixels (RGB, 8-bit) of the two masks, or None when too
    little of the set glows to need one."""
    if (np.maximum(own, energon) > 0.2).mean() <= GLOW_COVERAGE:
        return None
    rgb = np.stack([own, energon, np.zeros_like(own)], -1)
    return (linear_to_srgb(np.clip(rgb, 0, 1)) * 255 + 0.5).astype(np.uint8)


_cooked = {}
# (CookedTextures' and Textures.cooked_picture's packages' Texture2Ds by full name.)
_by_full_name = {}


def cooked_texture(cooked, name):
    """A Texture2D's largest mip (RGBA array) straight from the game's packages
    (WEAPON_PACKAGES, its full-size mips from the package it came from), or None."""
    import texture as ue3_texture
    for f in WEAPON_PACKAGES:
        path = os.path.join(cooked, f)
        if not os.path.exists(path):
            continue
        if path not in _cooked:
            p = ue3_texture._package(path)
            _cooked[path] = (p, {e["name"]: i + 1 for i, e in enumerate(p.exports)
                                 if p.class_name(e) == "Texture2D"})
        p, index = _cooked[path]
        if name in index:
            return np.asarray(ue3_texture.read_texture(p, index[name], cooked)[0])
    return None


class Textures:
    """The texture sets as KTX2 files in models/<class>/textures (cached by content
    in FOC_KTX_CACHE, as build_map.py's)."""

    def __init__(self, exports, out_dir, vehicle_glb, cooked=None):
        self.ex = exports
        self.dir = out_dir
        self.done = {}
        self.vehicle_glb = vehicle_glb
        self.cooked = cooked

    def _encode(self, arr, kind):
        cache = os.environ.get("FOC_KTX_CACHE")
        if cache:
            arr = np.ascontiguousarray(arr)
            key = hashlib.sha1(arr.tobytes() + repr((arr.shape, kind)).encode()).hexdigest()
            path = os.path.join(cache, key + ".ktx2")
            if os.path.exists(path):
                return open(path, "rb").read()
            data = ktx2_io.encode(arr, kind)
            os.makedirs(cache, exist_ok=True)
            open(path, "wb").write(data)
            return data
        return ktx2_io.encode(arr, kind)

    def _write(self, name, arr, kind):
        # (The glTF writers put the files in place, named after their keys.)
        return self._encode(arr, kind)

    def _pictures(self, base, cooked):
        """A texture set's two pictures (Color_NormX, Masks_NormY: RGBA arrays): from
        CookedPC when `cooked` (and it has them), else umodel's export; or None."""
        if cooked and self.cooked:
            c = cooked_texture(self.cooked, base + "_Color_NormX")
            m = cooked_texture(self.cooked, base + "_Masks_NormY")
            if c is not None and m is not None:
                return c, m
        cp = self.ex and self.ex.find("Texture2D", base + "_Color_NormX.png")
        mp = self.ex and self.ex.find("Texture2D", base + "_Masks_NormY.png")
        if not cp or not mp:
            # (Not exported: from CookedPC, by name.)
            c, m = self.cooked_picture(base + "_Color_NormX"), self.cooked_picture(base + "_Masks_NormY")
            return (c, m) if c is not None and m is not None else None
        return np.asarray(Image.open(cp).convert("RGBA")), np.asarray(Image.open(mp).convert("RGBA"))

    def texset(self, base, cooked=False):
        """{"c": colour+mask, "n": normal, "mr": metal-roughness, "e": glow or None}:
        KTX2 bytes, or None when its pictures weren't exported (`cooked`: read from
        CookedPC, see _pictures)."""
        if base in self.done:
            return self.done[base]
        pictures = self._pictures(base, cooked)
        if pictures is None:
            print("  ! texture set not exported:", base)
            self.done[base] = None
            return None
        c, m = pictures
        h, w = c.shape[:2]
        if m.shape[:2] != (h, w):
            m = np.asarray(Image.fromarray(m).resize((w, h), Image.BILINEAR))
        color = np.concatenate([c[..., :3], m[..., 2:3]], -1)
        # Normals: X in the colour's alpha, Y in the mask's (DirectX-style: flipped).
        nx = c[..., 3].astype(np.float32) / 127.5 - 1
        ny = -(m[..., 3].astype(np.float32) / 127.5 - 1)
        normal = np.clip(np.stack([nx, ny, np.zeros_like(nx)], -1) * 127.5 + 127.5, 0, 255).astype(np.uint8)
        spec = m[..., 1].astype(np.float32) / 255
        rough = np.clip(0.95 - 0.7 * spec, 0.2, 0.95)
        metal = np.clip(0.25 + 0.55 * spec, 0, 1)
        mr = (np.stack([np.zeros_like(spec), rough, metal], -1) * 255).astype(np.uint8)
        glow = glow_picture(*glow_masks(m[..., 0].astype(np.float32) / 255))
        res = {"c": self._write(base + "_c", color, "color"), "n": self._write(base + "_n", normal, "normal"),
               "mr": self._write(base + "_mr", mr, "data"), "e": None}
        if glow is not None:
            res["e"] = self._write(base + "_e", glow, "color")
        self.done[base] = res
        return res

    def cooked_picture(self, full):
        """A Texture2D's largest mip (an RGBA array) by its full name (or, without a
        dot, its object name), straight from CookedPC (the first of GLOW_PACKAGES
        that has it, its full-size mips from the package it came from), or None."""
        if not (self.cooked and full):
            return None
        import texture as ue3_texture
        for f in GLOW_PACKAGES:
            path = os.path.join(self.cooked, f)
            if not os.path.exists(path):
                continue
            if path not in _by_full_name:
                p = ue3_texture._package(path)
                index = {}
                for i, e in enumerate(p.exports):
                    if p.class_name(e) == "Texture2D":
                        index[p.full_name(i + 1)] = i + 1
                        index.setdefault(e["name"], i + 1)
                _by_full_name[path] = (p, index)
            p, index = _by_full_name[path]
            if full in index:
                return np.asarray(ue3_texture.read_texture(p, index[full], self.cooked)[0])
        return None

    def pictures(self, key, textures, full=None):
        """A material of separate pictures (PLAIN_TEXTURES: Soundwave's wheels) as a
        texture set's maps (see texset), or None when they weren't exported (or,
        given their full names, `full`, by parameter, aren't in CookedPC: the vehicles'
        Interior_DIF...). Its
        CustA_Ruff_CustB holds where the primary colour goes (red), its roughness
        (green, as a texture set's specular) and where the secondary goes (blue);
        its GlowA_Coeff_GlowB the glow (red, and the energon in blue); its normal
        map has X in its alpha and Y in its green."""
        if key in self.done:
            return self.done[key]
        names = {role: textures.get(param) for role, param in PLAIN_TEXTURES.items()}
        paths = {role: n and self.ex and self.ex.find("Texture2D", n + ".png") for role, n in names.items()}
        cooked = {role: None if paths[role] else self.cooked_picture((full or {}).get(param))
                  for role, param in PLAIN_TEXTURES.items()}
        if not paths["color"] and cooked["color"] is None:
            print("  ! pictures not exported:", key, names)
            self.done[key] = None
            return None

        def picture(role):
            return Image.open(paths[role]).convert("RGBA") if paths[role] else Image.fromarray(cooked[role])

        def load(role, size, default):
            if not paths[role] and cooked[role] is None:
                return np.full(size + (4,), default, np.uint8)
            im = picture(role)
            if im.size != size[::-1]:
                im = im.resize(size[::-1], Image.BILINEAR)
            return np.asarray(im)
        c = np.asarray(picture("color"))
        size = c.shape[:2]
        cust = load("cust", size, 0).astype(np.float32) / 255
        glow = load("glow", size, 0).astype(np.float32) / 255
        nrm = load("normal", size, 128).astype(np.float32)
        mask = np.clip(0.5 + 0.5 * cust[..., 0] - 0.5 * cust[..., 2], 0, 1)
        color = np.concatenate([c[..., :3], (mask * 255)[..., None].astype(np.uint8)], -1)
        nx = nrm[..., 3] / 127.5 - 1
        ny = -(nrm[..., 1] / 127.5 - 1)
        normal = np.clip(np.stack([nx, ny, np.zeros_like(nx)], -1) * 127.5 + 127.5, 0, 255).astype(np.uint8)
        spec = cust[..., 1]
        rough = np.clip(0.95 - 0.7 * spec, 0.2, 0.95)
        metal = np.clip(0.25 + 0.55 * spec, 0, 1)
        mr = (np.stack([np.zeros_like(spec), rough, metal], -1) * 255).astype(np.uint8)
        lit = glow_picture(srgb_to_linear(glow[..., 0]), srgb_to_linear(glow[..., 2]))
        res = {"c": self._write(key + "_c", color, "color"), "n": self._write(key + "_n", normal, "normal"),
               "mr": self._write(key + "_mr", mr, "data"), "e": None}
        if lit is not None:
            res["e"] = self._write(key + "_e", lit, "color")
        self.done[key] = res
        return res

    def plain(self, key, plain, full):
        """A material with neither a texture set nor the cars' pictures as a texture
        set's maps (see texset), unpainted (the mask at half), from CookedPC: a plain
        Material's own textures (class_data's plain_textures: InteriorAlt_Energon_MAT's,
        whose Interior_Emiss glows in the energon's colour, and the Swarmer's interior,
        each with an ordinary normal map, X in red and Y in green), or TR_Base_Mat's
        pictures by parameter (`full`: Vortex's interior, its Lum_Map's diffuse
        luminance in green and its specular in red, its Normal_XY_Map with X in its
        alpha and Y in its green). None without them."""
        if key in self.done:
            return self.done[key]
        emissive = None
        if plain.get("diffuse"):
            c = self.cooked_picture(plain["diffuse"])
            spec = self.cooked_picture(plain.get("specular"))
            nrm = self.cooked_picture(plain.get("normal"))
            emissive = self.cooked_picture(plain.get("emissive"))
            xy = False
            if c is not None:
                c = c[..., :3]
        elif full.get("Lum_Map"):
            lum = self.cooked_picture(full["Lum_Map"])
            c = spec = None
            if lum is not None:
                c = np.repeat(lum[..., 1:2], 3, -1)
                spec = np.repeat(lum[..., 0:1], 3, -1)
            nrm = self.cooked_picture(full.get("Normal_XY_Map"))
            xy = True
        else:
            c = None
        if c is None:
            print("  ! plain material's pictures not in CookedPC:", key, plain or full)
            self.done[key] = None
            return None
        h, w = c.shape[:2]

        def sized(a):
            if a is None or a.shape[:2] == (h, w):
                return a
            return np.asarray(Image.fromarray(a).resize((w, h), Image.BILINEAR))
        spec, nrm, emissive = sized(spec), sized(nrm), sized(emissive)
        color = np.concatenate([c, np.full((h, w, 1), 128, np.uint8)], -1)
        if nrm is None:
            normal = np.full((h, w, 3), (128, 128, 128), np.uint8)
        else:
            n = nrm.astype(np.float32) / 127.5 - 1
            nx, ny = (n[..., 3], -n[..., 1]) if xy else (n[..., 0], -n[..., 1])
            normal = np.clip(np.stack([nx, ny, np.zeros_like(nx)], -1) * 127.5 + 127.5, 0, 255).astype(np.uint8)
        gloss = spec[..., :3].astype(np.float32).mean(-1) / 255 if spec is not None else np.full((h, w), 0.3, np.float32)
        rough = np.clip(0.95 - 0.7 * gloss, 0.2, 0.95)
        metal = np.clip(0.25 + 0.55 * gloss, 0, 1)
        mr = (np.stack([np.zeros_like(gloss), rough, metal], -1) * 255).astype(np.uint8)
        lit = None
        if emissive is not None:
            # (Its own energon, Interior_Emiss times itself panning: the blue it
            # glows with, in the energon's colour.)
            e = srgb_to_linear(emissive[..., 2].astype(np.float32) / 255)
            lit = glow_picture(np.zeros_like(e), e * e)
        res = {"c": self._write(key + "_c", color, "color"), "n": self._write(key + "_n", normal, "normal"),
               "mr": self._write(key + "_mr", mr, "data"), "e": None}
        if lit is not None:
            res["e"] = self._write(key + "_e", lit, "color")
        self.done[key] = res
        return res

    def gun(self):
        """The car guns' texture set, as vehicle.glb has it (PNG bytes by role)."""
        if "gun" in self.done:
            return self.done["gun"]
        res = None
        if os.path.exists(self.vehicle_glb):
            d = Doc(self.vehicle_glb)
            m = next((m for m in d.g["materials"] if m.get("name") == GUN_MATERIAL), None)
            if m:
                def img(t):
                    if not t:
                        return None
                    src = d.g["images"][d.g["textures"][t["index"]]["source"]]
                    return d.view_bytes(src["bufferView"])
                pbr = m.get("pbrMetallicRoughness", {})
                res = {"c": img(pbr.get("baseColorTexture")), "mr": img(pbr.get("metallicRoughnessTexture")),
                       "n": img(m.get("normalTexture")), "e": img(m.get("emissiveTexture")),
                       "strength": m.get("extensions", {}).get("KHR_materials_emissive_strength", {})}
        if res is None:
            print("  ! no car gun material in", self.vehicle_glb)
        self.done["gun"] = res
        return res


class Materials:
    """glTF materials for one Writer, by game material name."""

    def __init__(self, w, textures, game_materials):
        self.w = w
        self.tex = textures
        self.game = game_materials
        self.cache = {}

    def _use(self, ext):
        used = self.w.g.setdefault("extensionsUsed", [])
        if ext not in used:
            used.append(ext)

    def get(self, name, mesh=None):
        texset = MATERIAL_FALLBACK.get(mesh) if name.startswith("dummy_material") else None
        # (The mesh's own where several of the package's materials share the name:
        # the vehicles' VH_Interior_MAT.)
        info = self.game.for_mesh(name, mesh) if hasattr(self.game, "for_mesh") else self.game.get(name)
        if texset is None:
            texset = info and info.get("texset")
        # A material of separate pictures (Soundwave's wheels; the Breakdown's and
        # Sideswipe's insides, from CookedPC) is drawn as a texture set made of them,
        # by its own name.
        pictures = None
        if texset is None and info and info.get("textures", {}).get(PLAIN_TEXTURES["color"]):
            pictures = self.tex.pictures(name, info["textures"], info.get("texture_paths"))
            if pictures:
                texset = name
        # A plain material (the energon-lit insides, InteriorAlt_Energon_MAT; the
        # Swarmer's and Swoop's) or TR_Base_Mat's pictures (Vortex's insides): its
        # own textures from CookedPC, by its diffuse texture's name.
        if texset is None and pictures is None and info and (
                info.get("plain", {}).get("diffuse") or info.get("texture_paths", {}).get("Lum_Map")):
            source = info["plain"].get("diffuse") or info["texture_paths"]["Lum_Map"]
            key = source.split(".")[-1]
            pictures = self.tex.plain(key, info["plain"], info["texture_paths"])
            if pictures:
                texset = key
        # The car guns and the trucks' launchers, painted (WEAPON_PAINT) when CookedPC
        # has their texture sets.
        own = VEHICLE_GUN_TEXSETS.get(name)
        if own and self.tex.texset(own, cooked=True):
            texset = own
        key = texset or name
        if key in self.cache:
            return self.cache[key]
        w = self.w
        if name == GUN_MATERIAL and texset != GUN_TEXSET and self.tex.gun():
            g = self.tex.gun()
            m = {"name": name, "pbrMetallicRoughness": {"baseColorTexture": {"index": w.texture("gun_c", g["c"])},
                                                        "metallicRoughnessTexture": {"index": w.texture("gun_mr", g["mr"])},
                                                        "metallicFactor": 1.0, "roughnessFactor": 1.0},
                 "normalTexture": {"index": w.texture("gun_n", g["n"])}}
            if g["e"]:
                m["emissiveTexture"] = {"index": w.texture("gun_e", g["e"])}
                m["emissiveFactor"] = [1, 1, 1]
                m["extensions"] = {"KHR_materials_emissive_strength": g["strength"] or {"emissiveStrength": 8.0}}
                self._use("KHR_materials_emissive_strength")
        elif texset and (pictures or self.tex.texset(texset)):
            t = pictures or self.tex.texset(texset)
            m = {"name": texset,
                 "pbrMetallicRoughness": {"baseColorTexture": {"index": w.texture(texset + "_c", t["c"])},
                                          "metallicRoughnessTexture": {"index": w.texture(texset + "_mr", t["mr"])},
                                          "metallicFactor": 1.0, "roughnessFactor": 1.0},
                 "normalTexture": {"index": w.texture(texset + "_n", t["n"])},
                 "extras": {"paint": True}}
            # (Its glow masks, which src/character.rs lights: see GLOW_COVERAGE.)
            if t["e"]:
                m["emissiveTexture"] = {"index": w.texture(texset + "_e", t["e"])}
                m["emissiveFactor"] = [1, 1, 1]
        else:
            if texset is None:
                print("  ! no texture set for material", name, "- drawn as glass")
            m = dict(GLASS, name=name)
        self.cache[key] = w.material(m)
        return self.cache[key]


def primitives(w, mats, doc, mesh_index, mesh_name, joint_map=None):
    """A mesh's primitives (the decals left out), its joints renumbered."""
    out = []
    for p in doc.g["meshes"][mesh_index]["primitives"]:
        mname = doc.g["materials"][p["material"]]["name"]
        if mname in DROP:
            continue
        at = p["attributes"]
        attrs = {"POSITION": w.accessor(doc.acc(at["POSITION"]).astype(np.float32), 34962, minmax=True),
                 "NORMAL": w.accessor(doc.acc(at["NORMAL"]).astype(np.float32), 34962),
                 "TEXCOORD_0": w.accessor(doc.acc(at["TEXCOORD_0"]).astype(np.float32), 34962)}
        if "TANGENT" in at:
            attrs["TANGENT"] = w.accessor(doc.acc(at["TANGENT"]).astype(np.float32), 34962)
        if "JOINTS_0" in at:
            j = doc.acc(at["JOINTS_0"]).astype(np.uint16)
            if joint_map is not None:
                j = joint_map[j].astype(np.uint16)
            attrs["JOINTS_0"] = w.accessor(j, 34962)
            wt = doc.acc(at["WEIGHTS_0"]).astype(np.float32)
            attrs["WEIGHTS_0"] = w.accessor(wt / np.maximum(wt.sum(1, keepdims=True), 1e-8), 34962)
        idx = doc.acc(p["indices"]).reshape(-1).astype(np.uint32)
        out.append({"attributes": attrs, "indices": w.accessor(idx, 34963), "material": mats.get(mname, mesh_name)})
    return out


def copy_skeleton(w, doc, parent):
    """umodel's skeleton (every node but the mesh node 0) under `parent`; returns
    the offset of its nodes and the skin's joints (as new node indices)."""
    offset = len(w.g["nodes"])
    for n in doc.g["nodes"][1:]:
        nn = {k: v for k, v in n.items() if k in ("name", "translation", "rotation", "scale")}
        if "children" in n:
            nn["children"] = [c - 1 + offset for c in n["children"]]
        w.node(nn)
    root = offset + doc.g["nodes"][0]["children"][0] - 1
    w.g["nodes"][parent].setdefault("children", []).append(root)
    joints = [j - 1 + offset for j in doc.g["skins"][0]["joints"]]
    return offset, root, joints


def root_node(w, scale=1.0):
    h = math.radians(YAW) / 2
    return w.node({"name": "Root", "rotation": [0, math.sin(h), 0, math.cos(h)], "scale": [scale] * 3,
                   "children": []}, root=True)


# ---------------------------------------------------------------------------
# Clips
# ---------------------------------------------------------------------------

_psa = {}


def psa(path):
    if path not in _psa:
        _psa[path] = Psa(path)
    return _psa[path]


def clip_tracks(ex, sets, name):
    """A clip from the last of `sets` that has it: ({bone: (pos, rot)}, frame time)."""
    for s in reversed(sets):
        path = ex.find("AnimSet", s + ".psa")
        if path and any(c["name"] == name for c in psa(path).clips):
            return psa(path).clip(name)
    return None, None


def scaled(bones, s):
    """A clip's tracks with their offsets scaled, as a mesh with a TranslationScale
    plays them."""
    if bones is None or s == 1.0:
        return bones
    return {b: (np.asarray(pos, np.float32) * np.float32(s), rot) for b, (pos, rot) in bones.items()}


def channel(w, samplers, channels, node, path, t, v):
    if len(v) > 2 and np.allclose(v, v[:1], atol=1e-6):
        t, v = np.array([0.0, t[-1]], np.float32), v[:2]
    samplers.append({"input": w.accessor(np.asarray(t, np.float32), minmax=True),
                     "output": w.accessor(np.asarray(v, np.float32)), "interpolation": "LINEAR"})
    channels.append({"sampler": len(samplers) - 1, "target": {"node": node, "path": path}})


def base_clip(w, names, out_name, bones, dt, loop=False, trim=False, rest=None, skip=("Root_Reference",),
              prefixes=None, turn_only=(), window=None):
    """A clip of poses (build_assets.py's add_animations for a .psa): every frame,
    a looping clip closed on its first pose; `trim` cuts the still frames off its
    end; bones it leaves alone hold `rest` ({bone: (t, r)}), as Unreal holds them
    at the reference pose. `prefixes`: the clip drives copies of another skeleton
    (the car guns), its bones under these name prefixes. `turn_only`: bones whose
    turn the clip keeps, held at their rest place (the root reference of the
    robot's transformations). `window`: the (start, end) frames of a looping clip's
    repeating cycle (a sprint starts by speeding up), the drift between its ends
    taken out of the offsets so it closes."""
    tracks = []
    for bone, (pos, rot) in bones.items():
        if any(s in bone for s in skip):
            continue
        if any(s in bone for s in turn_only):
            pos = np.asarray([(rest or {}).get(bone, ([0.0, 0.0, 0.0], None))[0]], np.float32)
        targets = [p + bone for p in prefixes if p + bone in names] if prefixes else (
            [bone] if bone in names else [])
        for t in targets:
            tracks.append((names[t], pos, rot))
    frames = max(len(p) for _, p, _ in tracks)
    if window and loop:
        lo, hi = window
        n = hi - lo
        cut = []
        for node, pos, rot in tracks:
            pos, rot = np.asarray(pos, np.float32), np.asarray(rot, np.float32)
            if len(pos) == frames:
                end = pos[hi % frames]
                seg = pos[lo:hi].copy()
                pos = seg - (end - pos[lo])[None, :] * (np.arange(n)[:, None] / n)
            if len(rot) == frames:
                rot = rot[lo:hi]
            cut.append((node, pos, rot))
        tracks, frames = cut, n
    last = frames
    if trim and frames > 2:
        motion = np.zeros(frames - 1)
        for _, _, rot in tracks:
            if len(rot) == frames:
                dots = np.abs(np.sum(rot[1:] * rot[:-1], 1)).clip(0, 1)
                motion += np.degrees(2 * np.arccos(dots))
        moving = np.nonzero(motion > 0.5)[0]
        last = int(moving[-1]) + 2 if len(moving) else frames
    length = (frames if loop else max(last - 1, 1)) * dt
    samplers, channels = [], []
    done = set()
    for node, pos, rot in tracks:
        done.add(node)
        for path, v in (("translation", pos), ("rotation", rot)):
            v = np.asarray(v, np.float32)
            if len(v) == 1:
                # (A bone held still, over the clip's own length: a longer track
                # would lengthen the clip, and a loop would stop for the rest.)
                t, v = np.array([0.0, length], np.float32), np.repeat(v, 2, 0)
            elif loop:
                first = v[:1].copy()
                if path == "rotation" and float(np.dot(v[-1], first[0])) < 0:
                    first = -first
                v = np.concatenate([v, first])
                t = np.arange(len(v)) * dt
            else:
                v = v[:last]
                t = np.arange(len(v)) * dt
            channel(w, samplers, channels, node, path, t, v)
    for bone, (rt, rr) in (rest or {}).items():
        if bone in names and names[bone] not in done:
            for path, v in (("translation", rt), ("rotation", rr)):
                channel(w, samplers, channels, names[bone], path, np.array([0.0, length]), np.array([v, v]))
    w.g.setdefault("animations", []).append({"name": out_name, "samplers": samplers, "channels": channels})
    return length


def additive_clip(w, names, out_name, bones, dt, loop=False, keep=lambda b: True):
    """An additive clip (build_assets.py's add_additive_animations for one part):
    the turns put in front of a bone's rotation and the offsets added to its
    position, every frame; only the bones that move."""
    samplers, channels = [], []
    frames = 1
    for bone, (pos, rot) in bones.items():
        if bone not in names or "Root_Reference" in bone or not keep(bone):
            continue
        frames = max(frames, len(rot))
        r = rot.astype(np.float64) * np.sign(rot[:, 3:4] + 1e-12)
        p = pos.astype(np.float64)
        if loop:
            r, p = np.concatenate([r, r[:1]]), np.concatenate([p, p[:1]])
        t = np.arange(len(r)) * dt
        for path, v, still in (("rotation", r, np.abs(r[:, :3]).max() < 1e-4),
                               ("translation", p, np.abs(p).max() < 1e-4)):
            if not still:
                channel(w, samplers, channels, names[bone], path, t, v)
    if not channels:
        hips = names.get("C_Spine00_Hips_XB", next(iter(names.values())))
        channel(w, samplers, channels, hips, "rotation", np.array([0.0, 1.0]), np.array([[0, 0, 0, 1]] * 2))
    w.g.setdefault("animations", []).append({"name": out_name, "samplers": samplers, "channels": channels})


def empty_additive(w, names, out_name):
    additive_clip(w, names, out_name, {}, 1 / 30)


def still_clip(w, names, out_name, node):
    """A clip of poses that holds one node where it rests (a slot of car.rs's that a
    vehicle has nothing for: a tank's car-gun clips, on a node of their own that
    nothing else moves, so they blend with nothing)."""
    n = w.g["nodes"][node]
    samplers, channels = [], []
    for path, v in (("translation", n.get("translation", [0, 0, 0])), ("rotation", n.get("rotation", [0, 0, 0, 1]))):
        channel(w, samplers, channels, node, path, np.array([0.0, 1.0]), np.array([v, v]))
    w.g.setdefault("animations", []).append({"name": out_name, "samplers": samplers, "channels": channels})


def stride_window(bones, frames, shortest=15, longest=40):
    """A looping clip's repeating cycle that a non-looping start leads into (the
    sprint, which speeds up first): the (start, end) frames, from the second half
    of the clip, between which the skeleton's pose repeats most closely."""
    rots = np.stack([np.asarray(r, np.float64) for b, (_, r) in bones.items()
                     if "Reference" not in b and len(r) == frames], 1)
    best = None
    for i in range(frames // 2 - shortest, frames):
        for j in range(i + shortest, min(frames, i + longest)):
            d = np.abs(np.sum(rots[i] * rots[j], -1)).clip(0, 1)
            err = float(np.degrees(2 * np.arccos(d)).mean())
            if best is None or err < best[0]:
                best = (err, i, j)
    if best is None:
        return None
    print(f"  sprint cycle: frames {best[1]}-{best[2]} ({best[0]:.2f} degrees apart)")
    return best[1], best[2]


# ---------------------------------------------------------------------------
# The builds
# ---------------------------------------------------------------------------

def reference_skeleton(ex, data):
    """A part mesh's export with the whole robot skeleton (every class's parts have
    the same): Jazz's legs, as robot.glb's, when exported, else the class's first."""
    path = ex.find("SkeletalMesh3", "RB_Jazz_CHARSKEL_Legs.gltf")
    if path:
        return path
    for p in data["parts"]:
        if p["type"] == "Legs":
            for m in p["meshes"]:
                path = ex.find("SkeletalMesh3", m + ".gltf")
                if path:
                    return path
    raise SystemExit("no legs part exported to take the robot skeleton from")


def build_parts(ex, data, textures, game_mats, out):
    """parts.glb: the robot skeleton, every part mesh skinned to it, every wheel."""
    w = Writer(share=True, image_dir=os.path.join(out, "textures"), image_uri="textures")
    mats = Materials(w, textures, game_mats)
    root = root_node(w)
    ref_path = reference_skeleton(ex, data)
    ref = Doc(ref_path)
    _, _, joints = copy_skeleton(w, ref, root)
    ref_names = [ref.g["nodes"][j]["name"] for j in ref.g["skins"][0]["joints"]]
    ref_ibm = ref.acc(ref.g["skins"][0]["inverseBindMatrices"]).astype(np.float32)
    w.g["skins"].append({"joints": joints, "inverseBindMatrices": w.accessor(ref_ibm), "skeleton": joints[0]})
    count = 0
    for mesh in sorted(data["meshes"]):
        path = ex.find("SkeletalMesh3", mesh + ".gltf")
        if not path:
            print("  ! part mesh not exported:", mesh)
            continue
        d = Doc(path)
        names = [d.g["nodes"][j]["name"] for j in d.g["skins"][0]["joints"]]
        missing = [n for n in names if n not in ref_names]
        if missing:
            print("  ! joints not on the robot skeleton:", mesh, missing[:4])
        jmap = np.array([ref_names.index(n) if n in ref_names else 0 for n in names])
        # A mesh bound in a pose of its own (the Destroyer's Elite crown, its lips'
        # bones a few centimetres off) keeps its own: its own skin, on the same bones.
        ibm = d.acc(d.g["skins"][0]["inverseBindMatrices"]).astype(np.float32)
        own = ref_ibm.copy()
        for k, n in enumerate(names):
            if n in ref_names:
                own[ref_names.index(n)] = ibm[k]
        skin = 0
        if np.abs(own - ref_ibm).max() > 1e-3:
            w.g["skins"].append({"joints": joints, "inverseBindMatrices": w.accessor(own), "skeleton": joints[0]})
            skin = len(w.g["skins"]) - 1
        prims = primitives(w, mats, d, d.g["nodes"][0]["mesh"], mesh, jmap)
        w.g["meshes"].append({"name": mesh, "primitives": prims})
        w.g["nodes"][root]["children"].append(w.node({"name": mesh, "mesh": len(w.g["meshes"]) - 1, "skin": skin}))
        count += 1
    wheels = wheel_meshes(data)
    for mesh in wheels:
        path = ex.find("StaticMesh3", mesh + ".gltf")
        if not path:
            print("  ! wheel mesh not exported:", mesh)
            continue
        d = Doc(path)
        w.g["meshes"].append({"name": mesh, "primitives": primitives(w, mats, d, d.g["nodes"][0]["mesh"], mesh)})
        w.node({"name": mesh, "mesh": len(w.g["meshes"]) - 1}, root=True)
    size = w.save(os.path.join(out, "parts.glb"))
    print(f"wrote parts.glb ({count} parts, {len(wheels)} wheels, {size / 1e6:.1f} MB)")
    return ref_path


def robot_rest(ref):
    """The robot skeleton's bind pose: {bone: (translation, rotation)}."""
    return {n["name"]: (n.get("translation", [0, 0, 0]), n.get("rotation", [0, 0, 0, 1]))
            for n in ref.g["nodes"][1:]}


def robot_writer(robot_glb, ref_path):
    """A Writer with robot.glb's Root and skeleton, as they are (names, hierarchy,
    rest pose), so the clips written to it drive the robot model's bones; returns
    it and its nodes by name."""
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
    joints = [new[j] for j in rg["skins"][0]["joints"]]
    ref = Doc(ref_path)
    w.g["skins"].append({"joints": joints,
                         "inverseBindMatrices": w.accessor(ref.acc(ref.g["skins"][0]["inverseBindMatrices"]).astype(np.float32))})
    return w, {w.g["nodes"][i]["name"]: i for i in new.values()}


def build_clips(ex, data, robot_glb, ref_path, out):
    """clips.glb (and clips_mirror.glb): the clips the parts and vehicles bring, on
    robot.glb's skeleton. Returns {"vehicles": {form: (to_vehicle, to_robot,
    lengths)}, "arms": {set: clip}, "shoulders": {set: (mirror, equip)}}."""
    w, names = robot_writer(robot_glb, ref_path)
    rest = robot_rest(Doc(ref_path))
    result = {"vehicles": {}, "arms": {}, "shoulders": {}}
    additive = set()
    index = 0
    for form, v in vehicles_in_order(data):
        lengths = []
        for clip_name, out_name in (("Transform_ToVehicle_ROBO", "to_vehicle"), ("Transform_ToRobot_ROBO", "to_robot")):
            bones_, dt = clip_tracks(ex, v["robot_anims"], clip_name)
            if bones_ is None:
                raise SystemExit(f"{clip_name} of {form} not exported ({v['robot_anims']})")
            # (With its root reference's turn: Sideswipe's folding up is made with
            # the whole robot turned 122.6 degrees on it, the rest of the skeleton
            # turned back, so without it he flips off to the side and folds up
            # facing away from his car. Its offset stays out, as before.)
            lengths.append(base_clip(w, names, f"{file_name(form)}_{out_name}", bones_, dt,
                                     trim=out_name == "to_vehicle", rest=rest, skip=(),
                                     turn_only=("Root_Reference",)))
        result["vehicles"][form] = (index, index + 1, lengths)
        index += 2
    # The forearms' switch: everything but the spine and legs (build_assets.py's
    # arm_bones: it would fight the running and jumping), and the shoulder pads'.
    arm_bones = lambda b: b[:2] in ("L_", "R_") and "_CloudA_" not in b and not any(  # noqa: E731
        k in b for k in ("_Leg", "Knee", "_Face"))
    for s in sorted(data["arm_sets"]):
        bones_, dt = clip_tracks(ex, [s], "ADD_Weapon_MirrorReverse_ARM")
        if bones_ is None:
            print("  ! no ADD_Weapon_MirrorReverse_ARM in", s)
            empty_additive(w, names, s + "_change_hands")
        else:
            additive_clip(w, names, s + "_change_hands", bones_, dt, keep=arm_bones)
        result["arms"][s] = index
        additive.add(index)
        index += 1
    # (Only the pads: some sets' clips turn the hand and fingers too, which the
    # game's tree keeps to the forearm clips.)
    pads = lambda b: "_PadRobo" in b  # noqa: E731
    shoulder_sets = sorted({a for p in data["parts"] if p["type"] == "ArmUp" for a in p["anims"] if "Shoulder" in a})
    for s in shoulder_sets:
        ids = []
        for clip_name, out_name in (("ADD_Weapon_Arm_Shoulder_Mirror_ARM", "change_hands"),
                                    ("ADD_Weapon_Arm_Shoulder_EquipUnequip_ARM", "equip")):
            bones_, dt = clip_tracks(ex, [s], clip_name)
            if bones_ is None:
                print("  ! no", clip_name, "in", s)
                empty_additive(w, names, f"{s}_{out_name}")
            else:
                additive_clip(w, names, f"{s}_{out_name}", bones_, dt, keep=pads)
            ids.append(index)
            additive.add(index)
            index += 1
        result["shoulders"][s] = tuple(ids)
    path = os.path.join(out, "clips.glb")
    size = w.save(path)
    print(f"wrote clips.glb ({index} clips, {size / 1e6:.1f} MB)")
    size, n = mirror_anims.mirror(path, ref_path, os.path.join(out, "clips_mirror.glb"),
                                  lambda i, name: i in additive)
    print(f"wrote clips_mirror.glb ({n} mirrored clips, {size / 1e6:.1f} MB)")
    return result


def class_clip(data, slot_set, game_clip):
    """The animation set a class plays a clip from (the last of its robot
    definition's AnimSets with it), and the Infiltrator's: the sets each one names
    that have it, by the export's own lists (`data["anim_clips"]`)."""
    def resolve(sets):
        for s in reversed(sets):
            if game_clip in data["anim_clips"].get(s, ()):
                return s
        return slot_set
    return resolve(data["robot"]["anims"]), resolve(data["scout_anims"])


def build_robot_clips(ex, data, robot_glb, ref_path, out, packages=()):
    """robot_clips.glb (and robot_clips_mirror.glb): the class's own versions of
    robot.glb's clips (ROBOT_SLOTS): {robot.glb clip: robot_clips.glb clip}, the
    class line's clip timings, the sounds the clips play ({robot.glb clip:
    [(seconds, cue)]}, from their notifies in `packages`, class_data.Mp's: its
    melee's, or the Infiltrator's sword swing's where its own plays none), and
    the preview's pose holding a gun where the class has its own (CUST_WEAPON:
    the robot_clips.glb clip, or None)."""
    overrides = {}
    timings = {}
    sounds = {}
    # (robot.glb's clips are the Infiltrator's own, but for its melee: Jazz's shield
    # bash there, its melee set's sword swing in multiplayer.)
    only_melee = data["class"] == "infiltrator"

    def notified(anim_set, clip):
        for pkg in packages:
            q = pkg.sequence(anim_set, clip) if pkg else 0
            if q:
                return pkg.clip_sounds(q, data["class_events"])
        return []
    w, names = robot_writer(robot_glb, ref_path)
    melee = data["robot"].get("melee_clip")
    for number, out_name, game_clip, slot_set, loop in ROBOT_SLOTS:
        if number == MELEE_SLOT and melee:
            sets, game_clip = [melee[0]], melee[1]
        elif only_melee:
            continue
        else:
            own, scout = class_clip(data, slot_set, game_clip)
            if own == scout or own == slot_set:
                continue
            sets = [own]
        bones, dt = clip_tracks(ex, sets, game_clip)
        if bones is None:
            print(f"  ! {game_clip} of {sets[0]} not exported: {out_name} stays robot.glb's")
            continue
        # (umodel's frame time already has the clip's RateScale in it: the
        # Destroyer's shooting jog, 0.95, plays at 28.5 frames a second.)
        frames = max(len(p) for p, _ in bones.values())
        window = stride_window(bones, frames) if out_name == "sprint" else None
        length = base_clip(w, names, out_name, bones, dt, loop=loop, window=window)
        overrides[number] = len(w.g["animations"]) - 1
        timings[out_name] = (length, window, dt)
        if number == MELEE_SLOT:
            sounds[number] = notified(sets[0], game_clip) or notified(*data["scout_melee"])
        print(f"  + {out_name}: {sets[0]}.{game_clip} ({length:.2f}s{', loop' if loop else ''})")
    # The Customize preview's pose holding a gun (build_customize.py's cust_weapon,
    # the Scout's Nav_Idle_Shoot): the class's own, where its sets have one, after
    # the others (main.rs plays none of it).
    cust_weapon = None
    own, scout = class_clip(data, SCOUT, CUST_WEAPON)
    if own not in (scout, SCOUT) and not only_melee:
        bones, dt = clip_tracks(ex, [own], CUST_WEAPON)
        if bones is None:
            print(f"  ! {CUST_WEAPON} of {own} not exported: the preview holds a gun in the Scout's pose")
        else:
            length = base_clip(w, names, "cust_weapon", bones, dt, loop=True)
            cust_weapon = len(w.g["animations"]) - 1
            print(f"  + cust_weapon: {own}.{CUST_WEAPON} ({length:.2f}s, loop)")
    if not overrides and cust_weapon is None:
        return overrides, timings, sounds, None
    path = os.path.join(out, "robot_clips.glb")
    size = w.save(path)
    print(f"wrote robot_clips.glb ({len(w.g['animations'])} clips, {size / 1e6:.1f} MB)")
    size, n = mirror_anims.mirror(path, ref_path, os.path.join(out, "robot_clips_mirror.glb"),
                                  lambda i, name: False)
    print(f"wrote robot_clips_mirror.glb ({n} mirrored clips, {size / 1e6:.1f} MB)")
    return overrides, timings, sounds, cust_weapon


def class_line(data, timings):
    """The class line: its size and the strides, take-off and melee its clips make."""
    r = data["robot"]
    strides = {}
    for kind, slot in (("jog", "jog_fl"), ("sprint", "sprint"), ("aim", "aim_move_fl")):
        stride, interval = JAZZ_STRIDES[kind]
        if slot in timings:
            length, window, dt = timings[slot]
            # Two footsteps to a cycle (the sprint's: its window).
            mine = ((window[1] - window[0]) * dt if window else length) / 2
            stride *= mine / interval
        strides[kind] = stride
    takeoff = timings["jump"][0] if "jump" in timings else JAZZ_TAKEOFF_SECS
    melee = timings["melee"][0] if "melee" in timings else JAZZ_MELEE_SECS
    return "\t".join(["class", data["class"], f"{r['scale']:.4f}", f"{r['height']:.1f}", f"{r['radius']:.1f}",
                      f"{strides['jog']:.3f}", f"{strides['sprint']:.3f}", f"{strides['aim']:.3f}",
                      f"{takeoff:.4f}", f"{melee:.4f}"])


def posed(w, clip):
    """The writer's glTF with its nodes in the first frame of one of its clips."""
    g = dict(w.g, nodes=[dict(n) for n in w.g["nodes"]])
    a = next(x for x in w.g["animations"] if x["name"] == clip)
    for c in a["channels"]:
        acc = w.g["accessors"][a["samplers"][c["sampler"]]["output"]]
        bv = w.g["bufferViews"][acc["bufferView"]]
        n = {"VEC3": 3, "VEC4": 4}[acc["type"]]
        v = np.frombuffer(bytes(w.blob[bv["byteOffset"]:bv["byteOffset"] + 4 * n]), np.float32)
        g["nodes"][c["target"]["node"]][c["target"]["path"]] = v.tolist()
    return g


def clip_values(w, clip, frame=0):
    """A clip's values at one of its frames (-1: its last): {(node, path): value}."""
    a = next(x for x in w.g["animations"] if x["name"] == clip)
    out = {}
    for c in a["channels"]:
        acc = w.g["accessors"][a["samplers"][c["sampler"]]["output"]]
        bv = w.g["bufferViews"][acc["bufferView"]]
        n = {"VEC3": 3, "VEC4": 4}[acc["type"]]
        k = frame % acc["count"]
        at = bv["byteOffset"] + acc.get("byteOffset", 0) + 4 * n * k
        out[(c["target"]["node"], c["target"]["path"])] = np.frombuffer(bytes(w.blob[at:at + 4 * n]), np.float32)
    return out


def gun_clip(anims, clip, guns):
    """One of GUN_CLIPS of the vehicle's weapon meshes (bones, frame time): the car
    guns' own; the rocket launchers held as their Weapon_Fire starts (the only clip
    they have, which the game plays as each one fires)."""
    if guns is CAR_GUNS:
        return anims.clip(clip)
    bones, _ = anims.clip("Weapon_Fire")
    if bones is None:
        return None, None
    held = {b: (np.repeat(np.asarray(pos)[:1], 2, 0), np.repeat(np.asarray(rot)[:1], 2, 0))
            for b, (pos, rot) in bones.items()}
    return held, 1.0


def guns_out_globals(w):
    """The vehicle's nodes' globals as it drives with its guns out and firing:
    the drive pose, the guns' Weapon_Fire pose, and ADD_Weapon_Equip_VEH (the pods
    slid out) at its end put in front."""
    g = dict(w.g, nodes=[dict(n) for n in w.g["nodes"]])
    for clip in ("drive", "gun_fire"):
        for (node, path), v in clip_values(w, clip).items():
            g["nodes"][node][path] = v.tolist()
    for (node, path), v in clip_values(w, "guns_out", -1).items():
        n = g["nodes"][node]
        if path == "rotation":
            r = np.asarray(n.get("rotation", [0, 0, 0, 1]), np.float64)
            x, y, z, q = v.astype(np.float64)
            a, b, c, d = r
            n["rotation"] = [q * a + x * d + y * c - z * b, q * b - x * c + y * d + z * a,
                             q * c + x * b - y * a + z * d, q * d - x * a - y * b - z * c]
        else:
            n["translation"] = (np.asarray(n.get("translation", [0, 0, 0])) + v).tolist()
    return node_globals(g)


def clip_globals(w, names, bones, frame, root):
    """The nodes' globals (4x4) in the Root node's space, posed at one frame of a
    clip's tracks (the bones it has no track for at rest)."""
    g = dict(w.g, nodes=[dict(n) for n in w.g["nodes"]])
    for k in ("translation", "rotation", "scale"):
        g["nodes"][root].pop(k, None)
    for bone, (pos, rot) in bones.items():
        if bone in names:
            g["nodes"][names[bone]]["translation"] = np.asarray(pos)[frame].tolist()
            g["nodes"][names[bone]]["rotation"] = np.asarray(rot)[frame].tolist()
    return node_globals(g)


def vertical_extent(w, d, joints, clip):
    """The lowest and highest point of a vehicle's mesh (umodel's, skinned as
    written) in the first frame of one of its clips: metres, as main.rs draws the
    model."""
    G = node_globals(posed(w, clip))
    ibm = d.acc(d.g["skins"][0]["inverseBindMatrices"]).reshape(-1, 4, 4).transpose(0, 2, 1)
    rows = np.array([(G[j] @ ibm[i])[1] for i, j in enumerate(joints)])
    low, high = np.inf, -np.inf
    for p in d.g["meshes"][0]["primitives"]:
        a = p["attributes"]
        pos = np.c_[d.acc(a["POSITION"]).astype(np.float64), np.ones(d.g["accessors"][a["POSITION"]]["count"])]
        jts = d.acc(a["JOINTS_0"]).astype(int)
        wts = d.acc(a["WEIGHTS_0"]).astype(np.float64)
        if np.issubdtype(d.acc(a["WEIGHTS_0"]).dtype, np.integer):
            wts /= np.iinfo(d.acc(a["WEIGHTS_0"]).dtype).max
        y = sum(wts[:, c] * np.einsum("nj,nj->n", rows[jts[:, c]], pos) for c in range(jts.shape[1]))
        low, high = min(low, float(y.min())), max(high, float(y.max()))
    return low / VEHICLE_BUILD_SCALE, high / VEHICLE_BUILD_SCALE


def vehicles_in_order(data):
    """The vehicles in the order their sets come (the chests' order)."""
    seen = []
    for p in data["parts"]:
        if p.get("vehicle") and p["vehicle"] in data["vehicles"] and p["vehicle"] not in seen:
            seen.append(p["vehicle"])
    return [(f.split(".")[-1], data["vehicles"][f]) for f in seen]


def build_vehicle(ex, form, v, textures, game_mats, out, model_dir, gun_sockets=(), guns=CAR_GUNS):
    """vehicle_<name>.glb; returns what <class>.txt says about it. `guns`: the
    weapon meshes on its weapon sockets (CAR_GUNS, ROCKET_LAUNCHERS), and
    `gun_sockets` their muzzle sockets."""
    name = file_name(form)
    path = ex.find("SkeletalMesh3", v["mesh"] + ".gltf")
    if not path:
        raise SystemExit(f"vehicle mesh not exported: {v['mesh']}")
    d = Doc(path)
    w = Writer(share=True, image_dir=os.path.join(out, "textures"), image_uri="textures")
    mats = Materials(w, textures, game_mats)
    scale = VEHICLE_BUILD_SCALE * v["scale"]
    root = root_node(w, scale)
    offset, skel_root, joints = copy_skeleton(w, d, root)
    w.g["skins"].append({"joints": joints, "inverseBindMatrices": w.accessor(
        d.acc(d.g["skins"][0]["inverseBindMatrices"]).astype(np.float32)), "skeleton": skel_root})
    w.g["meshes"].append({"name": v["mesh"], "primitives": primitives(w, mats, d, d.g["nodes"][0]["mesh"], v["mesh"])})
    w.g["nodes"][root]["children"].append(w.node({"name": v["mesh"], "mesh": 0, "skin": 0}))
    names = {n.get("name"): i for i, n in enumerate(w.g["nodes"]) if i >= offset}
    # The guns' ScaleBone notifies leave them scaled in the car (Bumblebee's at half
    # size); a gun bone's scale is its own (the clips only move and turn bones).
    gun_scale = 1.0
    for e in (v["clips"]["car_to_vehicle"] or {}).get("scale_bones", []):
        if "GunRobo" in e["bone"] and e["start"] == e["end"]:
            gun_scale = e["end"]
    if gun_scale != 1.0:
        for b in ("L_GunRobo01_XT", "R_GunRobo01_XT"):
            if b in names:
                w.g["nodes"][names[b]]["scale"] = [gun_scale] * 3
    # The car guns (build_assets.py's add_car_guns), or a truck's rocket launchers,
    # on the weapon sockets (a tank fires its cannon instead, a jet its rockets from
    # the sockets themselves).
    gun_path = ex.find("SkeletalMesh3", guns["mesh"] + ".gltf")
    if not gun_path:
        raise SystemExit(f"{guns['mesh']} (the vehicles' weapon) not exported")
    sockets = {s["name"]: s for s in v["sockets"]}
    if gun_path and not is_tank(v) and not is_plane(v):
        g = Doc(gun_path)
        gun_mesh = None
        for prefix, socket in zip(GUN_PREFIXES, ("WeaponSocket_Primary", "WeaponSocket_Primary2")):
            s = sockets.get(socket)
            if not s or s["bone"] not in names:
                continue
            t, q, sc = gltf_socket(s)
            mount = w.node({"name": prefix + "Socket", "translation": t, "rotation": q, "scale": sc, "children": []})
            w.g["nodes"][names[s["bone"]]].setdefault("children", []).append(mount)
            goff = len(w.g["nodes"])
            for n in g.g["nodes"][1:]:
                nn = {k: val for k, val in n.items() if k in ("translation", "rotation", "scale")}
                nn["name"] = prefix + n["name"]
                if "children" in n:
                    nn["children"] = [c - 1 + goff for c in n["children"]]
                w.node(nn)
            groot = goff + g.g["nodes"][0]["children"][0] - 1
            w.g["nodes"][mount]["children"].append(groot)
            w.g["skins"].append({"joints": [j - 1 + goff for j in g.g["skins"][0]["joints"]],
                                 "inverseBindMatrices": w.accessor(g.acc(g.g["skins"][0]["inverseBindMatrices"]).astype(np.float32)),
                                 "skeleton": groot})
            if gun_mesh is None:
                w.g["meshes"].append({"name": guns["mesh"],
                                      "primitives": primitives(w, mats, g, g.g["nodes"][0]["mesh"], "gun")})
                gun_mesh = len(w.g["meshes"]) - 1
            w.g["nodes"][root]["children"].append(w.node({"name": prefix + "Mesh", "mesh": gun_mesh,
                                                          "skin": len(w.g["skins"]) - 1}))
        names = {n.get("name"): i for i, n in enumerate(w.g["nodes"]) if i >= offset}
    # The clips, their offsets scaled by the mesh's TranslationScale as the game
    # plays them (Snarl's are made for a Dinobot 1/0.6 the size of his multiplayer
    # one: unscaled, his pieces stand apart). The car's own hold the bone physics
    # places at its rest pose (class_data.py). The transformations move it about
    # as each set's animators placed the car by the robot (Jazz's car three
    # quarters of a metre up, Breakdown's and Deadend's a metre, Wheeljack's and
    # Sideswipe's half a metre down...): each is moved as a whole so that the car
    # is just where it drives and hovers where it meets those clips (the end of
    # folding up, the start of unfolding), as the game's physics keeps it.
    body_bones = [b for b in v.get("body_bones", []) if b in names]
    held = {b: ([w.g["nodes"][names[b]].get("translation", [0, 0, 0])],
                [w.g["nodes"][names[b]].get("rotation", [0, 0, 0, 1])]) for b in body_bones}
    ts = v.get("translation_scale", 1.0)
    tracks = {}
    # (The game's clip each of car.rs's clips is.)
    used = {}
    for out_name, clips, loop in VEHICLE_CLIPS:
        bones = dt = None
        # (A jet's tree hovers in its Nav_Hover_Pose_VEH: Swoop's set has a
        # Nav_Hover_VEH too, which it doesn't play.)
        if is_plane(v) and out_name == "hover":
            clips = ["Nav_Hover_Pose_VEH", "Nav_Hover_VEH"]
        for c in clips:
            bones, dt = clip_tracks(ex, v["anims"], c)
            if bones is not None:
                bones = scaled(bones, ts)
                if out_name not in TRANSFORM_CLIPS:
                    bones = dict(bones, **held)
                used[out_name] = c
                break
        if bones is None:
            # The skeleton's own rest pose, held (what the tree plays without a clip).
            rest = {w.g["nodes"][i]["name"]: (w.g["nodes"][i].get("translation", [0, 0, 0]),
                                             w.g["nodes"][i].get("rotation", [0, 0, 0, 1]))
                    for i in joints}
            bones = {b: (np.array([t, t], np.float32), np.array([r, r], np.float32)) for b, (t, r) in rest.items()}
            dt = 1.0
        tracks[out_name] = (bones, dt, loop)
    if body_bones and ANCHOR_BONE in names:
        anchor = names[ANCHOR_BONE]
        for out_name, (frame, meets) in TRANSFORM_CLIPS.items():
            bones = tracks[out_name][0]
            if body_bones[0] not in bones:
                continue
            want = clip_globals(w, names, tracks[meets][0], 0, root)[anchor][:3, 3]
            got = clip_globals(w, names, bones, frame, root)[anchor][:3, 3]
            pos, rot = bones[body_bones[0]]
            bones[body_bones[0]] = (np.asarray(pos, np.float32) - (got - want).astype(np.float32), rot)
    lengths = {}
    for out_name, _, _ in VEHICLE_CLIPS:
        bones, dt, loop = tracks[out_name]
        lengths[out_name] = base_clip(w, names, out_name, bones, dt, loop=loop, skip=())
    gun_psa = ex.find("AnimSet", guns["anims"] + ".psa")
    spare = None
    if is_tank(v) or is_plane(v):
        spare = w.node({"name": "NoCarGuns"})
        w.g["nodes"][root]["children"].append(spare)
    for out_name, clip, loop in GUN_CLIPS:
        if spare is not None:
            still_clip(w, names, out_name, spare)
            continue
        bones, dt = gun_clip(psa(gun_psa), clip, guns) if gun_psa else (None, None)
        if bones is None:
            raise SystemExit(f"{guns['anims']}.psa not exported")
        base_clip(w, names, out_name, bones, dt, loop=loop, skip=("Reference",), prefixes=GUN_PREFIXES)
    keep = lambda b: b != "C_Reference_XR"  # noqa: E731
    additives = TANK_ADDITIVE if is_tank(v) else PLANE_ADDITIVE if is_plane(v) else VEHICLE_ADDITIVE
    for out_name, clip, loop in additives:
        bones, dt = clip_tracks(ex, v["anims"], clip) if clip else (None, None)
        if bones is None:
            empty_additive(w, names, out_name)
        else:
            additive_clip(w, names, out_name, scaled(bones, ts), dt, loop=loop, keep=keep)
    # A jet's own clips, after those (car.rs's CAR_BANK_L...).
    for out_name, clips, loop in PLANE_CLIPS if is_plane(v) else ():
        for c in clips:
            bones, dt = clip_tracks(ex, v["anims"], c)
            if bones is not None:
                used[out_name] = c
                lengths[out_name] = base_clip(w, names, out_name, dict(scaled(bones, ts), **held), dt,
                                              loop=loop, skip=())
                break
        else:
            raise SystemExit(f"{form} has none of {clips}")
    # A walker's jump and landings, in the same places (car.rs's CAR_JUMP...).
    for out_name, key, loop in WALKER_CLIPS if walks(v) else ():
        clip = (v.get("jump_anims") or {}).get(key)
        bones, dt = clip_tracks(ex, v["anims"], clip) if clip else (None, None)
        if bones is None:
            raise SystemExit(f"{form}: its {key} clip ({clip}) isn't in {v['anims']}")
        used[out_name] = clip
        lengths[out_name] = base_clip(w, names, out_name, dict(scaled(bones, ts), **held), dt, loop=loop, skip=())
    size = w.save(os.path.join(out, f"vehicle_{name}.glb"))
    # Where the effects come from, in the model as main.rs places it (umodel's +X
    # ahead turned to -Z, metres at the mesh set's scale): the hover jets in the
    # hover pose (the wheels turned flat under the car), the rest as it drives.
    to_model = np.diag([1 / VEHICLE_BUILD_SCALE] * 3 + [1.0])
    pts = {}
    for kind in VEHICLE_POINTS:
        G = node_globals(posed(w, "hover" if kind == "hover" else "drive"))
        for sn in effect_sockets(v, kind):
            s = sockets.get(sn)
            if s and s["bone"] in names:
                t, _, _ = gltf_socket(s)
                p = to_model @ G[names[s["bone"]]] @ np.array(t + [1.0])
                pts.setdefault(kind, []).append(p[:3])
    # Where its guns' shots leave from (each gun's two barrels, MuzzleFlash1 and 2,
    # in turn), as it drives with them out; a tank's, the end of its cannon, as it
    # hovers.
    if is_tank(v):
        s = sockets.get(TANK_MUZZLE)
        if s and s["bone"] in names:
            G = node_globals(posed(w, "hover"))
            t, _, _ = gltf_socket(s)
            pts["muzzle"] = [(to_model @ G[names[s["bone"]]] @ np.array(t + [1.0]))[:3]]
    # A jet's rockets: its weapon sockets, by turns, as it hovers.
    if is_plane(v):
        G = node_globals(posed(w, "hover"))
        for sn in PLANE_MUZZLES:
            s = sockets.get(sn)
            if s and s["bone"] in names:
                t, _, _ = gltf_socket(s)
                pts.setdefault("muzzle", []).append((to_model @ G[names[s["bone"]]] @ np.array(t + [1.0]))[:3])
    G = guns_out_globals(w) if not is_tank(v) and not is_plane(v) else None
    for prefix in GUN_PREFIXES if G is not None else ():
        for s in gun_sockets:
            bone = prefix + s["bone"]
            if bone in names:
                t, _, _ = gltf_socket(s)
                p = to_model @ G[names[bone]] @ np.array(t + [1.0])
                pts.setdefault("muzzle", []).append(p[:3])
    if "muzzle" in pts and G is not None:
        # (Left and right gun by turns, as the game fires them: a launcher's one
        # muzzle each.)
        m = pts["muzzle"]
        half = len(m) // 2
        pts["muzzle"] = [x for pair in zip(m[:half], m[half:]) for x in pair]
    pos = np.concatenate([d.acc(p["attributes"]["POSITION"]) for p in d.g["meshes"][0]["primitives"]])
    height = float(pos[:, 1].max() - min(pos[:, 1].min(), 0.0)) * v["scale"]
    # Its body against walls (main.rs): how far it reaches ahead of its origin and
    # behind it, and its half width (umodel's +X ahead, Z to the side), metres.
    body = (float(np.abs(pos[:, 2]).max()) * v["scale"], float(pos[:, 0].max()) * v["scale"],
            float(-pos[:, 0].min()) * v["scale"])
    # A walker (no wheels) stands on its feet where it stands still (its hover pose).
    low, high = vertical_extent(w, d, joints, "hover")
    lift = -low if walks(v) else 0.0
    # A tank floats TANK_CLEARANCE off the ground (its model that much higher than it
    # stands, less how far below its root its hover pose reaches), or stands on its
    # legs where its hover pose has them (Hardshell's beetle).
    float_height = 0.0
    if is_tank(v) and not walks(v):
        float_height = -low if low < -TANK_LEGS else TANK_CLEARANCE - low
    # A jet hovers as high on its jets (the game's jets in its Customize garage float
    # on them as the tanks do; with no gravity, in play it rests on the ground no
    # lower).
    if is_plane(v):
        float_height = TANK_CLEARANCE - low
    print(f"wrote vehicle_{name}.glb ({len(w.g['animations'])} clips, {size / 1e6:.1f} MB)")
    return {"model": f"{model_dir}/vehicle_{name}.glb", "points": pts, "height": height,
            "lift": lift, "clips": used, "body": body, "float_height": float_height, "lengths": lengths}


def hull_line(form, v):
    """A vehicle's vhull line: the box round what of its rigid body strikes the world
    (class_data.physics_hull, at its MeshScale): how far it reaches ahead of its root
    and behind it, its half width and its bottom and top over the root, metres. A jet
    flies and a tank hovers against the map by it (main.rs), the jets' rotors and
    wings no part of it."""
    hull = v.get("hull")
    if not hull:
        return None
    (lo, hi), k = hull, 0.01 * v.get("scale", 1.0)
    vals = (hi[0] * k, -lo[0] * k, max(-lo[1], hi[1]) * k, lo[2] * k, hi[2] * k)
    return "\t".join(["vhull", form] + [f"{x:.3f}" for x in vals])


# Which of each vehicle form's physics blueprints the demo drives by: the game picks
# by TnGame.GetCurrentSkillDataIndex (TnCarForm.OnPostInitialize, and the tanks' and
# jets' alike), which is the game type's GetSkillDataIndex: 0 in a multiplayer
# match, and 1 in Escalation (TnSurvivalGame's, as TnGame's own): the second each
# form lists, Car_Physics and HoverCar_Physics for the Infiltrator's cars rather
# than Car_MP_Physics and HoverCar_MP_Physics.
SKILL_INDEX = 1


def blueprint(names, index=SKILL_INDEX):
    """The physics blueprint a form drives by out of its list (SKILL_INDEX's, else
    the last there is)."""
    names = names or [None]
    return names[min(index, len(names) - 1)]


def handling_line(cls, form, v, info):
    """The vhandling line: how the vehicle drives, by its blueprints (metres and
    seconds): Escalation's, SKILL_INDEX's (Car_Physics and HoverCar_Physics for the
    Infiltrator's cars). A tank only hovers (HoverTank_Physics): boosting is its
    faster hover (MaxBoostSpeed) in place of the wheels, its jump the same either
    way, and it floats its model `float_height` higher than it stands
    (build_vehicle: TANK_CLEARANCE)."""
    if is_plane(v):
        # A jet: flying is boosting (Plane_Physics' MaxSpeed and MaxAcceleration),
        # hovering its hover's (MaxLinearSpeed, MaxLinearAcceleration, DriftDuration);
        # it doesn't jump (Jump/Ascend climbs: plane_line).
        hover = v["handling"].get(blueprint(v["plane_physics"]["hover"])) or {}
        fly = v["handling"].get(blueprint(v["plane_physics"]["fly"])) or {}
        values = [fly.get("MaxSpeed", 4000.0) / 100, fly.get("MaxAcceleration", 2500.0) / 100, 0.0, 0.0, 0.0, 0.0,
                  hover.get("MaxLinearSpeed", 2100.0) / 100, hover.get("MaxLinearAcceleration", 1500.0) / 100,
                  0.0, 0.0, hover.get("DriftDuration", 0.25), 0.0, info["float_height"], 0.0]
        return "\t".join(["vhandling", form] + [f"{x:.4g}" for x in values])
    if is_tank(v):
        tank = v["handling"].get(blueprint(v.get("tank_physics"))) or {}
        jump, kick = tank.get("JumpLinearSpeed", 1200.0) / 100, -tank.get("JumpAngularSpeed", 1.0)
        accel = tank.get("MaxLinearAcceleration", 3000.0) / 100
        values = [tank.get("MaxBoostSpeed", 3500.0) / 100, accel, jump, 0.0, kick, 1.0,
                  tank.get("MaxLinearSpeed", 1500.0) / 100, accel, jump, kick, tank.get("DriftDuration", 0.5),
                  1.0, info["float_height"], 0.0]
        return "\t".join(["vhandling", form] + [f"{x:.4g}" for x in values])
    car = v["handling"].get(blueprint(v["car_physics"])) or {}
    hover = v["handling"].get(blueprint(v["hover_physics"])) or {}
    jump = car.get("jump", (200.0, 0.0, 1500.0))
    wheel = car.get("suspension", {"RestingLength": 35.0, "InAirRestingLength": 70.0})
    values = [
        car.get("MaxSpeed", 3100.0) / 100, car.get("MaxAcceleration", 1200.0) / 100,
        jump[2] / 100, jump[0] / 100, car.get("JumpAngularVelocity", 0.4), car.get("JumpingGravityScale", 1.0),
        hover.get("MaxLinearSpeed", 1500.0) / 100, hover.get("MaxLinearAcceleration", 3000.0) / 100,
        hover.get("JumpLinearSpeed", 1550.0) / 100, -hover.get("JumpAngularSpeed", -5.0),
        hover.get("DriftDuration", 0.5), hover.get("JumpingGravityScale", 1.0),
        hover_rise(car, hover),
        (wheel["InAirRestingLength"] - wheel["RestingLength"]) / 100,
    ]
    return "\t".join(["vhandling", form] + [f"{x:.4g}" for x in values])


# The rigid bodies' gravity (WorldInfo's DefaultGravityZ, RBPhysicsGravityScaling 1)
# and the frame the game's springs are worked out over (its 60 fps), Unreal units.
RB_GRAVITY = 2940.0
PHYSICS_DT = 1.0 / 60.0


def spring_at_rest(mass, suspension):
    """One of a vehicle's TnSuspensions standing still on level ground: its push
    up is a - b * length (TnSpring.Update, its velocity 0: an implicit step of K =
    Stiffness / mass, B = Damping / mass, the vehicle's whole mass, over the frame,
    less gravity's share of it), as (a, b)."""
    k = suspension["Stiffness"] / mass
    d = 1.0 + suspension["Damping"] / mass * PHYSICS_DT + k * PHYSICS_DT * PHYSICS_DT
    return mass * ((k * suspension["RestingLength"] - RB_GRAVITY) / d + RB_GRAVITY), mass * k / d


def hover_rise(car, hover):
    """How much higher a car or truck floats hovering than it stands on its wheels
    (metres, at its origin), where its suspension holds it on level ground: on its
    wheels, its four wheels' springs (TnWheelAssembly: from its wheel mounts, the
    car physics' LocalPosition, down to the ground less the wheel's radius) bear
    its weight and keep its pitch about its centre of mass; hovering, the four
    hover springs (TnHoverCarSimulation: SuspensionRadius round its centre of mass,
    at its height, no wheel) each bear a quarter of it. Both sets work on the same
    rigid body, its mass and centre of mass the car physics' (TnCarSimulation.
    InitializeFromBlueprint; the ChassisOffset moves both alike). Jazz's car floats
    1.5 cm higher than it drives, the trucks 15.5 cm; Swindle's and Hound's, on
    taller wheels, half a metre. Not lower than its wheels (Snarl and Slug walk on
    their legs instead)."""
    if not car.get("wheels") or "suspension" not in hover:
        return 0.0
    mass = car.get("Mass", 1.0)
    a, b = spring_at_rest(mass, hover["suspension"])
    floats = (a - mass * RB_GRAVITY / 4) / b - car["centre"][2]
    # On the wheels: its height at the origin h and pitch t (each wheel's spring as
    # long as h + z + t x - radius) for the springs' sum to bear the weight and
    # their turn about the centre of mass to come to nothing.
    cx = car["centre"][0]
    m = np.zeros((2, 2))
    r = np.zeros(2)
    for w in car["wheels"]:
        wa, wb = spring_at_rest(mass, w["suspension"])
        x, z = w["at"][0], w["at"][2]
        for row, arm in ((0, 1.0), (1, x - cx)):
            m[row] += (-wb * arm, -wb * x * arm)
            r[row] -= (wa - wb * (z - w["radius"])) * arm
    r[0] += mass * RB_GRAVITY
    stands = np.linalg.solve(m, r)[0]
    return max(0.0, (floats - stands) / 100)


def plane_line(data, form, v, info):
    """A jet's vplane line: its hover's climb and dash, its flight's lead into turns,
    how far above the view it hovers pitched, its cameras' fields of view, pitch
    ranges and the flying camera's turn (its class's camera set's: the hovering
    jet's strategy, TnDrivingCameraStrategyType's, and the flying one's,
    TnFlyingCameraStrategyType's), and its clips' lengths taking off, dropping
    back to hovering and crashing (build_vehicle's); metres, seconds and degrees."""
    hover = v["handling"].get(blueprint(v["plane_physics"]["hover"])) or {}
    fly = v["handling"].get(blueprint(v["plane_physics"]["fly"])) or {}
    cams = v.get("camera") or data["robot"]["camera"]
    hover_cam, fly_cam = cams.get("drive") or {}, cams.get("fly") or {}
    follow = fly_cam.get("follow") or class_data.FLYING_FOLLOW_DEFAULTS
    values = [hover.get("VerticalDashSpeed", 1000.0) / 100, hover.get("VerticalDashDuration", 0.1),
              hover.get("DashSpeed", 1000.0) / 100, hover.get("DashDuration", 0.1),
              fly.get("DragCoefficient", 600.0), hover.get("MotionPhysicsScale", 1.0),
              fly.get("PitchDueToPitchValue", 27.0), fly.get("YawDueToYawValue", 16.0),
              fly.get("RollDueToYawValue", 77.0), fly.get("ExtraRotationLerpValue", 0.1),
              fly.get("FullPitchThreshold", 45.0), fly.get("MaxPitchValue", 60.0), v.get("pitch_offset", 0.0),
              hover_cam.get("fov") or 80.0, fly_cam.get("fov") or 90.0]
    values += list(hover_cam.get("pitch_range") or (-45.0, 45.0)) + list(fly_cam.get("pitch_range") or (-80.0, 80.0))
    values += [follow["MaxYawVelocity"], follow["MaxYawAcceleration"], follow["YawDamping"],
               follow["MaxPitchVelocity"], follow["MaxPitchAcceleration"], follow["PitchDamping"],
               follow["RollDueToYawVelocity"]]
    values += [info["lengths"].get(k, 0.5) for k in ("hover_to_drive", "drive_to_hover", "crash")]
    return "\t".join(["vplane", form] + [f"{x:.4g}" for x in values])


# The camera states of the scope (the Nucleon Charge Rifle's, main.rs's
# WeaponStats::scope_camera): its own offsets and orbit in the class's camera.
SCOPE_STATES = ["TnPCS_FineAim", "TnPCS_SniperRifle"]


def by_state(entries, states, default):
    """A camera strategy's value for just these camera states (class_data.
    camera_strategy's entries), or `default`."""
    return next((v for s, v in entries if s == sorted(states)), default)


def camera_line(data):
    """The camera line: the class's camera on foot (its camera set's over-the-shoulder
    strategy): how far out it orbits, the pitch range its aim's offsets (ahead, side,
    up) are spread over and those at its bottom, middle and top, and the scope's
    camera (side, up and back from the middle of the robot; its offsets for the
    scope's camera states at their orbit), metres and degrees. None for the
    Infiltrator, who keeps the camera main.rs has always given Jazz (his own
    campaign one: character.rs's Camera::JAZZ)."""
    cam = data["robot"]["camera"]["robot"]
    if data["class"] == "infiltrator" or not cam or not cam["curve"] or not cam["distance"]:
        return None
    curve = cam["curve"]
    ahead, side, up = (x / 100 for x in by_state(cam["curves"], SCOPE_STATES, curve)["offsets"][0])
    scope_orbit = by_state(cam["distances"], SCOPE_STATES, cam["distance"]) / 100
    return "\t".join(["camera", f"{cam['distance'] / 100:.3f}", f"{curve['pitch'][0]:.1f}", f"{curve['pitch'][1]:.1f}"]
                     + [fmt([x / 100 for x in o]) for o in curve["offsets"]]
                     + [fmt([side, up, scope_orbit - ahead])])


def curve_at(curve, pitch):
    """A TnCameraOffsetCurve's offsets (ahead, side, up) with the view at `pitch`
    (degrees, up), as the game evaluates it (TnScreenSpaceOffsetByPitchCameraBehavior.
    GetCameraOffset, HmCurveVector.Build): its three offsets keyed at 0, a half and 1
    of the way through its pitch range, Hermite between them (CIM_CurveUser), level
    at its ends and through the middle one at half the slope from the first to the
    last (UE3's FInterpCurve tangents, by the keys' spacing)."""
    (lo, hi), o = curve["pitch"], curve["offsets"]
    if hi <= lo:
        return tuple(o[0])
    t = min(max((pitch - lo) / (hi - lo), 0.0), 1.0)
    mid = [(o[2][i] - o[0][i]) * 0.5 * 0.5 for i in range(3)]
    if t < 0.5:
        p0, p1, m0, m1, a = o[0], o[1], [0.0] * 3, mid, t * 2
    else:
        p0, p1, m0, m1, a = o[1], o[2], mid, [0.0] * 3, t * 2 - 1
    h00, h10 = 2 * a ** 3 - 3 * a ** 2 + 1, a ** 3 - 2 * a ** 2 + a
    h01, h11 = -2 * a ** 3 + 3 * a ** 2, a ** 3 - a ** 2
    return tuple(h00 * p0[i] + h10 * m0[i] + h01 * p1[i] + h11 * m1[i] for i in range(3))


def level_offset(cam, states):
    """A camera strategy's aim's offsets (ahead, side, up; Unreal units) with the view
    level, as its offset curve for these camera states (or its default) has them."""
    curve = (by_state(cam["curves"], states, None) if states else None) or cam["curve"]
    if not curve:
        return (0.0, 0.0, 0.0)
    return curve_at(curve, 0.0)


def vehicle_camera_line(data, form, v):
    """A vehicle's vcamera line: how far behind it the camera is and how high over its
    middle it aims (its anchor's offset, and the fixed offset its form's camera state
    adds: Slug's), driving and hovering (its camera set's strategies, with their own
    orbits for that state; Escalation's, class_data.vehicle_camera_set, with its aim's
    offsets for a level view too, and their fields of view), metres and degrees. None
    for the Infiltrator's, which keep the camera main.rs has always given Jazz's car."""
    escalation = bool(v.get("camera"))
    cams = v.get("camera") or data["robot"]["camera"]
    own = [v["pcs"]] if v.get("pcs") else []
    if is_plane(v) and cams.get("drive") and cams.get("fly"):
        # A jet's: flying (its flying strategy) in place of driving and hovering (its
        # driving one, TnDrivingCameraStrategyType's), each its offset behind and its
        # anchor's height, with their own for its form's camera state (Vortex's
        # TnPCS_Chopper); the offset's own height is the anchor's too.
        values = []
        for cam in (cams["fly"], cams["drive"]):
            distance = by_state(cam["distances"], own, cam["distance"]) if own else cam["distance"]
            anchor = by_state(cam.get("anchors", []), own, cam["anchor"]) if own else cam["anchor"]
            lift = by_state(cam.get("lifts", []), own, cam.get("lift", 0.0)) if own else cam.get("lift", 0.0)
            values += [distance / 100, (anchor[2] + lift) / 100]
        return "\t".join(["vcamera", form] + [f"{x:.3f}" for x in values])
    if data["class"] == "infiltrator" or not cams["drive"] or not (cams["hover"] or escalation):
        return None
    values = []
    fovs = []
    # (Boosting is driving: its strategy's own for TnPCS_Boosting, the tanks'. An
    # Escalation tank's set has the one strategy, driving: it hovers by it too.)
    hover = cams["hover"] or cams["drive"]
    for cam, states in ((cams["drive"], own + ["TnPCS_Boosting"]), (hover, own)):
        distance = by_state(cam["distances"], states, None) if states else None
        if distance is None and len(states) > 1:
            distance = by_state(cam["distances"], own, None)
        if distance is None:
            distance = cam["distance"]
        up = cam["anchor"][2]
        curve = by_state(cam["curves"], own, None) if own else None
        if curve and curve["pitch"] == (0.0, 0.0):
            up += curve["offsets"][0][2]
        elif escalation:
            ahead, _, lift = level_offset(cam, states)
            distance -= ahead
            up += lift
        values += [distance / 100, up / 100]
        fovs.append(cam["fov"] or 75.0)
    if escalation:
        values += fovs
    return "\t".join(["vcamera", form] + [f"{x:.3f}" for x in values])


def drives_on_wheels(v):
    """A car or a truck (TnCarFormBlueprint, TnTruckFormBlueprint: TnCarForm's
    hovering and driving), not a tank or a jet."""
    return v.get("kind") in ("TnCarFormBlueprint", "TnTruckFormBlueprint")


# Of a hover car's tilt off level, its hover holds back this share a frame while
# it is on the ground (TnHoverCarSimulation.UpdateTurn: its roll and yaw set to
# turn by 5% of what is between it and its frame of reference, MakeRotator(0, Yaw,
# 0), its pitch left free while it has contact); a tank's, stable on the ground,
# none (TnHoverTankSimulation.UpdateTurn turns only its yaw then).
HOVER_ROLL_HOLD = 0.05


def ground_line(form, v):
    """The vground line: how a car's, truck's or tank's body follows the ground under
    it (src/car.rs's GroundFit), as its rigid body does on its springs. Its four
    wheels (TnWheelAssembly: each a spring from its mount, LocalPosition, down to the
    ground less its radius: TnSuspension.Update's trace) where they are across it
    and ahead of its origin, metres; the pull and damping of those springs against
    its pitch about its centre of mass (Stiffness and Damping at each wheel's
    distance ahead of it, over its pitch inertia: per second squared, per second)
    and against its rise and fall (over its Mass). Then its hover's four springs
    (TnHoverCarSimulation.CalculateSuspensionLocation, TnHoverTankSimulation's: at
    SuspensionRadius round its centre of mass, at 45, 135, 225 and 315 degrees): the
    radius and the centre across and ahead, metres; their pull and damping against
    its pitch, its roll and its rise, as the wheels'; and the share of its roll its
    hover holds level a frame (HOVER_ROLL_HOLD; a tank's none). A tank has no wheels
    and its rigid body's mass is its physics asset's (zeros: car.rs keeps its own
    for it); a jet has no line. Zeros for what a vehicle lacks."""
    if is_plane(v):
        return None
    car = v["handling"].get(blueprint(v.get("car_physics"))) or {}
    hover = v["handling"].get(blueprint(v.get("hover_physics"))) or {}
    tank = None
    if is_tank(v):
        tank = v["handling"].get(blueprint(v.get("tank_physics"))) or {}
        hover = tank
    if car.get("kind") != "car":
        car = {}
    if not car and not hover:
        return None
    wheels = car.get("wheels") or []
    mass = max(car.get("Mass", 1.0), 1.0)
    roll_inertia, pitch_inertia, _ = car.get("inertia", (1.0, 1.0, 1.0))
    centre = car.get("centre", (0.0, 0.0, 0.0))
    values = []
    for w in (wheels + [None] * 4)[:4]:
        values += [w["at"][1] / 100, w["at"][0] / 100] if w else [0.0, 0.0]
    if len(wheels) == 4:
        arms = [(w["at"][0] - centre[0]) ** 2 for w in wheels]
        values += [sum(w["suspension"]["Stiffness"] * a for w, a in zip(wheels, arms)) / max(pitch_inertia, 1.0),
                   sum(w["suspension"]["Damping"] * a for w, a in zip(wheels, arms)) / max(pitch_inertia, 1.0),
                   sum(w["suspension"]["Stiffness"] for w in wheels) / mass,
                   sum(w["suspension"]["Damping"] for w in wheels) / mass]
    else:
        values += [0.0] * 4
    if hover:
        radius = hover.get("SuspensionRadius", 200.0)
        spring = hover.get("suspension") or {}
        values += [radius / 100, centre[1] / 100, centre[0] / 100]
        if car and tank is None:
            # (Four springs at 45 degrees round it: each radius / sqrt(2) ahead or
            # behind and across, their squares summing to twice the radius squared.)
            arm = 2 * radius * radius
            values += [spring["Stiffness"] * arm / max(pitch_inertia, 1.0),
                       spring["Damping"] * arm / max(pitch_inertia, 1.0),
                       spring["Stiffness"] * arm / max(roll_inertia, 1.0),
                       spring["Damping"] * arm / max(roll_inertia, 1.0),
                       4 * spring["Stiffness"] / mass, 4 * spring["Damping"] / mass]
        else:
            values += [0.0] * 6
        values.append(0.0 if tank is not None else HOVER_ROLL_HOLD)
    else:
        values += [0.0] * 10
    return "\t".join(["vground", form] + [f"{x:.5g}" for x in values])


def drive_line(form, v):
    """The vdrive line: how a car or truck drives on its wheels beyond vhandling's,
    by its car physics (Escalation's: SKILL_INDEX's) (TnCarSimulation and its TnWheelAssemblies,
    main.rs's drive_velocity): its AngularDamping; how far ahead of its centre of
    mass its front wheels are and how far behind it its back ones, and the square
    of its turning radius of gyration (its InertiaTensor's yaw over its Mass),
    metres; its front wheels' MaxSteeringAngle (degrees); its front and back tyres'
    TireFrictionCoefficient (per metre a second they slide); its TurningBankValue
    (degrees) and TurningBankSpeed (a frame's share); the pull of its suspension's
    springs and dampers against its roll (four of Stiffness and Damping over the
    whole Mass, as TnSpring has them, at its wheels' track, over its roll inertia:
    per second squared and per second); its air control's turn and strafe
    (AirControlTurnAcceleration, rad/s², AirControlStrafeAcceleration, m/s²); and
    the camera shakes its jumps and landings start (each Intensity, Roll,
    RollFalloffDuration, ShakeFalloffDuration). None for a vehicle without wheels'
    physics."""
    car = v["handling"].get(blueprint(v.get("car_physics"))) or {}
    if car.get("kind") != "car" or not car.get("wheels"):
        return None
    wheels = car["wheels"]
    front = [w for w in wheels if w["steer"] > 0] or wheels[:2]
    rear = [w for w in wheels if w["steer"] <= 0] or wheels[2:]
    mean = lambda xs: sum(xs) / max(1, len(xs))  # noqa: E731
    centre = car["centre"][0]
    ahead = mean([w["at"][0] for w in front]) - centre
    behind = centre - mean([w["at"][0] for w in rear])
    track = mean([abs(w["at"][1]) for w in wheels])
    mass = max(car["Mass"], 1.0)
    roll_inertia, _, yaw_inertia = car["inertia"]
    spring = wheels[0]["suspension"]
    roll_pull = 4 * spring["Stiffness"] / mass * track ** 2 * mass / max(roll_inertia, 1.0)
    roll_damp = 4 * spring["Damping"] / mass * track ** 2 * mass / max(roll_inertia, 1.0)
    values = [car["AngularDamping"], ahead / 100, behind / 100, yaw_inertia / mass / 1e4, front[0]["steer"],
              mean([w["grip"] for w in front]) * 100, mean([w["grip"] for w in rear]) * 100,
              car["TurningBankValue"], car["TurningBankSpeed"], roll_pull, roll_damp,
              car["AirControlTurnAcceleration"], car["AirControlStrafeAcceleration"] / 100]
    for kind in ("jump", "land"):
        s = v["shakes"][kind]
        values += [s["Intensity"], s["Roll"], s["RollFalloffDuration"], s["ShakeFalloffDuration"]]
    return "\t".join(["vdrive", form] + [f"{x:.5g}" for x in values])


# The camera states main.rs's vehicle camera puts the player in (car.rs's
# CamStates), by the game's names; a strategy's entries for any other (fine aim, a
# point of interest, another vehicle's form state) never apply.
CAM_STATES = ["TnPCS_HighSpeed", "TnPCS_TurningLeft", "TnPCS_TurningRight", "TnPCS_Jumping",
              "TnPCS_HoverStrafing", "TnPCS_Boosting", "TnPCS_SuperBoosting"]


def cam_states(states, own):
    """A camera strategy entry's states as a vcam line has them ("-" for none, else
    their names less TnPCS_, comma-joined): the vehicle's own form state (FormPCS)
    left out, as it is always on; None for an entry that can't apply."""
    rest = [s for s in states if s != own]
    if any(s not in CAM_STATES for s in rest):
        return None
    return ",".join(s[len("TnPCS_"):] for s in rest) or "-"


def vehicle_middle(assets, model, lift, v):
    """Where a vehicle's middle is as the demo draws its model (its scale, raised
    `lift`): how high over the ground it stands on and how far ahead of its origin
    (umodel's +X), metres; None when its model can't be read. It is the middle of
    its mesh's bounds as the mesh is made (its reference pose), where the game puts
    the pawn (TnVehicleForm.CalculateCylinderBounds moves the mesh to centre it on
    the pawn's Location), and so where its cameras anchor: driving and hovering at
    the pawn's Location (HmOffsetAnchorPointBaseCameraBehavior.GetOffsetOriginWS,
    TnPlayerPawn.GetCameraAnchorLocationWS: no bone named, no foot offset for a
    vehicle form), flying at its Location too (TnFlyingFollowCameraBehavior.
    UpdateAnchorPoint); not its rigid body's bone, C_Reference_XR, which only its
    death camera anchors on (Death_Vehicle_STRATEGY's BoneName)."""
    path = os.path.join(assets, *model.split("/"))
    try:
        gltf, rest = _read_glb(path)
    except OSError:
        return None
    acc = gltf["accessors"]
    prims = gltf["meshes"][0]["primitives"]
    if all("min" in acc[p["attributes"]["POSITION"]] for p in prims):
        lo = [min(acc[p["attributes"]["POSITION"]]["min"][i] for p in prims) for i in range(3)]
        hi = [max(acc[p["attributes"]["POSITION"]]["max"][i] for p in prims) for i in range(3)]
    else:
        from gltf_io import Doc
        d = Doc(path)
        pos = np.concatenate([d.acc(p["attributes"]["POSITION"]) for p in prims])
        lo, hi = pos.min(0).tolist(), pos.max(0).tolist()
    scale = v.get("scale", 1.0)
    return ((lo[1] + hi[1]) * 0.5 * scale + lift, (lo[0] + hi[0]) * 0.5 * scale)


def middle_line(form, middle):
    """A vehicle's vmiddle line (vehicle_middle's), or None without one."""
    if middle is None:
        return None
    return "\t".join(["vmiddle", form, f"{middle[0]:.3f}", f"{middle[1]:.3f}"])


# vehicle_<name>.glb is built this many times its size, as models/vehicle.glb is
# (main.rs's VEHICLE_SCALE draws it at 1 / this).
VEHICLE_BUILD_SCALE = 2.2


def vehicle_cam_lines(data, form, v, origin, height):
    """A car's or truck's vcam lines: its class's camera set's driving and hovering
    strategies (class_data.camera_detail), each a property by camera state:
      vcam <form> <drive|hover> blend - <secs>        (TransitionTime)
      vcam <form> <..> blur - <amount>                (MotionBlur_Amount, 0 for none)
      vcam <form> <..> anchor - <up> <turns 0/1>      (metres; in the view's axes)
      vcam <form> <..> origin - <metres>              (its middle's height: vehicle_middle)
      vcam <form> <..> pitch - <min> <max>            (degrees)
      vcam <form> <..> match - <rate>                 (MatchRotationSpeed: 0, free)
      vcam <form> <..> smooth - <secs>                (the orbit smoother's)
      vcam <form> <..> dist_secs - <in> <out>         (PCSTransitionInTime, OutTime)
      vcam <form> <..> fov <states> <degrees> <secs>
      vcam <form> <..> dist <states> <metres>
      vcam <form> <..> curve <states> <ahead side up>x3 <pitch min> <pitch max> <secs>
      vcam <form> <..> roll <states> <degrees> <secs>
      vcam <form> <..> wiggle <states> <yaw pitch roll amplitudes> <their frequencies> <secs>
    each list in the game's order, its default last (states "-")."""
    det = v.get("camera_detail") or data["robot"].get("camera_detail") or {}
    own = v.get("pcs")
    lines = []
    for which in ("drive", "hover"):
        cam = det.get(which)
        if not cam:
            continue

        def line(prop, states, *values):
            if states is not None:
                lines.append("\t".join(["vcam", form, which, prop, states] + [f"{x:.5g}" for x in values]))
        line("blend", "-", cam["blend"])
        line("blur", "-", cam["blur"])
        line("anchor", "-", cam["anchor"] / 100, 1.0 if cam["anchor_turns"] else 0.0)
        line("origin", "-", origin if origin is not None else height * 0.5)
        line("pitch", "-", *cam["pitch_range"])
        line("match", "-", cam["match"])
        line("smooth", "-", cam["smooth"])
        line("dist_secs", "-", *cam["distance_secs"])
        for states, fov, secs in cam["fov"]:
            line("fov", cam_states(states, own), fov, secs)
        for states, d in cam["distance"]:
            line("dist", cam_states(states, own), d / 100)
        for states, c in cam["curve"]:
            line("curve", cam_states(states, own), *[x / 100 for o in c["offsets"] for x in o], *c["pitch"], c["secs"])
        for states, deg, secs in cam["roll"]:
            line("roll", cam_states(states, own), deg, secs)
        for states, amp, hz, secs in cam["wiggle"]:
            line("wiggle", cam_states(states, own), *amp, *hz, secs)
    return lines


def vfx_lines(form, v):
    """A vehicle's vfx lines (TANK_FX or CAR_FX: its effects on their sockets, those
    whose sockets its mesh has), and a car's or truck's vfxland."""
    kinds = TANK_FX if is_tank(v) or is_plane(v) else CAR_FX if drives_on_wheels(v) else ()
    by_socket = {s["name"]: s for s in v["sockets"]}
    lines = []
    for kind, key in kinds:
        for socket, system in v["fx"].get(key, []):
            s = by_socket.get(socket)
            if s and system:
                t, _, _ = gltf_socket(s)
                lines.append("\t".join(["vfx", form, kind, system, s["bone"], fmt(t),
                                        " ".join(str(int(r)) for r in s["rotation"]),
                                        " ".join(f"{x:g}" for x in s.get("scale", (1.0, 1.0, 1.0)))]))
    if lines and drives_on_wheels(v):
        lines.append("\t".join(["vfxland", form, f"{v.get('low_landing_air', 1.0):g}"]))
    return lines


def vehicle_audio_lines(form, v, used, class_events):
    """A vehicle's transformations' footsteps (vtstep lines: to the vehicle or back,
    seconds into its clips, the robot's sound event, where his class's sound event
    set, `class_events`, gives it a cue: his landing as he comes out of a jet, his
    jump into a car), its clips' sounds (vcsound lines: its looping clips', by the
    game clip each of VEHICLE_CLIPS is, `used`; and at WALKER_FIRST_CLIP on a
    walker's jump's and landings': a Dinobot's stamps as it lands) and the sound
    events its audio component plays (its vaudio line: car.rs and main.rs play no
    other of its engine's, boosts', jumps' and landings' events: a Dinobot has none
    of them but Grimlock's boost start)."""
    lines = []
    c = v.get("clips") or {}
    for kind, keys in (("to_vehicle", ("robot_to_vehicle", "car_to_vehicle")),
                       ("to_robot", ("robot_to_robot", "car_to_robot"))):
        for t, event in sorted(s for k in keys for s in (c.get(k) or {}).get("steps", [])):
            if event in class_events:
                lines.append("\t".join(["vtstep", form, kind, f"{t:.4f}", event]))
    for index, (out_name, _, loop) in enumerate(VEHICLE_CLIPS):
        game_clip = used.get(out_name)
        if loop and game_clip:
            for t, cue in v.get("clip_sounds", {}).get(game_clip, []):
                lines.append("\t".join(["vcsound", form, str(index), f"{t:.4f}", cue]))
    if walks(v):
        for index, (_, key, _) in enumerate(WALKER_CLIPS):
            game_clip = (v.get("jump_anims") or {}).get(key)
            for t, cue in v.get("clip_sounds", {}).get(game_clip, []) if game_clip else ():
                lines.append("\t".join(["vcsound", form, str(WALKER_FIRST_CLIP + index), f"{t:.4f}", cue]))
    if v.get("audio_events") is not None:
        lines.append("\t".join(["vaudio", form, ",".join(v["audio_events"]) or "-"]))
    return lines


def used_loop_clips(mp, v):
    """The game clips a vehicle model's looping VEHICLE_CLIPS are (build_vehicle's
    choice: the first of each one's candidates its animation sets have), by name."""
    used = {}
    for out_name, clips, loop in VEHICLE_CLIPS:
        if is_plane(v) and out_name == "hover":
            clips = ["Nav_Hover_Pose_VEH", "Nav_Hover_VEH"]
        for c in clips:
            if any(mp.sequence(s, c) for s in v["anims"]):
                used[out_name] = c
                break
    return used


def sounds_pass(cls, cooked, transgame, assets):
    """--sounds: brings a class already built up to date with its vehicles'
    transformations' footsteps, clip sounds and audio components' events, straight
    from CookedPC, without building it again: rewrites <class>.txt's vtstep, vcsound
    and vaudio lines (after each vehicle's vsound and vtsound lines)."""
    data = class_data.read(cooked, transgame, cls)
    mp = data["mp"]
    by_form = {p.split(".")[-1]: v for p, v in data["vehicles"].items() if v}
    path = os.path.join(assets, "models", cls, f"{cls}.txt")
    lines = open(path, encoding="utf-8").read().splitlines()
    sound_kinds = ("vsound", "vtsound")
    kept = [line for line in lines if line.split("\t")[0] not in ("vaudio", "vcsound", "vtstep")]
    out = []
    count = 0
    for i, line in enumerate(kept):
        out.append(line)
        f = line.split("\t")
        nxt = kept[i + 1].split("\t") if i + 1 < len(kept) else [""]
        if f[0] in sound_kinds and not (nxt[0] in sound_kinds and nxt[1] == f[1]) and f[1] in by_form:
            v = by_form[f[1]]
            extra = vehicle_audio_lines(f[1], v, used_loop_clips(mp, v), data["class_events"])
            count += len(extra)
            out += extra
    with open(path, "w", encoding="utf-8") as fh:
        fh.write("\n".join(out) + "\n")
    print(f"wrote {cls}.txt ({count} vehicle audio lines)")


def fx_pass(cls, cooked, transgame, assets):
    """--fx: brings a class already built up to date with its vehicles' effects,
    straight from CookedPC, without building it again: rewrites <class>.txt's vfx and
    vfxland lines (before each vehicle's vbody line)."""
    data = class_data.read(cooked, transgame, cls)
    by_form = {p.split(".")[-1]: v for p, v in data["vehicles"].items() if v}
    path = os.path.join(assets, "models", cls, f"{cls}.txt")
    lines = open(path, encoding="utf-8").read().splitlines()
    out = []
    count = 0
    for line in lines:
        f = line.split("\t")
        if f[0] in ("vfx", "vfxland"):
            continue
        v = by_form.get(f[1]) if len(f) > 1 else None
        if f[0] == "vbody" and v:
            fx = vfx_lines(f[1], v)
            count += len(fx)
            out += fx
        out.append(line)
    with open(path, "w", encoding="utf-8") as fh:
        fh.write("\n".join(out) + "\n")
    print(f"wrote {cls}.txt ({count} effect lines)")


def decals_pass(cls, cooked, transgame, assets):
    """--decals: brings a class already built up to date with its vehicles' decal
    sockets, straight from CookedPC, without building it again: rewrites
    <class>.txt's vdecal lines (before each vehicle's vbody line)."""
    data = class_data.read(cooked, transgame, cls)
    by_form = {p.split(".")[-1]: v for p, v in data["vehicles"].items() if v}
    path = os.path.join(assets, "models", cls, f"{cls}.txt")
    lines = open(path, encoding="utf-8").read().splitlines()
    out = []
    count = 0
    for line in lines:
        f = line.split("\t")
        if f[0] == "vdecal":
            continue
        v = by_form.get(f[1]) if len(f) > 1 else None
        if f[0] == "vbody" and v:
            decals = decal_lines(f[1], v)
            count += len(decals)
            out += decals
        out.append(line)
    with open(path, "w", encoding="utf-8") as fh:
        fh.write("\n".join(out) + "\n")
    print(f"wrote {cls}.txt ({count} decal sockets)")


def drive_pass(cls, cooked, transgame, assets):
    """--drive: brings a class already built up to date with how its vehicles drive
    and how its camera follows its cars and trucks, straight from CookedPC, without
    building it again: rewrites <class>.txt's vhandling lines (every vehicle's, by
    the blueprints SKILL_INDEX picks; a tank's or jet's float height kept), its
    jets' vplane lines (their clips' lengths kept), its cars' and trucks' vdrive
    and vcam lines and its cars', trucks' and tanks' vground lines (after each one's
    vbody line) and its vcamera lines."""
    data = class_data.read(cooked, transgame, cls)
    by_form = {p.split(".")[-1]: v for p, v in data["vehicles"].items() if v}
    path = os.path.join(assets, "models", cls, f"{cls}.txt")
    lines = open(path, encoding="utf-8").read().splitlines()
    models = {}
    middles = {}
    out = []
    for line in lines:
        f = line.split("\t")
        if f[0] in ("vdrive", "vcam", "vmiddle", "vground"):
            continue
        v = by_form.get(f[1]) if len(f) > 1 else None
        if f[0] == "vehicle":
            models[f[1]] = (f[2], float(f[16]), float(f[14]))
            if v:
                middles[f[1]] = vehicle_middle(assets, f[2], float(f[16]), v)
        if f[0] == "vhandling" and v:
            line = handling_line(cls, f[1], v, {"float_height": 0.0 if drives_on_wheels(v) else float(f[14])})
        if f[0] == "vplane" and v and is_plane(v):
            ends = [float(x) for x in f[-3:]]
            line = plane_line(data, f[1], v, {"lengths": dict(zip(("hover_to_drive", "drive_to_hover", "crash"), ends))})
        if f[0] == "vcamera" and v:
            line = vehicle_camera_line(data, f[1], v)
            if not line:
                continue
        out.append(line)
        if f[0] == "vbody" and v:
            middle = middles.get(f[1])
            line = middle_line(f[1], middle)
            if line:
                out.append(line)
        if f[0] == "vbody" and v and drives_on_wheels(v):
            model, lift, height = models[f[1]]
            middle = middles.get(f[1])
            drive = drive_line(f[1], v)
            if drive:
                out.append(drive)
            ground = ground_line(f[1], v)
            if ground:
                out.append(ground)
            out += vehicle_cam_lines(data, f[1], v, middle[0] if middle else None, height)
        elif f[0] == "vbody" and v:
            ground = ground_line(f[1], v)
            if ground:
                out.append(ground)
    with open(path, "w", encoding="utf-8") as fh:
        fh.write("\n".join(out) + "\n")
    print(f"wrote {cls}.txt ({len(out)} lines)")


def ground_pass(cls, cooked, transgame, assets):
    """--ground: gives a class already built how its cars', trucks' and tanks' bodies
    follow the ground (ground_line), straight from CookedPC, without building it
    again: rewrites <class>.txt's vground lines, each after its vehicle's vdrive line
    (a tank's after its vbody and vmiddle lines), the rest as it was."""
    data = class_data.read(cooked, transgame, cls)
    by_form = {p.split(".")[-1]: v for p, v in data["vehicles"].items() if v}
    path = os.path.join(assets, "models", cls, f"{cls}.txt")
    lines = [line for line in open(path, encoding="utf-8").read().splitlines()
             if not line.startswith("vground\t")]
    wheeled = {line.split("\t")[1] for line in lines if line.startswith("vdrive\t")}
    out = []
    pending = None
    for line in lines:
        f = line.split("\t")
        if pending and not (f[0] == "vmiddle" and f[1] == pending[0]):
            out.append(pending[1])
            pending = None
        out.append(line)
        v = by_form.get(f[1]) if len(f) > 1 else None
        if not v:
            continue
        ground = ground_line(f[1], v)
        if not ground:
            continue
        if f[0] == "vdrive":
            out.append(ground)
        elif f[0] == "vbody" and f[1] not in wheeled:
            pending = (f[1], ground)
        elif f[0] == "vmiddle" and pending and pending[0] == f[1]:
            out.append(pending[1])
            pending = None
    if pending:
        out.append(pending[1])
    with open(path, "w", encoding="utf-8") as fh:
        fh.write("\n".join(out) + "\n")
    print(f"wrote {cls}.txt ({sum(1 for line in out if line.startswith('vground'))} vground lines)")


def interiors_pass(cls, cooked, assets):
    """--interiors: gives a class already built its vehicles' insides (the undersides
    of their panels as they change form): each vehicle model's material drawn as
    glass for want of a texture set (GLASS: VH_Interior_MAT and the like) is made
    again as Materials.get makes it now, for the mesh that uses it
    (GameMaterials.for_mesh, the pictures and plain materials from CookedPC), its
    textures written to models/<class>/textures; the rest of the model as it was."""
    import shutil
    import tempfile
    out = os.path.join(assets, "models", cls)
    mp = class_data.Mp(cooked, class_data.MP_PACKAGE)
    textures = Textures(None, os.path.join(out, "textures"), os.path.join(assets, "models", "vehicle.glb"), cooked)
    game = GameMaterials(mp)
    # (The textures are written aside, and only those the class hasn't yet are put
    # in its textures: one already there, another model's, stays as it is.)
    aside = tempfile.mkdtemp()
    for f in sorted(os.listdir(out)):
        if not (f.startswith("vehicle_") and f.endswith(".glb")):
            continue
        path = os.path.join(out, f)
        g, rest = _read_glb(path)
        users = {}
        for mesh in g.get("meshes", []):
            for p in mesh["primitives"]:
                users.setdefault(p.get("material"), set()).add(mesh.get("name"))
        changed = []
        for mi, m in enumerate(g.get("materials", [])):
            if m.get("pbrMetallicRoughness") != GLASS["pbrMetallicRoughness"]:
                continue
            meshes = users.get(mi, set())
            if len(meshes) != 1:
                print(f"  ! {f}: {m.get('name')} is on {len(meshes)} meshes: left as it is")
                continue
            mesh = next(iter(meshes))
            name = m.get("name")
            if name not in game:
                game[name] = mp.material(name) if mp.by_name.get(name) else None
            w = Writer(image_dir=aside, image_uri="textures")
            made = w.g["materials"][Materials(w, textures, game).get(name, mesh)]
            if made.get("pbrMetallicRoughness") == GLASS["pbrMetallicRoughness"]:
                continue
            for image in w.g["images"]:
                file = image["uri"].split("/")[-1]
                if not os.path.exists(os.path.join(out, "textures", file)):
                    shutil.copy(os.path.join(aside, file), os.path.join(out, "textures", file))
            # Its textures into the model (their images by file, the sampler shared).
            for key in ("samplers", "textures", "images"):
                g.setdefault(key, [])

            def graft(ref):
                t = w.g["textures"][ref["index"]]
                image = {k: v for k, v in w.g["images"][t["source"]].items()}
                if image in g["images"]:
                    source = g["images"].index(image)
                else:
                    g["images"].append(image)
                    source = len(g["images"]) - 1
                sampler = w.g["samplers"][t["sampler"]]
                if sampler not in g["samplers"]:
                    g["samplers"].append(sampler)
                tex = {"source": source, "sampler": g["samplers"].index(sampler)}
                if tex not in g["textures"]:
                    g["textures"].append(tex)
                return dict(ref, index=g["textures"].index(tex))
            pbr = made.get("pbrMetallicRoughness", {})
            for k in ("baseColorTexture", "metallicRoughnessTexture"):
                if k in pbr:
                    pbr[k] = graft(pbr[k])
            for k in ("normalTexture", "emissiveTexture"):
                if k in made:
                    made[k] = graft(made[k])
            g["materials"][mi] = made
            changed.append(f"{name} -> {made['name']}")
        if changed:
            _write_glb(path, g, rest)
            print(f"wrote {f}: {', '.join(changed)}")
    shutil.rmtree(aside, ignore_errors=True)


def transforms_pass(cls, assets):
    """--transforms: brings a class already built up to date with where its cars are
    drawn as they change form (transform_fit.py), from its clips.glb and vehicle
    models as built: rewrites <class>.txt's vtransform lines (after each vehicle
    line)."""
    path = os.path.join(assets, "models", cls, f"{cls}.txt")
    lines = open(path, encoding="utf-8").read().splitlines()
    scale = float(next(l for l in lines if l.startswith("class\t")).split("\t")[2])
    robot = transform_fit.Clips(os.path.join(assets, "models", cls, "clips.glb"), scale)
    out = []
    for line in lines:
        f = line.split("\t")
        if f[0] == "vtransform":
            continue
        out.append(line)
        if f[0] == "vehicle":
            out.append(transform_fit.line(assets, robot, f))
            print(out[-1])
    with open(path, "w", encoding="utf-8") as fh:
        fh.write("\n".join(out) + "\n")
    print(f"wrote {cls}.txt ({len(out)} lines)")


class GameMaterials(dict):
    """The game's materials by name ({name: mp.material(name), None where the package
    hasn't it}), and by mesh (for_mesh) where several of the package's materials
    share a name: VH_Interior_MAT is Swoop's own material and eleven vehicles'
    instances of Interior_DIF__TEXSET, the first of which drew them all as glass."""

    def __init__(self, mp):
        super().__init__()
        self.mp = mp
        self.by_mesh = {}

    def for_mesh(self, name, mesh):
        if mesh is None or len(self.mp.by_name.get(name, [])) < 2:
            return self.get(name)
        if (name, mesh) not in self.by_mesh:
            self.by_mesh[(name, mesh)] = self.mp.mesh_material(mesh, name)
        return self.by_mesh[(name, mesh)]


def exported_materials(ex, mp):
    """Every material any exported mesh uses: {name: mp.material(name), None where
    the package hasn't it} (by mesh too: GameMaterials)."""
    game_mats = GameMaterials(mp)
    for kind in ("SkeletalMesh3", "StaticMesh3"):
        for p in ex.pkgs:
            folder = os.path.join(ex.root, p, kind)
            if os.path.isdir(folder):
                for f in os.listdir(folder):
                    if f.endswith(".gltf"):
                        for m in json.load(open(os.path.join(folder, f))).get("materials", []):
                            if m["name"] not in game_mats:
                                game_mats[m["name"]] = mp.material(m["name"]) if mp.by_name.get(m["name"]) else None
    return game_mats


def parts_pass(cls, export_dir, cooked, transgame, assets):
    """--parts: brings a class already built up to date with its parts: its
    parts.glb again (and the textures it uses) and its file's part lines (in their
    place, the rest of the file as it was)."""
    out = os.path.join(assets, "models", cls)
    ex = Exports(export_dir)
    print("reading the game's data")
    data = class_data.read(cooked, transgame, cls)
    mp = data.pop("mp")
    textures = Textures(ex, os.path.join(out, "textures"), os.path.join(assets, "models", "vehicle.glb"), cooked)
    build_parts(ex, data, textures, exported_materials(ex, mp), out)
    path = os.path.join(out, f"{cls}.txt")
    old = open(path, encoding="utf-8").read().splitlines()
    lines = [part_line(p, data) for p in data["parts"]]
    at = next((i for i, l in enumerate(old) if l.startswith("part\t")), len(old))
    rest = [l for l in old if not l.startswith("part\t")]
    new = rest[:at] + lines + rest[at:]
    with open(path, "w", encoding="utf-8") as fh:
        fh.write("\n".join(new) + "\n")
    print(f"wrote {cls}.txt ({len(lines)} part lines, {len(new)} lines)")


def main(cls, export_dir, cooked, transgame, assets):
    model_dir = f"models/{cls}"
    out = os.path.join(assets, *model_dir.split("/"))
    os.makedirs(out, exist_ok=True)
    ex = Exports(export_dir)
    print("reading the game's data")
    data = class_data.read(cooked, transgame, cls)
    mp = data.pop("mp")
    game_mats = exported_materials(ex, mp)
    textures = Textures(ex, os.path.join(out, "textures"), os.path.join(assets, "models", "vehicle.glb"), cooked)
    ref_path = build_parts(ex, data, textures, game_mats, out)
    robot_glb = os.path.join(assets, "models", "robot.glb")
    clips = build_clips(ex, data, robot_glb, ref_path, out)
    shared_path = os.path.join(cooked, class_data.SHARED_PACKAGE)
    shared = class_data.Mp(cooked, class_data.SHARED_PACKAGE) if os.path.exists(shared_path) else None
    overrides, timings, clip_sounds, cust_weapon = build_robot_clips(ex, data, robot_glb, ref_path, out,
                                                                     (mp, shared))
    # The vehicles' weapon meshes (the car guns, or the trucks' rocket launchers)
    # and their muzzles (their meshes' MuzzleFlash sockets).
    weapons = data["vehicle_weapons"]
    guns = ROCKET_LAUNCHERS if ROCKETS in weapons else CAR_GUNS
    gun_sockets = []
    for pkg in (mp, shared):
        gun = (pkg.by_name.get(guns["mesh"]) or [0])[0] if pkg else 0
        if gun:
            gun_sockets = [s for s in pkg.sockets(gun) if s["name"].startswith("MuzzleFlash")]
            break
    name = {"infiltrator": "Infiltrator", "destroyer": "Destroyer", "titan": "Titan", "scientist": "Scientist"}[cls]
    lines = [f"# The {name}'s characters, made by tools/build_class.py (see there)", class_line(data, timings)]
    camera = camera_line(data)
    if camera:
        lines.append(camera)
    for weapon in weapons:
        lines.append("\t".join(["vweapon", weapon]))
    for event, cue in sorted(data["class_events"].items()):
        if event.startswith(ROBOT_SOUND_EVENTS):
            lines.append("\t".join(["csound", event, cue]))
    for number, clip in sorted(overrides.items()):
        lines.append("\t".join(["robotclip", str(number), str(clip)]))
    for number, cues in sorted(clip_sounds.items()):
        for t, cue in cues:
            lines.append("\t".join(["rcsound", str(number), f"{t:.4f}", cue]))
    if cust_weapon is not None:
        lines.append("\t".join(["custpose", "weapon", str(cust_weapon)]))
    for p in data["parts"]:
        lines.append(part_line(p, data))
    for s in data["sets"]:
        lines.append("\t".join(["set", s["id"], s["name"], str(int(s["dlc"])), ",".join(s["parts"])]))
    for mesh, m in sorted(data["meshes"].items()):
        for s in m["sockets"]:
            t, q, sc = gltf_socket(s)
            lines.append("\t".join(["socket", mesh, s["name"], s["bone"], fmt(t), fmt(q), fmt(sc)]))
    attach = attach_sockets(data)
    # (Where each car is drawn as it changes form: transform_fit.py, from the clips
    # just written.)
    robot_fit = transform_fit.Clips(os.path.join(out, "clips.glb"), float(lines[1].split("\t")[2]))
    for form, v in vehicles_in_order(data):
        info = build_vehicle(ex, form, v, textures, game_mats, out, model_dir, gun_sockets, guns)
        to_vehicle, to_robot, (rv_len, rr_len) = clips["vehicles"][form]
        c = v["clips"]
        rv, rr, cv, cr = c["robot_to_vehicle"], c["robot_to_robot"], c["car_to_vehicle"], c["car_to_robot"]
        radius = v["visual_wheels"][0]["radius"] * 0.01 * v["scale"] if v["visual_wheels"] else 0.0
        legs = int(walks(v))
        lines.append("\t".join(["vehicle", form, info["model"], str(to_vehicle), str(to_robot),
                                f"{rv['toggle'] or rv_len:.4f}", f"{rv_len:.4f}", f"{rr['toggle'] or 0.0:.4f}",
                                f"{rr_len:.4f}", f"{cv['toggle'] or 0.0:.4f}", f"{cv['length']:.4f}",
                                f"{cr['toggle'] or cr['length']:.4f}", f"{cr['length']:.4f}", f"{radius:.4f}",
                                f"{info['height']:.4f}", str(legs), f"{info['lift']:.4f}"]))
        lines.append(transform_fit.line(assets, robot_fit, lines[-1].split("\t")))
        lines.append(handling_line(cls, form, v, info))
        muzzle = next((s for s in v["sockets"] if s["name"] == TANK_MUZZLE), None)
        if is_tank(v) and muzzle:
            t, q, sc = gltf_socket(muzzle)
            lines.append("\t".join(["vtank", form, TANK_CANNON_BONE, muzzle["bone"], fmt(t), fmt(q), fmt(sc)]))
        if is_plane(v):
            lines.append(plane_line(data, form, v, info))
        lines += vfx_lines(form, v)
        lines.append("\t".join(["vbody", form] + [f"{x:.3f}" for x in info["body"]]))
        middle = vehicle_middle(assets, info["model"], info["lift"], v)
        line = middle_line(form, middle)
        if line:
            lines.append(line)
        hull = hull_line(form, v)
        if hull:
            lines.append(hull)
        ground = ground_line(form, v)
        if drives_on_wheels(v):
            drive = drive_line(form, v)
            if drive:
                lines.append(drive)
            if ground:
                lines.append(ground)
            lines += vehicle_cam_lines(data, form, v, middle[0] if middle else None, info["height"])
        elif ground:
            lines.append(ground)
        camera = vehicle_camera_line(data, form, v)
        if camera:
            lines.append(camera)
        for s in part_sockets(v, attach):
            t, q, sc = gltf_socket(s)
            lines.append("\t".join(["vsocket", form, s["name"], s["bone"], fmt(t), fmt(q), fmt(sc)]))
        lines += decal_lines(form, v)
        for kind, pts in info["points"].items():
            for p in pts:
                lines.append("\t".join(["vpoint", form, kind, fmt(p)]))
        sounds = dict(v["sound_events"])
        if v.get("death_cue"):
            sounds["Death"] = v["death_cue"]
        for event, cue in sorted(sounds.items()):
            lines.append("\t".join(["vsound", form, event, cue]))
        for kind, clips_ in (("to_vehicle", (rv, cv)), ("to_robot", (rr, cr))):
            for t, cue in sorted(s for c in clips_ for s in (c or {}).get("sounds", [])):
                lines.append("\t".join(["vtsound", form, kind, f"{t:.4f}", cue]))
        lines += vehicle_audio_lines(form, v, info["clips"], data["class_events"])
    for s, clip in sorted(clips["arms"].items()):
        lines.append("\t".join(["armset", s, str(clip)]))
        for e in data["arm_sets"][s]["switch"]:
            lines.append("\t".join(["switch", s, e["bone"], f"{e['time']:.4f}", f"{e['start']:.4f}",
                                    f"{e['end']:.4f}", f"{e['ramp']:.4f}"]))
    for s, (mirror, equip) in sorted(clips["shoulders"].items()):
        lines.append("\t".join(["shoulderset", s, str(mirror), str(equip)]))
    # (robot.glb's guns are every class's: their paint goes with the Infiltrator's.)
    for material, texset in (GUN_ARM,) + WEAPON_PAINT if cls == "infiltrator" else ():
        t = textures.texset(texset, cooked=texset != GUN_ARM[1])
        if t:
            os.makedirs(os.path.join(out, "textures"), exist_ok=True)
            with open(os.path.join(out, "textures", texset + "_c.ktx2"), "wb") as fh:
                fh.write(t["c"])
            lines.append("\t".join(["gunpaint", material, f"{model_dir}/textures/{texset}_c.ktx2"]))
    with open(os.path.join(out, f"{cls}.txt"), "w", encoding="utf-8") as fh:
        fh.write("\n".join(lines) + "\n")
    print(f"wrote {cls}.txt ({len(lines)} lines)")


# ---------------------------------------------------------------------------
# The glow pass (--glow)
# ---------------------------------------------------------------------------

class CookedTextures:
    """Texture2Ds read straight from CookedPC: the first of GLOW_PACKAGES that has
    one (its full-size mips from the packages next to it), opened as needed."""

    def __init__(self, cooked):
        import texture as ue3_texture
        self.T = ue3_texture
        self.cooked = cooked
        self.pkgs = []
        self.todo = list(GLOW_PACKAGES)

    def get(self, name):
        """Texture2D `name`'s largest mip (an RGBA array), or None."""
        i = 0
        while True:
            if i == len(self.pkgs):
                if not self.todo:
                    return None
                path = os.path.join(self.cooked, self.todo.pop(0))
                if os.path.exists(path):
                    p = self.T._package(path)
                    self.pkgs.append((p, {e["name"]: k + 1 for k, e in enumerate(p.exports)
                                          if p.class_name(e) == "Texture2D"}))
                continue
            p, index = self.pkgs[i]
            if name in index:
                return np.asarray(self.T.read_texture(p, index[name], self.cooked)[0])
            i += 1


def _read_glb(path):
    """A .glb's glTF (dict) and everything after its JSON chunk (the binary chunk)."""
    import struct
    d = open(path, "rb").read()
    n = struct.unpack_from("<I", d, 12)[0]
    return json.loads(d[20:20 + n]), d[20 + n:]


def _write_glb(path, g, rest):
    import struct
    js = json.dumps(g, separators=(",", ":")).encode()
    js += b" " * (-len(js) % 4)
    body = struct.pack("<II", len(js), 0x4E4F534A) + js + rest
    open(path, "wb").write(struct.pack("<III", 0x46546C67, 2, 12 + len(body)) + body)


def glow_pass(cls, cooked, assets):
    """Gives a class already built the glow: writes each painted material's glow
    texture (see GLOW_COVERAGE) from CookedPC over the one its models use, and
    points the materials that glow and had none at theirs."""
    import fxmat
    out = os.path.join(assets, "models", cls)
    tex = CookedTextures(cooked)
    lib = fxmat.Library(cooked, list(GLOW_PACKAGES[:2]))
    enc = Textures(None, None, None)
    pictures = {}

    def picture(name):
        if name not in pictures:
            m = tex.get(name + "_Masks_NormY")
            if m is not None:
                pictures[name] = glow_picture(*glow_masks(m[..., 0].astype(np.float32) / 255))
            else:
                # A material of separate pictures: its GlowA_Coeff_GlowB.
                try:
                    _, params = lib.material(name)
                    ref = params["texture"].get(PLAIN_TEXTURES["glow"])
                    g = tex.get(ref[0].obj_name(ref[1])) if ref else None
                except fxmat.CompileError:
                    g = None
                if g is None:
                    print("  ! no glow source for", name)
                    pictures[name] = None
                else:
                    g = g.astype(np.float32) / 255
                    pictures[name] = glow_picture(srgb_to_linear(g[..., 0]), srgb_to_linear(g[..., 2]))
        return pictures[name]

    written = set()
    counts = {"rewritten": 0, "added": 0, "dark": 0}
    for f in sorted(os.listdir(out)):
        if not f.endswith(".glb") or "clips" in f:
            continue
        path = os.path.join(out, f)
        g, rest = _read_glb(path)
        changed = False
        for m in g.get("materials", []):
            if not (m.get("extras") or {}).get("paint"):
                continue
            pic = picture(m["name"])
            e = m.get("emissiveTexture")
            if e is None and pic is None:
                continue
            if e is None:
                uri = f"textures/{m['name']}_e.ktx2"
                images = g.setdefault("images", [])
                images.append({"uri": uri, "name": m["name"] + "_e"})
                base = m.get("pbrMetallicRoughness", {}).get("baseColorTexture", {}).get("index")
                sampler = g["textures"][base].get("sampler", 0) if base is not None else 0
                g["textures"].append({"source": len(images) - 1, "sampler": sampler})
                m["emissiveTexture"] = {"index": len(g["textures"]) - 1}
                counts["added"] += 1
            else:
                uri = g["images"][g["textures"][e["index"]]["source"]]["uri"]
                counts["rewritten" if pic is not None else "dark"] += 1
            m["emissiveFactor"] = [1, 1, 1]
            ext = m.get("extensions", {})
            ext.pop("KHR_materials_emissive_strength", None)
            if not ext:
                m.pop("extensions", None)
            changed = True
            if uri not in written:
                written.add(uri)
                pixels = pic if pic is not None else np.zeros((4, 4, 3), np.uint8)
                open(os.path.join(out, *uri.split("/")), "wb").write(enc._encode(pixels, "color"))
        if changed:
            if not any("KHR_materials_emissive_strength" in m.get("extensions", {}) for m in g["materials"]):
                for key in ("extensionsUsed", "extensionsRequired"):
                    if key in g:
                        g[key] = [x for x in g[key] if x != "KHR_materials_emissive_strength"]
                        if not g[key]:
                            del g[key]
            _write_glb(path, g, rest)
            print("  wrote", f)
    print(f"{cls}: {counts['rewritten']} glow textures rewritten, {counts['added']} added, "
          f"{counts['dark']} left dark; {len(written)} files")


def robot_clips_pass(cls, export_dir, cooked, transgame, assets):
    """For a class already built, after robot.glb has gained clips (ROBOT_SLOTS'
    later ones: the heavy weapons' stance) or the class its own melee (the
    Infiltrator's sword swing): its robot_clips.glb again (and the mirrored one), its
    file's lines that name them (robotclip, rcsound, custpose) and its class line's
    melee seconds, the rest of the file as it was."""
    out = os.path.join(assets, "models", cls)
    ex = Exports(export_dir)
    print("reading the game's data")
    data = class_data.read(cooked, transgame, cls)
    mp = data.pop("mp")
    ref_path = reference_skeleton(ex, data)
    shared_path = os.path.join(cooked, class_data.SHARED_PACKAGE)
    shared = class_data.Mp(cooked, class_data.SHARED_PACKAGE) if os.path.exists(shared_path) else None
    overrides, timings, clip_sounds, cust_weapon = build_robot_clips(
        ex, data, os.path.join(assets, "models", "robot.glb"), ref_path, out, (mp, shared))
    lines = [f"\t".join(["robotclip", str(n), str(c)]) for n, c in sorted(overrides.items())]
    for number, cues in sorted(clip_sounds.items()):
        for t, cue in cues:
            lines.append("\t".join(["rcsound", str(number), f"{t:.4f}", cue]))
    if cust_weapon is not None:
        lines.append("\t".join(["custpose", "weapon", str(cust_weapon)]))
    path = os.path.join(out, f"{cls}.txt")
    old = open(path, encoding="utf-8").read().splitlines()

    def melee_secs(line):
        # (class_line's last field: how long its melee clip plays.)
        f = line.split("\t")
        if f[0] == "class" and len(f) == 10 and "melee" in timings:
            f[9] = f"{timings['melee'][0]:.4f}"
        return "\t".join(f)
    old = [melee_secs(l) for l in old]
    kinds = ("robotclip", "rcsound", "custpose")
    at = next((i for i, l in enumerate(old) if l.split("\t")[0] in kinds), None)
    if at is None:
        # (None before: after the class's sounds.)
        at = max((i + 1 for i, l in enumerate(old) if l.split("\t")[0] in ("class", "camera", "vweapon", "csound")),
                 default=len(old))
    rest = [l for l in old if l.split("\t")[0] not in kinds]
    before = sum(1 for l in old[:at] if l.split("\t")[0] not in kinds)
    new = rest[:before] + lines + rest[before:]
    with open(path, "w", encoding="utf-8") as fh:
        fh.write("\n".join(new) + "\n")
    print(f"wrote {cls}.txt ({len(lines)} robot clip lines, {len(new)} lines)")


def gun_paint_pass(cooked, assets):
    """The Infiltrator's file (whose gunpaint lines are every class's) gets the guns
    WEAPON_PAINT lists that it hasn't yet: each one's colour texture with the paint
    mask, from CookedPC, and its gunpaint line, after the others."""
    model_dir = "models/infiltrator"
    out = os.path.join(assets, *model_dir.split("/"))
    path = os.path.join(out, "infiltrator.txt")
    old = open(path, encoding="utf-8").read().splitlines()
    have = {l.split("\t")[1] for l in old if l.startswith("gunpaint\t")}
    textures = Textures(None, os.path.join(out, "textures"), os.path.join(assets, "models", "vehicle.glb"), cooked)
    lines = []
    for material, texset in WEAPON_PAINT:
        if material in have:
            continue
        t = textures.texset(texset, cooked=True)
        if not t:
            print("  ! no texture set", texset, "in CookedPC")
            continue
        os.makedirs(os.path.join(out, "textures"), exist_ok=True)
        with open(os.path.join(out, "textures", texset + "_c.ktx2"), "wb") as fh:
            fh.write(t["c"])
        lines.append("\t".join(["gunpaint", material, f"{model_dir}/textures/{texset}_c.ktx2"]))
        print("  + gunpaint", material, texset)
    at = max((i + 1 for i, l in enumerate(old) if l.startswith("gunpaint\t")), default=len(old))
    with open(path, "w", encoding="utf-8") as fh:
        fh.write("\n".join(old[:at] + lines + old[at:]) + "\n")
    print(f"wrote infiltrator.txt ({len(lines)} gunpaint lines added)")


def umodel_commands(cls, cooked, transgame, export_dir=None):
    """The umodel commands that export what main() reads (see the top): the meshes
    and animation sets; with the meshes exported, also their textures."""
    data = class_data.read(cooked, transgame, cls)
    mp = data.pop("mp")

    def q(path):
        return f'"{path}"' if " " in path else path

    def cmd(flags, package, objs=()):
        return " ".join([f"umodel -game=trans -export {flags}-path={q(cooked)} -out={q(export_dir or '<export dir>')}",
                         package] + [f"-obj={o}" for o in sorted(objs)])

    vehicles = [v for v in data["vehicles"].values() if v]
    meshes = set(data["meshes"]) | set(wheel_meshes(data))
    guns = ROCKET_LAUNCHERS if ROCKETS in data["vehicle_weapons"] else CAR_GUNS
    meshes |= {v["mesh"] for v in vehicles} | {CAR_GUNS["mesh"]}
    anims = {a for p in data["parts"] for a in p["anims"]} | {CAR_GUNS["anims"], guns["anims"]}
    anims |= {a for v in vehicles for a in v["anims"] + v["robot_anims"]}
    # The class's own robot clips' sets (none of the Infiltrator's are needed: robot.glb
    # has them), and its melee set's sword swing (the Infiltrator's too: robot.glb's
    # melee is Jazz's shield bash).
    if cls != "infiltrator":
        anims |= {s for s in data["robot"]["anims"] if s not in data["scout_anims"]}
    if data["robot"].get("melee_clip"):
        anims.add(data["robot"]["melee_clip"][0])
    shared = {a for a in anims if not mp.by_name.get(a)}
    out = [cmd("-gltf ", class_data.MP_PACKAGE, meshes), cmd("", class_data.MP_PACKAGE, anims - shared)]
    if shared:
        out.append(cmd("", class_data.SHARED_PACKAGE, shared))
    if guns is not CAR_GUNS:
        out.append(cmd("-gltf ", class_data.SHARED_PACKAGE, [guns["mesh"]]))
    out.append(cmd("-png ", "WEP_AssaultRifleArm_p.xxx"))
    folder = os.path.join(export_dir or "", class_data.MP_PACKAGE.split(".")[0])
    if not (export_dir and os.path.isdir(folder)):
        out.append("# then run this again with the export folder: it lists the meshes' textures")
        return out
    texsets = set(MATERIAL_FALLBACK.values()) if cls == "infiltrator" else set()
    pictures = set()
    for kind in ("SkeletalMesh3", "StaticMesh3"):
        if os.path.isdir(os.path.join(folder, kind)):
            for f in os.listdir(os.path.join(folder, kind)):
                if f.endswith(".gltf"):
                    for m in json.load(open(os.path.join(folder, kind, f))).get("materials", []):
                        if mp.by_name.get(m["name"]):
                            info = mp.material(m["name"])
                            texsets.add(info["texset"])
                            if not info["texset"] and info["textures"].get(PLAIN_TEXTURES["color"]):
                                pictures.update(t for p, t in info["textures"].items()
                                                if p in PLAIN_TEXTURES.values())
    # Each set's two pictures, from the map where it has them, or the character's
    # own package (TR_<name>_ROBO_p) where it doesn't.
    by_package = {}
    others = {}
    for t in sorted(x for x in texsets if x):
        objs = [t + "_Color_NormX", t + "_Masks_NormY"]
        package = class_data.MP_PACKAGE
        if not all(mp.by_name.get(o) for o in objs):
            package = None
            for f in sorted(os.listdir(cooked)):
                if f.startswith("TR_") and f.endswith("_ROBO_p.xxx"):
                    if f not in others:
                        others[f] = class_data.Mp(cooked, f)
                    if all(others[f].by_name.get(o) for o in objs):
                        package = f
                        break
        if package:
            by_package.setdefault(package, []).extend(objs)
    if pictures:
        by_package.setdefault(class_data.MP_PACKAGE, []).extend(sorted(pictures))
    out += [cmd("-png ", package, objs) for package, objs in sorted(by_package.items())]
    return out


if __name__ == "__main__":
    args = sys.argv[1:]
    if len(args) in (4, 5) and args[0] == "--umodel" and args[1] in class_data.CLASSES:
        print("\n".join(umodel_commands(*args[1:])))
    elif len(args) == 4 and args[0] == "--glow" and args[1] in class_data.CLASSES:
        glow_pass(*args[1:])
    elif len(args) == 5 and args[0] == "--drive" and args[1] in class_data.CLASSES:
        drive_pass(*args[1:])
    elif len(args) == 5 and args[0] == "--ground" and args[1] in class_data.CLASSES:
        ground_pass(*args[1:])
    elif len(args) == 5 and args[0] == "--fx" and args[1] in class_data.CLASSES:
        fx_pass(*args[1:])
    elif len(args) == 5 and args[0] == "--sounds" and args[1] in class_data.CLASSES:
        sounds_pass(*args[1:])
    elif len(args) == 5 and args[0] == "--decals" and args[1] in class_data.CLASSES:
        decals_pass(*args[1:])
    elif len(args) == 6 and args[0] == "--robotclips" and args[1] in class_data.CLASSES:
        robot_clips_pass(*args[1:])
    elif len(args) == 6 and args[0] == "--parts" and args[1] in class_data.CLASSES:
        parts_pass(*args[1:])
    elif len(args) == 4 and args[0] == "--interiors" and args[1] in class_data.CLASSES:
        interiors_pass(*args[1:])
    elif len(args) == 3 and args[0] == "--transforms" and args[1] in class_data.CLASSES:
        transforms_pass(*args[1:])
    elif len(args) == 3 and args[0] == "--gunpaint":
        gun_paint_pass(*args[1:])
    elif len(args) == 5 and args[0] in class_data.CLASSES:
        main(*args)
    else:
        raise SystemExit(__doc__)
