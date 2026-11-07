"""The multiplayer melee (src/melee.rs): the Ion Blade each class's sword swing pulls
out, and when and where each class's swing strikes, read from the game.

In multiplayer a melee is the class's melee set's MELEE_WeaponAttack (its robot
definition's MeleeSet: TR_Shared_MS_p.MPMeleeSet_<class>, a TnMeleeSet, in
MP_Array_BASE_m.xxx): Melee_Sword_<class> of TR_Iconic_Melee_ANM_p.<class>_ROBO_ANIM,
played at its own rate (TnMeleeManager.PlayMeleeAttack; the melee speed modifier is
1). Its notifies time the rest:

  * TnAnimNotify_WeaponMelee, 8.6 ms in: the gun puts on the character's melee
    attachment (TnWeapon.EquipMelee; TransCustomization.ini's IonBlade, every
    specialty's: TransContent.TnWeaponModificationIonBlade). That is its
    SkeletalMeshAttachment, WEP_IonBlade_SKEL (mp_shared.xxx), on the gun's weapon
    mesh at its MeshAttachmentSocket, MeleeSocket - every gun has one, on the
    forearm's elbow bone, placed and sized for the gun - playing its
    MeleeEquipAnimation, IonBlade_Equip (the blade's pieces sliding out of the
    housing, 0.33 s). The clip's length less IonBlade_UnEquip's (0.37 s) after the
    notify, it plays that (UnequipTime), and as that ends the blade comes off
    (HideTime). Only the Infiltrator's and the Scientist's swings carry the notify
    in the cooked clips: the Destroyer's and the Titan's swing without the blade, as
    in the game (their swing line's blade field is "-").
  * TnAnimNotify_DamageSweep, 0.15 s in: the blow. A box (DamageSweepInfo: its
    SocketName, MeleeSocket_SmallRobot - on the robot skeleton's root bone, ahead of
    the robot at the height of its chest - with its Offset turned as the robot
    faces, TnTraceSweeper.AddOffset: 1.1 to 2 m to its right, to its left as it
    holds its gun in the left hand; its Extent, half sizes) strikes what it meets
    for the sweep's Duration, each thing once (TnTraceSweeper; TnMeleeManager: the
    attack's Damage, 135, TnDamageTypeMelee).

The blade is drawn in two materials: its housing and blade, WEP_IonBlade_MATINST (the
characters' paint, Iconic_Cars_Mat, on WEP_RocketLauncher_p's textures: src/melee.rs
paints it as the Destroyer's rocket launcher is painted), and a strip along the
blade's edge, WEP_BLadeAut_MAT: unlit and additive, WEP_IaconMask's circuitry lit
red and orange, with pulses running along it (assets/shaders/ionblade.wgsl has its
graph).

This writes, under <assets>/models/weapons:

  ionblade.glb         the blade: its skeleton (17 bones from WEP_Reference_XR) and
                       its skinned mesh, two primitives (materials named
                       WEP_IonBlade_MATINST and WEP_BLadeAut_MAT, without textures),
                       in umodel's glTF axes and metres
  ionblade_mask.ktx2   WEP_IaconMask (the glow's mask, RGB as data)
  melee.txt            tab-separated:
      bladeclip <clip> <seconds>
          IonBlade_Equip (equip) and IonBlade_UnEquip (unequip)
      bladekey <clip> <bone> <seconds> <tx ty tz> <qx qy qz qw>
          each frame of the bones each clip moves (the others keep their rest pose)
      bladesocket <weapon mesh> <bone> <t> <q> <s>
          each gun's MeleeSocket, right-arm and left-arm meshes (umodel's glTF axes,
          metres, on the bone; as the class files' socket lines)
      swing <class> <clip seconds> <blade at> <sweep at> <sweep seconds>
            <socket x y z> <offset x y z> <half size x y z>
          each class's sword swing: its length, when the blade comes out ("-": it
          doesn't, the swing has no TnAnimNotify_WeaponMelee), when the
          blow's sweep starts and how long it lasts, and its box: the socket in
          robot.glb's space (its rest pose: glTF axes, metres, before the class's
          robot scale), the offset from it and the box's half sizes in the robot's
          own axes (x to its right, y up, z behind it; metres)

usage: python build_melee.py <TransGame\\CookedPC> <umodel export folder> <foc_esc\\assets>
       python build_melee.py --umodel <TransGame\\CookedPC> <umodel export folder>
           prints the umodel commands that export what it needs
(<foc_esc\\assets> needs models/robot.glb, the sweep's socket's bone's rest pose.)
"""
import os
import sys

import numpy as np
from PIL import Image

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
sys.path.insert(0, os.path.join(HERE, "ue3"))
import build_class  # noqa: E402
import class_data  # noqa: E402
import ktx2_io  # noqa: E402
from build_upgrades import WEAPON_MESHES  # noqa: E402
from gltf_io import Doc, Writer, node_globals  # noqa: E402
from props import read_props  # noqa: E402
from psa_io import Psa  # noqa: E402
from ue3pkg import Reader  # noqa: E402

PACKAGE = "mp_shared.xxx"
BLADE_MESH = "WEP_IonBlade_SKEL"
BLADE_ANIMS = "WEP_IonBlade_ANIM"
MASK = "WEP_IaconMask"
# The blade's clips (the attachment's MeleeEquipAnimation and MeleeUnequipAnimation).
CLIPS = (("equip", "IonBlade_Equip"), ("unequip", "IonBlade_UnEquip"))
SOCKET = "MeleeSocket"
CLASS_ORDER = ("infiltrator", "destroyer", "titan", "scientist")
# The robot skeleton, whose sockets the sweeps start from (build_robot_extras.py's).
SKELETON = ("all_shared.xxx", "TR_Skeletons_p.RB_Iconic_SKEL")


def find(export_dir, kind, name):
    for root, _, files in os.walk(export_dir):
        if os.path.basename(root) == kind and name in files:
            return os.path.join(root, name)
    return None


def build_blade(export_dir, path):
    """ionblade.glb: the export's skeleton and skinned mesh, as they are."""
    src = find(export_dir, "SkeletalMesh3", BLADE_MESH + ".gltf")
    if not src:
        raise SystemExit(f"{BLADE_MESH} not exported (see --umodel)")
    d = Doc(src)
    g = d.g
    w = Writer()
    # The nodes as exported (the mesh's node and its skeleton below it).
    for n in g["nodes"]:
        node = {k: v for k, v in n.items() if k in ("name", "children", "translation", "rotation", "scale")}
        w.g["nodes"].append(node)
    prims = []
    for p in g["meshes"][g["nodes"][0]["mesh"]]["primitives"]:
        a = p["attributes"]
        mat_name = g["materials"][p["material"]]["name"]
        material = w.material({"name": mat_name, "pbrMetallicRoughness": {"baseColorFactor": [1, 1, 1, 1]}})
        weights = d.acc(a["WEIGHTS_0"]).astype(np.float32)
        weights /= np.maximum(weights.sum(1, keepdims=True), 1e-9)
        prims.append({"attributes": {
            "POSITION": w.accessor(d.acc(a["POSITION"]).astype(np.float32), target=34962, minmax=True),
            "NORMAL": w.accessor(d.acc(a["NORMAL"]).astype(np.float32), target=34962),
            "TANGENT": w.accessor(d.acc(a["TANGENT"]).astype(np.float32), target=34962),
            "TEXCOORD_0": w.accessor(d.acc(a["TEXCOORD_0"]).astype(np.float32), target=34962),
            "JOINTS_0": w.accessor(d.acc(a["JOINTS_0"]).astype(np.uint16), target=34962),
            "WEIGHTS_0": w.accessor(weights, target=34962),
        }, "indices": w.accessor(d.acc(p["indices"]).astype(np.uint32).ravel(), target=34963),
            "material": material})
    w.g["meshes"].append({"name": BLADE_MESH, "primitives": prims})
    w.g["nodes"][0]["mesh"] = 0
    skin = g["skins"][0]
    w.g["skins"] = [{"joints": skin["joints"],
                     "inverseBindMatrices": w.accessor(d.acc(skin["inverseBindMatrices"]).astype(np.float32))}]
    w.g["nodes"][0]["skin"] = 0
    w.g["scenes"] = [{"nodes": [0]}]
    w.g["scene"] = 0
    size = w.save(path)
    bones = [g["nodes"][j]["name"] for j in skin["joints"]]
    return size, bones, [g["materials"][p["material"]]["name"] for p in g["meshes"][0]["primitives"]]


def build_mask(export_dir, path):
    src = find(export_dir, "Texture2D", MASK + ".png")
    if not src:
        raise SystemExit(f"{MASK} not exported (see --umodel)")
    rgb = np.asarray(Image.open(src).convert("RGB"))
    with open(path, "wb") as fh:
        fh.write(ktx2_io.encode(rgb, "data"))
    return rgb.shape


def clip_lines(cooked, export_dir):
    """The blade's clips' lengths (their SequenceLength, as TnWeapon.EquipMelee times
    the blade by), and every frame of the bones each moves."""
    src = find(export_dir, "AnimSet", BLADE_ANIMS + ".psa")
    if not src:
        raise SystemExit(f"{BLADE_ANIMS} not exported (see --umodel)")
    psa = Psa(src)
    shared = class_data.Mp(cooked, PACKAGE)
    lines = []
    for key, name in CLIPS:
        tracks, dt = psa.clip(name)
        q = shared.sequence(BLADE_ANIMS, name)
        if tracks is None or not q:
            raise SystemExit(f"{BLADE_ANIMS} has no {name}")
        frames = len(next(iter(tracks.values()))[0])
        length = float(shared.props(q).get("SequenceLength", (frames - 1) * dt))
        lines.append("\t".join(["bladeclip", key, f"{length:.4f}"]))
        for bone, (pos, rot) in tracks.items():
            if np.ptp(pos, 0).max() < 1e-4 and np.ptp(rot, 0).max() < 1e-4:
                continue
            for f in range(frames):
                lines.append("\t".join(["bladekey", key, bone, f"{f * dt:.4f}", build_class.fmt(pos[f]),
                                        build_class.fmt(rot[f])]))
    return lines


def socket_lines(cooked):
    """Each gun's MeleeSocket (its right-arm and left-arm weapon meshes)."""
    mp = class_data.Mp(cooked, "all_shared.xxx")
    lines = []
    for weapon, meshes in WEAPON_MESHES.items():
        for mesh in meshes:
            idx = next((i for full, i in mp.by_full.items() if full.endswith("." + mesh)), 0)
            s = next((s for s in mp.sockets(idx) if s["name"] == SOCKET), None) if idx else None
            if s is None:
                print(f"  ! {mesh} ({weapon}) has no {SOCKET}")
                continue
            t, q, sc = build_class.gltf_socket(s)
            lines.append("\t".join(["bladesocket", mesh, s["bone"], build_class.fmt(t), build_class.fmt(q),
                                    build_class.fmt(sc)]))
    return lines


def _vec(v):
    """A Vector struct property (its 12 bytes), or None."""
    if isinstance(v, tuple) and v[0] == "struct" and isinstance(v[2], (bytes, bytearray)) and len(v[2]) >= 12:
        return [float(x) for x in np.frombuffer(bytes(v[2][:12]), "<f4")]
    return None


def model_sockets(cooked, robot_glb):
    """The robot skeleton's sockets' places in robot.glb's space, in its rest pose
    ({name: (x, y, z)}: glTF axes, metres, before a class's robot scale)."""
    mp = class_data.Mp(cooked, SKELETON[0])
    idx = mp.find(SKELETON[1])
    if not idx:
        raise SystemExit(f"no {SKELETON[1]} in {SKELETON[0]}")
    g = Doc(robot_glb).g
    bones = {n.get("name"): i for i, n in enumerate(g["nodes"])}
    globals_ = node_globals(g)
    out = {}
    for s in mp.sockets(idx):
        if s["bone"] in bones:
            t, _, _ = build_class.gltf_socket(s)
            out[s["name"]] = (globals_[bones[s["bone"]]] @ np.array([*t, 1.0]))[:3]
    return out


def robot_axes(v):
    """An Unreal vector in the robot's own space (X ahead, Y to its right, Z up), in
    the demo's robot axes (x to its right, y up, z behind it) and metres."""
    return [v[1] * 0.01, v[2] * 0.01, -v[0] * 0.01]


def swing_lines(cooked, robot_glb):
    """Each class's sword swing (its melee set's MELEE_WeaponAttack): its length, the
    blade's notify and the blow's sweep."""
    mp = class_data.Mp(cooked)
    sockets = model_sockets(cooked, robot_glb)
    lines = []
    rows = []
    for cls in CLASS_ORDER:
        robot = class_data.robot_info(mp, class_data.CLASSES[cls][1])
        anim_set, clip = robot["melee_clip"]
        q = mp.sequence(anim_set, clip)
        if not q:
            raise SystemExit(f"{anim_set}.{clip} isn't in {class_data.MP_PACKAGE}")
        length = float(mp.props(q).get("SequenceLength", 0.0))
        blade, sweep = None, None
        for t, kind, pr in mp.notifies(q):
            if kind == "TnAnimNotify_WeaponMelee" and blade is None:
                blade = t
            elif kind == "TnAnimNotify_DamageSweep" and sweep is None:
                info = pr.get("DamageSweepInfo")
                sub = read_props(mp.pkg, Reader(info[2]), len(info[2])) if isinstance(info, tuple) else {}
                offset = _vec(sub.get("Offset")) or [0.0, 0.0, 0.0]
                extent = _vec(sub.get("Extent")) or _vec(pr.get("Extent"))
                secs = float(sub.get("Duration", pr.get("Duration", 0.2)))
                socket = sub.get("SocketName", pr.get("SocketName"))
                if socket not in sockets:
                    raise SystemExit(f"{clip}'s sweep's socket {socket} isn't the robot skeleton's")
                sweep = (t, secs, sockets[socket], offset, extent)
        if sweep is None:
            raise SystemExit(f"{clip} has no damage sweep")
        rows.append((cls, clip, length, blade, sweep))
    for cls, clip, length, blade, (t, secs, socket, offset, extent) in rows:
        # (The box's half sizes as the offset's axes have them: Extent's X ahead, Y
        # to the side, Z up.)
        half = [abs(x) for x in robot_axes(extent)]
        lines.append("\t".join(["swing", cls, f"{length:.4f}", "-" if blade is None else f"{blade:.4f}",
                                f"{t:.4f}", f"{secs:.4f}", build_class.fmt(socket),
                                build_class.fmt(robot_axes(offset)), build_class.fmt(half)]))
        print(f"  {cls}: {clip} {length:.3f}s, blade {'at %.4f' % blade if blade is not None else '(none)'}, "
              f"sweep {t:.3f}+{secs:.2f}s at {np.round(socket, 3).tolist()}, offset {offset}, extent {extent}")
    return lines


def main(cooked, export_dir, assets):
    out = os.path.join(assets, "models", "weapons")
    os.makedirs(out, exist_ok=True)
    size, bones, mats = build_blade(export_dir, os.path.join(out, "ionblade.glb"))
    print(f"wrote ionblade.glb ({len(bones)} bones, materials {mats}, {size} bytes)")
    h, w = build_mask(export_dir, os.path.join(out, "ionblade_mask.ktx2"))[:2]
    print(f"wrote ionblade_mask.ktx2 ({w} x {h})")
    lines = ["# The multiplayer melee (tools/build_melee.py): the Ion Blade's clips and its",
             "# place on each gun, and each class's sword swing."]
    lines += clip_lines(cooked, export_dir)
    lines += socket_lines(cooked)
    lines += swing_lines(cooked, os.path.join(assets, "models", "robot.glb"))
    with open(os.path.join(out, "melee.txt"), "w", encoding="utf-8") as fh:
        fh.write("\n".join(lines) + "\n")
    print(f"wrote melee.txt ({len(lines)} lines)")


def umodel_commands(cooked, export_dir):
    base = f'umodel -game=trans -path="{cooked}" -out="{export_dir}" {PACKAGE}'
    return [f"{base.replace('-path', '-export -gltf -path')} {BLADE_MESH}",
            f"{base.replace('-path', '-export -path')} {BLADE_ANIMS}",
            f"{base.replace('-path', '-export -png -path')} {MASK} Texture2D"]


if __name__ == "__main__":
    args = sys.argv[1:]
    if len(args) == 3 and args[0] == "--umodel":
        print("\n".join(umodel_commands(*args[1:])))
    elif len(args) == 3:
        main(*args)
    else:
        raise SystemExit(__doc__)
