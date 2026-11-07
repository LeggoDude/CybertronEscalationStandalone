"""Turn UE Viewer (umodel) glTF exports from Fall of Cybertron into .glb files the
Bevy demo can load directly.

What it does
  * merges character parts that share a skeleton into one model (one skeleton)
  * finds each material's packed "TEXSET" textures by name and converts them:
      _Color_NormX : RGB = colour,          A = normal X
      _Masks_NormY : R = glow, G = specular, A = normal Y
    into standard glTF base colour / normal / metal-roughness / emissive maps
  * drops the untextured faction-logo decals
  * wraps each model in a "Root" node that turns it to face Bevy's forward (-Z)

usage: python build_assets.py <umodel package export dir> <foc_esc/assets dir>

Files missing from the package export are also looked for in a second umodel
export of all_shared (the Photon Burst Rifle, the Scatter Blaster and their
textures and animation sets live there): set
FOC_SHARED_EXPORT to that folder (default: an "all_shared" folder next to the
package export).

The Escalation health stations' base (props/health_base.glb) is
ESC_PickupBase_Health_STAT from a umodel export of an Escalation map's BASE package
(Downfall's ESC_Iacon_BASE_m; every map's has the same), with its texture set
(MP_PickupBase_TEXSET) from one of ENV_MP_PropsShared_p (the package holding its
full-size mips): exports of those packages next to the package export (folders
named after them, as umodel writes them) are searched too. Without them the model
is left out and the demo stands a plain disc in for it.

The Escalation ammo stations' crate (props/ammo_crate.glb) is AmmoPickupLight_STAT
(TnAmmoCratePickup's PickupFactoryMesh) from the all_shared export, with its texture
set (PROP_NEU_AmmoPickup_NEW_texset, whose full-size mips are in
PROP_NEU_Pickups_p.xxx), drawn as the weapon pickups are (their materials and
glowing outline: pickup_material, outline_shell).

The energon cube (props/energon.glb) is PROP_NEU_EnergonCube_STAT with the swirl of
colour its sides' material draws its clouds from (fractalMaps), from an export of
PROP_NEU_Pickups_p (the package holding its pixels) next to the package export or in
the all_shared export; src/energon.rs draws the cube in the game's materials.
Without the texture the cube's sides are flat cyan.
"""
import io, math, os, sys
import numpy as np
from PIL import Image
from gltf_io import Doc, Writer
from psa_io import Psa
import mirror_anims

SRC = sys.argv[1]
OUT = sys.argv[2]
SK = os.path.join(SRC, "SkeletalMesh3")
ST = os.path.join(SRC, "StaticMesh3")
SHARED = os.environ.get("FOC_SHARED_EXPORT", os.path.join(os.path.dirname(os.path.abspath(SRC)), "all_shared"))
# Exports of the Escalation map's BASE package and the package of its pickup
# stations' textures, next to the package export (for the health stations).
MORE = [os.path.join(os.path.dirname(os.path.abspath(SRC)), d)
        for d in ("ESC_Iacon_BASE_m", "ESC_Gladiator_BASE_m", "ESC_Ark_BASE_m", "mp_shared", "ENV_MP_PropsShared_p",
                  "PROP_NEU_Pickups_p", "WEP_KamikazeMine_p")]
# More export folders to look in (FOC_MORE_EXPORTS, separated as PATH is).
MORE += [d for d in os.environ.get("FOC_MORE_EXPORTS", "").split(os.pathsep) if d]


def find(sub, name):
    """Path of an exported file: from the package export, else the all_shared export
    (or the exports in MORE)."""
    for base in [SRC, SHARED] + MORE:
        path = os.path.join(base, sub, name)
        if os.path.exists(path):
            return path
    return os.path.join(SRC, sub, name)


def psa_file(anim_dir, name):
    """An animation set's ActorX .psa: Animations\\<set>\\<set>.psa if it was saved
    there, else umodel's AnimSet export of it in the package exports."""
    path = os.path.join(anim_dir, name, name + ".psa")
    return path if os.path.exists(path) else find("AnimSet", name + ".psa")


class PsaForGltf(str):
    """The path of a .psa standing in for umodel's glTF export of Jazz with that
    animation set (which only umodel's viewer saves): add_animations reads a clip
    that holds one pose as one key, as the glTF has it."""


def gltf_or_psa(anim_dir, name):
    """An animation set's clips: umodel's glTF export of Jazz with the set
    (Animations\\<set>\\RB_Jazz_CHARSKEL_ArmLoL.gltf, saved from the viewer) if there
    is one, else the set's .psa (psa_file), which umodel's command line exports."""
    path = os.path.join(anim_dir, name, "RB_Jazz_CHARSKEL_ArmLoL.gltf")
    return path if os.path.exists(path) else PsaForGltf(psa_file(anim_dir, name))


def tex_path(name):
    return find("Texture2D", name)

# Material name in the export -> texture set name in Texture2D (without suffix).
TEXSETS = {
    "RB_Jazz_Legs_Mat_INST": "RB_Jazz_Legs_TEXSET",
    "RB_Jazz_Chest_Mat_INST": "RB_Jazz_Chest_TEXSET",
    "RB_Jazz_Head_Mat_INST": "RB_Jazz_Head_TEXSET",
    "RB_Jazz_ArmUp_Mat_INST": "RB_Jazz_ArmUP_TEXSET",
    "RB_Jazz_ArmLo_Mat_INST": "RB_Jazz_ArmLo_TEXSET",
    "WEP_SubsonicRepeater_MATINST": "WEP_SubsonicRepeater_TEXSET",
    "VH_Jazz_NormSpec_Cust_E_Mat_INST": "VH_Jazz_Clr_TEXSET",
    "WEP_AttackSentry_body_MATINST": "AP_AttackSentry_body_TEXSET",
    "WEP_AttackSentry_parts_MATINST": "AP_AttackSentry_parts_TEXSET",
    "ENV_RST_EXT_Wall01_MATINST": "ENV_RST_EXT_Wall01l_TXTSET",
    "ENV_RST_SewerLedgeFloor01_MATINST": "ENV_RST_SewerLedgeFloor01_TXTSET",
    "ENV_RST_VentCoverRust_matinst": "ENV_RST_VentCover_TXTSET",
    "PROP_UND_PistonSet_C_CLR_MATIST": "PROP_UND_PistonSet_C_CLR_TEXSET",
    "WEP_Grenade_MATINST": "WEP_Grenade_TEXSET",
    "RB_Jazz_Wheel_Mat_INST_INST": "RB_Jazz_Wheel_TEXSET",
    "WEP_AssaultRifle_MATINST": "WEP_AssaultRifle_TEXSET",
    "WEP_BurstRifle_MATINST": "WEP_BurstRifle_TEXSET",
    "WEP_Shotgun_MatINST": "WEP_Shotgun_TEXSET",
    # The Nucleon Charge Rifle (the game's "SniperRifle"): its texture set's full-size
    # mips are in WEP_SniperRifle_p.xxx (the level package holds 128 x 128 ones).
    "WEP_SniperRifle_MatINST": "WEP_SniperRifle_TEXSET",
    # The Path Blaster (the game's "HeavyPistol"): its mesh's material is
    # WEP_IonBlaster_MATINST, on the texture set of that name that WEP_HeavyPistol_p
    # holds (its full-size mips are in WEP_HeavyPistol_p.xxx).
    "WEP_IonBlaster_MATINST": "WEP_IonBlaster_TEXSET",
    # The Riot Cannon (the game's "Bazooka", WEP_Bazooka_p): its texture set's
    # full-size mips (512 x 512) are in WEP_Bazooka_p.xxx.
    "WEP_RiotCannon_MATINST": "WEP_RiotCannon_TEXSET",
    # The Orbital Pistol (the game's "TracerDart", WEP_TracerDart_p, on the campaign's
    # SysCorruptor gun): its texture set's full-size mips (512 x 512) are in
    # WEP_TracerDart_p.xxx.
    "WEP_SysCorruptor_MATINST": "WEP_SysCorrupter_TEXSET",
    # The eight weapons added after those, all in all_shared.xxx, each texture set's
    # full-size mips (512 x 512) in its weapon's WEP_*_p.xxx: the Titan's X18
    # Scrapmaker (the game's "HeavyMG", WEP_HeavyMG_p), Energon Harvester ("DrainGun",
    # on the EMP Shotgun's mesh: WEP_EMPShotgun_p), E.D.K. TechVolt ("ChainBeam", on the
    # repair beam's mesh: WEP_RepairBeam_p) and Corrosive Slime Cannon ("AoESlow", the
    # Rust Cannon's mesh: WEP_AoeSlow_p), the Scientist's Gear Shredder ("SawBlade",
    # WEP_Sawblade_p), and the Destroyer's Thermo Rocket Cannon ("HomingRocket", the
    # rocket launcher: WEP_RocketLauncher_p), A-4 Pulsar Cannon ("GrenadeLauncher",
    # WEP_GrenadeLauncher_p) and Chaos-Rift Combustor ("ClusterCannon", the grenade
    # arm's mesh: WEP_GrenadeArm_p).
    "WEP_HeavyMG_MATINST": "WEP_HeavyMG_TEXSET",
    "WEP_EMPShotgun_MATINST": "WEP_EMPShotgun_TEXSET",
    "WEP_TechvoltEmitter_MatINST": "WEP_TechvoltEmitter_TEXSET",
    "WEP_RustCannon_MATINST": "WEP_RustCannon_TEXSET",
    "WEP_Sawblade_MATINST": "WEP_SawBlade_TEXSET",
    "WEP_RocketLauncher_MATINST": "WEP_RocketLauncher_TEXSET",
    "WEP_GrenadeLauncher_MATINST": "WEP_A4PulsarCannon_TEXSET",
    "WEP_ClusterCannon_MATINST": "WEP_ClusterCannon_TEXSET",
    # The ammo stations' crate (AmmoPickupLight_STAT): its texture set's full-size mips
    # are in PROP_NEU_Pickups_p.xxx.
    "AmmoPickup_MATINST": "PROP_NEU_AmmoPickup_NEW_texset",
    # The heavy ammo crate and the equipment on Escalation's other purchase points
    # (STATION_PROPS): their texture sets are in ENV_MP_PropsShared_p.xxx (the
    # Intercooler's mesh in mp_shared.xxx), WEP_KamikazeMine_p.xxx and
    # PROP_NEU_Pickups_p.xxx.
    "HeavyAmmoPickup_MATINST": "PROP_NEU_AmmoPickup_NEW_texset",
    "MP_Pickup_Barrier_MATINST": "MP_Pickup_Barrier_TEXSET",
    "WEP_KMine_MATINST": "WEP_KMine_TEXSET",
    "MP_Pickup_EnergonRecharger_MATINST": "MP_Pickup_EnergonRecharger_TEXSET",
    "MP_Pickup_Intercooler_MATINST": "MP_Pickup_Intercooler_TEXSET",
    # The Dimensional Decimator's pickup (its texture set WEP_GravityBomb_TEXSET's
    # textures are named WEP_GravityBomb2_TEXSET_*, in the Escalation map packages).
    "WEP_BlackHoleGrenade_MATINST": "WEP_GravityBomb2_TEXSET",
    # The support items in use: the Thermo Mine Payload's mines and the Dimensional
    # Decimator's ball (build_equipment_models).
    "KMine_MATINST": "WEP_KMine_TEXSET",
    "WEP_GravityBomb_MATINST": "WEP_GravityBomb2_TEXSET",
    # The part of Jazz's arm built into each weapon mesh (the gun arm's upper arm). Its
    # texture set's full-size mips are streamed from WEP_AssaultRifleArm_p.xxx, so
    # umodel needs that package next to the map's to export them at 512 x 512.
    "RB_ArmUp_Cust_MatINST_INST": "WEP_ArmUp_TEXSET",
    # The Escalation health station's base (ESC_PickupBase_Health_STAT): the pickup
    # bases' shared texture set, its lights glowing white (Emissive_Color 1,1,1).
    "ESC_PickupBase_Health_MATINST": "MP_PickupBase_TEXSET",
}
# Wheel bones whose wheel mesh is mirrored (see add_wheel_meshes).
WHEEL_MIRROR_PREFIX = "R_"
# The car mesh is exported smaller than Jazz's robot form: its wheel arches fit
# the game's wheel mesh (0.69 m radius) only at about 2.2x. Scaling it up makes
# the car match the robot and its wheels.
VEHICLE_SCALE = float(os.environ.get("FOC_VEHICLE_SCALE", "2.2"))
# Plain colour + standard normal map.
PLAIN = {"vint_pipeset_CLR_Mat": ("vint_pipeset_CLR", "vint_pipeset_NORM")}
# No textures available: solid materials. The energon cube's are stand-ins (the demo
# draws its sides and edges in its own materials: src/energon.rs): cyan, its sides'
# average glow (0.27, 1.93, 2.15; they carry the clouds texture too, build_energon),
# and its edges' white at 3.
SOLID = {
    "PROP_NEU_EnergonCube_Clouds_MAT": {"pbrMetallicRoughness": {"baseColorFactor": [0, 0, 0, 1], "metallicFactor": 0.0, "roughnessFactor": 0.3},
                                        "emissiveFactor": [0.125, 0.9, 1.0], "extensions": {"KHR_materials_emissive_strength": {"emissiveStrength": 2.15}}},
    "PROP_NEU_EnergonCube_EdgeGlow_MAT": {"pbrMetallicRoughness": {"baseColorFactor": [0, 0, 0, 1], "metallicFactor": 0.0, "roughnessFactor": 0.2},
                                          "emissiveFactor": [1.0, 1.0, 1.0], "extensions": {"KHR_materials_emissive_strength": {"emissiveStrength": 3.0}}},
}
DROP = {"TR_FactionLogo_Mat_INST"}


def png(arr):
    b = io.BytesIO()
    Image.fromarray(arr).save(b, "PNG", optimize=True)
    return b.getvalue()


_tex_cache = {}


def texset_images(base):
    """Decode a packed TEXSET into standard glTF images (cached)."""
    if base in _tex_cache:
        return _tex_cache[base]
    c = np.asarray(Image.open(tex_path(base + "_Color_NormX.png")).convert("RGBA"))
    h, w = c.shape[:2]
    m = np.asarray(Image.open(tex_path(base + "_Masks_NormY.png")).convert("RGBA").resize((w, h), Image.BILINEAR))
    color = c[..., :3].copy()
    # Normal map: X from colour alpha, Y from mask alpha. Unreal stores DirectX-style
    # normals (green points down), glTF wants OpenGL-style, so Y is flipped.
    nx = c[..., 3].astype(np.float32) / 127.5 - 1
    ny = -(m[..., 3].astype(np.float32) / 127.5 - 1)
    nz = np.sqrt(np.clip(1 - nx * nx - ny * ny, 0, 1))
    normal = np.stack([nx, ny, nz], -1) * 127.5 + 127.5
    normal = np.clip(normal, 0, 255).astype(np.uint8)
    spec = m[..., 1].astype(np.float32) / 255
    rough = np.clip(0.95 - 0.7 * spec, 0.2, 0.95)
    metal = np.clip(0.25 + 0.55 * spec, 0, 1)
    mr = np.stack([np.zeros_like(spec), rough, metal], -1)
    mr = (mr * 255).astype(np.uint8)
    glow = np.clip((m[..., 0].astype(np.float32) - 150) / 105, 0, 1)
    emissive = None
    if (glow > 0.2).mean() > 0.0005 and base.startswith(("RB_", "VH_", "AP_", "WEP_", "MP_PickupBase")):
        emissive = (color.astype(np.float32) * glow[..., None]).astype(np.uint8)
    res = {"color": png(color), "normal": png(normal), "mr": png(mr),
           "emissive": png(emissive) if emissive is not None else None,
           "glow": png((np.repeat(glow[..., None], 3, -1) * 255).astype(np.uint8))}
    _tex_cache[base] = res
    return res


class Builder:
    def __init__(self):
        # Identical arrays (animation key times, still poses) are stored once.
        self.w = Writer(share=True)
        self.mats = {}

    def material(self, name):
        if name in self.mats:
            return self.mats[name]
        w = self.w
        if name in TEXSETS:
            t = texset_images(TEXSETS[name])
            m = {"name": name,
                 "pbrMetallicRoughness": {"baseColorTexture": {"index": w.texture(name + "_color", t["color"])},
                                          "metallicRoughnessTexture": {"index": w.texture(name + "_mr", t["mr"])},
                                          "metallicFactor": 1.0, "roughnessFactor": 1.0},
                 "normalTexture": {"index": w.texture(name + "_normal", t["normal"])}}
            if t["emissive"]:
                m["emissiveTexture"] = {"index": w.texture(name + "_emissive", t["emissive"])}
                m["emissiveFactor"] = [1, 1, 1]
                m["extensions"] = {"KHR_materials_emissive_strength": {"emissiveStrength": 8.0}}
        elif name in PLAIN:
            clr, nrm = PLAIN[name]
            m = {"name": name,
                 "pbrMetallicRoughness": {"baseColorTexture": {"index": w.texture(clr, png(np.asarray(Image.open(tex_path(clr + ".png")).convert("RGB"))))},
                                          "metallicFactor": 0.6, "roughnessFactor": 0.55},
                 "normalTexture": {"index": w.texture(nrm, png(np.asarray(Image.open(tex_path(nrm + ".png")).convert("RGB"))))}}
        elif name in SOLID:
            m = dict(SOLID[name], name=name)
        else:
            print("  ! no texture mapping for material", name, "- using grey")
            m = {"name": name, "pbrMetallicRoughness": {"baseColorFactor": [0.5, 0.5, 0.5, 1], "metallicFactor": 0.5, "roughnessFactor": 0.6}}
        if "extensions" in m:
            w.g.setdefault("extensionsUsed", [])
            for e in m["extensions"]:
                if e not in w.g["extensionsUsed"]:
                    w.g["extensionsUsed"].append(e)
        self.mats[name] = w.material(m)
        return self.mats[name]

    def primitives(self, doc, mesh_index, joint_map=None, uv_scale=None):
        out = []
        for p in doc.g["meshes"][mesh_index]["primitives"]:
            mname = doc.g["materials"][p["material"]]["name"]
            if mname in DROP:
                continue
            at = p["attributes"]
            attrs = {}
            attrs["POSITION"] = self.w.accessor(doc.acc(at["POSITION"]).astype(np.float32), 34962, minmax=True)
            attrs["NORMAL"] = self.w.accessor(doc.acc(at["NORMAL"]).astype(np.float32), 34962)
            if "TANGENT" in at:
                attrs["TANGENT"] = self.w.accessor(doc.acc(at["TANGENT"]).astype(np.float32), 34962)
            uv = doc.acc(at["TEXCOORD_0"]).astype(np.float32)
            if uv_scale:
                uv = uv * uv_scale
            attrs["TEXCOORD_0"] = self.w.accessor(uv, 34962)
            if "JOINTS_0" in at:
                j = doc.acc(at["JOINTS_0"]).astype(np.uint16)
                if joint_map is not None:
                    j = joint_map[j].astype(np.uint16)
                attrs["JOINTS_0"] = self.w.accessor(j, 34962)
                wt = doc.acc(at["WEIGHTS_0"]).astype(np.float32)
                wt = wt / np.maximum(wt.sum(1, keepdims=True), 1e-8)
                attrs["WEIGHTS_0"] = self.w.accessor(wt, 34962)
            idx = doc.acc(p["indices"]).reshape(-1).astype(np.uint32)
            out.append({"attributes": attrs, "indices": self.w.accessor(idx, 34963), "material": self.material(mname)})
        return out

    def root(self, name, yaw_deg=0.0, scale=1.0, translation=(0, 0, 0)):
        h = math.radians(yaw_deg) / 2
        return self.w.node({"name": name, "rotation": [0, math.sin(h), 0, math.cos(h)],
                            "scale": [scale] * 3, "translation": list(translation), "children": []}, root=True)


def mat_to_quat(m):
    t = np.trace(m)
    if t > 0:
        s = math.sqrt(t + 1) * 2
        return np.array([(m[2, 1] - m[1, 2]) / s, (m[0, 2] - m[2, 0]) / s, (m[1, 0] - m[0, 1]) / s, s / 4])
    i = int(np.argmax(np.diag(m)))
    j, k = (i + 1) % 3, (i + 2) % 3
    s = math.sqrt(1 + m[i, i] - m[j, j] - m[k, k]) * 2
    q = np.zeros(4)
    q[i] = s / 4
    q[j] = (m[j, i] + m[i, j]) / s
    q[k] = (m[k, i] + m[i, k]) / s
    q[3] = (m[k, j] - m[j, k]) / s
    return q


def rot_between(a, b):
    a = a / np.linalg.norm(a); b = b / np.linalg.norm(b)
    v = np.cross(a, b); c = float(np.dot(a, b))
    if np.linalg.norm(v) < 1e-9:
        return np.eye(3)
    vx = np.array([[0, -v[2], v[1]], [v[2], 0, -v[0]], [-v[1], v[0], 0]])
    return np.eye(3) + vx + vx @ vx * (1 / (1 + c))


def aim_joint(g, joint, child, target_dir):
    """Re-pose the skeleton's rest pose: rotate `joint` so the bone towards `child`
    points along `target_dir` (in the final, Bevy-facing model space)."""
    from gltf_io import node_globals, quat_to_mat
    names = {n.get("name"): i for i, n in enumerate(g["nodes"])}
    G = node_globals(g)
    j, c = names[joint], names[child]
    parent = next(i for i, n in enumerate(g["nodes"]) if j in n.get("children", []))
    cur = G[c][:3, 3] - G[j][:3, 3]
    delta = rot_between(cur, np.array(target_dir, float))
    new_global = delta @ G[j][:3, :3]
    pr = G[parent][:3, :3]
    # strip scale from the parent rotation (the Root node is unscaled here, but be safe)
    pr = pr / np.linalg.norm(pr, axis=0)
    ng = new_global / np.linalg.norm(new_global, axis=0)
    local = pr.T @ ng
    g["nodes"][j]["rotation"] = mat_to_quat(local).tolist()


_anim_docs = {}


# Clips that repeat while held (the rest play once). A (start, end) frame window
# cuts a repeating cycle out of a clip that doesn't loop as a whole: the game's
# sprint clip starts by accelerating, then settles into a 21-frame stride.
LOOPS = {"idle": None, "jog_fl": None, "jog_fr": None, "jog_l": None, "jog_r": None,
         "jog_bl": None, "jog_br": None, "sprint": (45, 66)}
# The shooting and fine-aim stances (gun raised from the hip / aimed down the sights)
# loop like the relaxed ones.
for _stance in ("shoot", "aim"):
    LOOPS[_stance + "_idle"] = None
    for _d in ("fl", "fr", "l", "r", "bl", "br"):
        LOOPS[f"{_stance}_move_{_d}"] = None
# The rest of each stance's strafe set (STRAFE_EXTRA): straight forward and back, and
# the left/right strafes with the hips turned back.
for _stance in ("jog", "shoot", "aim"):
    for _d in ("f", "b", "lb", "rb"):
        LOOPS[f"{_stance}_{_d}"] = None
# The heavy weapons' stance (STANCE_HeavyGunArmShootFromHip: the X18 Scrapmaker's and
# the Thermo Rocket Cannon's), standing, jogging and walking: loops too.
LOOPS["heavy_idle"] = None
for _set in ("heavy_jog", "heavy_walk"):
    for _d in ("fl", "fr", "l", "r", "bl", "br", "f", "b", "lb", "rb"):
        LOOPS[f"{_set}_{_d}"] = None
# Looping clips that don't loop on every bone: the frames at their start where some
# bones move from a pose of their own into the cycle. Nav_StrafeWalk_FineAim_BL
# starts with the left arm lowered and raises it over its first four frames, so
# looped it would drop the arm and swing it back up every step. Those bones get the
# frames bridged from the clip's end instead (see close_seam).
LOOP_SEAM_FRAMES = {"aim_move_bl": 4}


def close_seam(seg, k, is_quat):
    """A looping track whose end doesn't meet its start (a jump bigger than three
    times any step after the first `k` frames, and over 5 degrees / 1 cm) gets its
    first `k` frames replaced by a blend from its last frame to frame `k`, so the
    loop runs on without a jump. Tracks that already loop are left as they are."""
    n = len(seg)
    if n <= 2 * k:
        return seg
    if is_quat:
        def dist(a, b):
            return math.degrees(2 * math.acos(min(1.0, abs(float(np.dot(a, b))))))
        tol = 5.0
    else:
        def dist(a, b):
            return float(np.linalg.norm(a - b))
        tol = 0.01
    biggest = max(dist(seg[i], seg[i + 1]) for i in range(k, n - 1))
    if dist(seg[-1], seg[0]) <= max(3 * biggest, tol):
        return seg
    out = seg.copy()
    a0, a1 = seg[-1], seg[k]
    if is_quat and float(np.dot(a0, a1)) < 0:
        a1 = -a1
    for i in range(k):
        x = (i + 1) / (k + 1)
        q = a0 * (1 - x) + a1 * x
        out[i] = q / max(np.linalg.norm(q), 1e-9) if is_quat else q
    return out


# One-off clips whose still frames at the end are cut. The game swaps in the car
# as soon as Jazz is folded up; the clip then holds that pose for almost a second.
TRIM_STILL_END = {"to_vehicle"}
# The car's guns spin while firing.
LOOPS["gun_fire"] = None
# Name prefixes of the two copies of the car's gun skeleton (add_car_guns).
GUN_PREFIXES = ("GunL_", "GunR_")


def _resample(t, v, frames, loop, is_quat):
    """umodel writes compressed tracks with one key per frame from time 0, even
    when the game stored fewer keys spread over the whole clip. Spread them back
    out (Unreal 3 spacing: clip length / keys for looping clips, wrapping to the
    first key; length / (keys - 1) otherwise) and sample every frame."""
    k = len(v)
    if k == 1:
        return np.repeat(v, frames, axis=0)
    out = []
    for f in range(frames):
        x = f * k / frames if loop else f * (k - 1) / max(frames - 1, 1)
        i = min(int(x), k - 1)
        j = (i + 1) % k if loop else min(i + 1, k - 1)
        a = x - int(x)
        q0, q1 = v[i], v[j]
        if is_quat and float(np.dot(q0, q1)) < 0:
            q1 = -q1
        q = q0 * (1 - a) + q1 * a
        if is_quat:
            q = q / max(np.linalg.norm(q), 1e-9)
        out.append(q)
    return np.array(out, np.float32)


def add_animations(w, skel_offset, anims):
    """Copy animation clips from umodel exports onto this model's skeleton, matching
    bones by name. `anims` is a list of (new name, source .gltf path, clip name).
    The clips are written in list order, so the game can load them by index."""
    names = {n.get("name"): i for i, n in enumerate(w.g["nodes"]) if i >= skel_offset}
    w.g.setdefault("animations", [])
    for entry in anims:
        out_name, src, clip = entry[:3]
        # Optional: name prefixes of copies of another skeleton the clip drives (the
        # car's two guns), instead of this model's own bones.
        prefixes = entry[3] if len(entry) > 3 else None
        if src is None:
            # The skeleton's own rest pose, held (the game's reference pose: what an
            # animation node with no sequence plays, e.g. Jazz's car while driving).
            samplers, channels = [], []
            t = np.array([0.0, 1.0], np.float32)
            for bone, node in names.items():
                n = w.g["nodes"][node]
                if "skin" in n or "mesh" in n or bone.endswith("_Tyre") or bone.startswith(GUN_PREFIXES):
                    continue
                for path, rest in (("translation", n.get("translation", [0, 0, 0])),
                                   ("rotation", n.get("rotation", [0, 0, 0, 1]))):
                    v = np.array([rest, rest], np.float32)
                    samplers.append({"input": w.accessor(t, minmax=True), "output": w.accessor(v),
                                     "interpolation": "LINEAR"})
                    channels.append({"sampler": len(samplers) - 1, "target": {"node": node, "path": path}})
            w.g["animations"].append({"name": out_name, "samplers": samplers, "channels": channels})
            print(f"  + animation {out_name} (rest pose, {len(channels)} channels)")
            continue
        if src not in _anim_docs:
            _anim_docs[src] = Psa(src) if src.lower().endswith(".psa") else Doc(src)
        d = _anim_docs[src]
        # Source tracks as (bone name, "translation"/"rotation", keys).
        if isinstance(d, Psa):
            # umodel's .psa export: every frame sampled, already on the right timing.
            bones, dt = d.clip(clip)
            if bones is None:
                raise SystemExit(f"animation {clip} not found in {src}")
            if isinstance(src, PsaForGltf) and all(np.allclose(v, v[:1], atol=1e-6) for b in bones.values() for v in b):
                # One pose (umodel's glTF has one key for it; the .psa repeats it).
                bones = {bone: (pos[:1], rot[:1]) for bone, (pos, rot) in bones.items()}
            src_tracks = []
            for bone, (pos, rot) in bones.items():
                src_tracks.append((bone, "translation", pos))
                src_tracks.append((bone, "rotation", rot))
        else:
            a = next((x for x in d.g.get("animations", []) if x["name"] == clip), None)
            if a is None:
                raise SystemExit(f"animation {clip} not found in {src}")
            dt = 1.0 / 30.0
            full = max(len(d.acc(a["samplers"][c["sampler"]]["input"])) for c in a["channels"])
            for c in a["channels"]:
                t = d.acc(a["samplers"][c["sampler"]]["input"]).ravel()
                if len(t) == full and full > 1:
                    dt = float(t[1] - t[0])
                    break
            src_tracks = [(d.g["nodes"][c["target"]["node"]]["name"], c["target"]["path"],
                           d.acc(a["samplers"][c["sampler"]]["output"]).astype(np.float32)) for c in a["channels"]]
        frames = max(len(v) for _, _, v in src_tracks)
        loop = out_name in LOOPS
        window = LOOPS.get(out_name)
        last = frames  # frames kept (one-off clips may end early, see TRIM_STILL_END)
        channels = []  # (node, path, times, values), written out below
        for bone, path, v in src_tracks:
            # The reference bone carries umodel's root-motion data, which would
            # slide the model away from the player; the game moves the player itself.
            if prefixes:
                targets = [] if "Reference" in bone else [p + bone for p in prefixes if p + bone in names]
            else:
                targets = [] if "Root_Reference" in bone or bone not in names else [bone]
            if not targets:
                continue
            is_quat = path == "rotation"
            if frames == 1:
                # static pose: a 1 second clip
                t = np.array([0.0, 1.0], np.float32)
                v = np.repeat(v[:1], 2, axis=0)
            else:
                full = _resample(None, v, frames, loop, is_quat)
                if loop:
                    lo, hi = window if window else (0, frames)
                    n = hi - lo
                    seg = full[lo:hi].copy()
                    if window and not is_quat:
                        # remove the drift between the window's ends so it closes
                        end = full[hi % frames]
                        seg -= (end - full[lo])[None, :] * (np.arange(n)[:, None] / n)
                    if out_name in LOOP_SEAM_FRAMES:
                        seg = close_seam(seg, LOOP_SEAM_FRAMES[out_name], is_quat)
                    first = seg[:1].copy()
                    if is_quat and float(np.dot(seg[-1], first[0])) < 0:
                        first = -first
                    v = np.concatenate([seg, first])  # close the loop on its first pose
                    t = (np.arange(n + 1) * dt).astype(np.float32)
                else:
                    v = full
                    t = (np.arange(frames) * dt).astype(np.float32)
            for target in targets:
                channels.append((names[target], path, t, v))
        if out_name in TRIM_STILL_END and frames > 2:
            # Drop the frames at the end where nothing moves any more.
            motion = np.zeros(frames - 1)
            for _, path, t, v in channels:
                if len(v) == frames and path == "rotation":
                    dots = np.abs(np.sum(v[1:] * v[:-1], 1)).clip(0, 1)
                    motion += np.degrees(2 * np.arccos(dots))
            moving = np.nonzero(motion > 0.5)[0]
            last = int(moving[-1]) + 2 if len(moving) else frames
        samplers, final = [], []
        for node, path, t, v in channels:
            if len(v) == frames and last < frames:
                t, v = t[:last], v[:last]
            if len(v) > 2 and np.allclose(v, v[:1], atol=1e-6):
                # a bone that doesn't move: two keys are enough
                t = np.array([0.0, t[-1]], np.float32)
                v = v[:2]
            samplers.append({"input": w.accessor(t, minmax=True), "output": w.accessor(v.astype(np.float32)), "interpolation": "LINEAR"})
            final.append({"sampler": len(samplers) - 1, "target": {"node": node, "path": path}})
        channels = final
        w.g["animations"].append({"name": out_name, "samplers": samplers, "channels": channels})
        length = max(float(w.g["accessors"][s["input"]]["max"][0]) for s in samplers)
        print(f"  + animation {out_name} ({clip}, {length:.2f}s{', loop' if loop else ''}, {len(channels)} channels)")


def _qmul(a, b):
    """Hamilton product of (x, y, z, w) quaternion arrays."""
    ax, ay, az, aw = np.moveaxis(a, -1, 0)
    bx, by, bz, bw = np.moveaxis(b, -1, 0)
    return np.stack([aw * bx + ax * bw + ay * bz - az * by,
                     aw * by - ax * bz + ay * bw + az * bx,
                     aw * bz + ax * by - ay * bx + az * bw,
                     aw * bw - ax * bx - ay * by - az * bz], -1)


def _sample_frames(v, x, loop, is_quat):
    """Values at fractional frame positions x (wrapping when looping)."""
    n = len(v)
    out = []
    for xi in x:
        if loop:
            xi = xi % n
            i = int(xi)
            j = (i + 1) % n
        else:
            xi = min(max(xi, 0.0), n - 1.0)
            i = int(xi)
            j = min(i + 1, n - 1)
        a = xi - i
        q0, q1 = v[i], v[j]
        if is_quat and float(np.dot(q0, q1)) < 0:
            q1 = -q1
        q = q0 * (1 - a) + q1 * a
        if is_quat:
            q = q / max(np.linalg.norm(q), 1e-9)
        out.append(q)
    return np.array(out, np.float64)


def add_additive_animations(w, skel_offset, clips):
    """Additive clips (the game's ADD_* animations: weapon recoil, barrels spinning).
    They hold differences from whatever pose is playing, not poses: a rotation to put
    in front of the bone's rotation and an offset to add to its position. main.rs
    plays them under an additive blend node, which combines them the same way the
    game does (checked against the rifle's spinning barrels).

    `clips` is a list of (name, frames, loop, parts); each part is
    (.psa path, clip name, play rate, bone filter[, restart frames]). Parts are
    layered (recoil plus the gun's own moving parts) over `frames` frames at 30 fps.
    A part with restart frames starts over from its first frame at each of them
    (one recoil per shot of a burst)."""
    names = {n.get("name"): i for i, n in enumerate(w.g["nodes"]) if i >= skel_offset}
    for out_name, frames, loop, parts in clips:
        rot, pos = {}, {}
        for src, clip, rate, keep, *restarts in parts:
            if src not in _anim_docs:
                _anim_docs[src] = Psa(src)
            bones, dt = _anim_docs[src].clip(clip)
            if bones is None:
                raise SystemExit(f"animation {clip} not found in {src}")
            f = np.arange(frames, dtype=np.float64)
            if restarts:
                starts = np.array(sorted(restarts[0]), np.float64)
                f = f - starts[np.maximum(np.searchsorted(starts, f, side="right") - 1, 0)]
            x = f * (1.0 / 30.0) * rate / dt
            for bone, (p, r) in bones.items():
                if bone not in names or "Root_Reference" in bone or not keep(bone):
                    continue
                rs = _sample_frames(r.astype(np.float64), x, loop, True)
                ps = _sample_frames(p.astype(np.float64), x, loop, False)
                rot[bone] = _qmul(rs, rot[bone]) if bone in rot else rs
                pos[bone] = pos[bone] + ps if bone in pos else ps
        samplers, channels = [], []
        n = frames + 1 if loop else frames
        t = (np.arange(n) / 30.0).astype(np.float32)
        for bone in rot:
            r, p = rot[bone], pos[bone]
            r = r * np.sign(r[:, 3:4] + 1e-12)  # keep w >= 0 (no long-way-round blends)
            if loop:
                r, p = np.concatenate([r, r[:1]]), np.concatenate([p, p[:1]])
            for path, v, still in (("rotation", r, np.abs(r[:, :3]).max() < 1e-4),
                                   ("translation", p, np.abs(p).max() < 1e-4)):
                if still:
                    continue  # nothing to add for this bone
                samplers.append({"input": w.accessor(t, minmax=True),
                                 "output": w.accessor(v.astype(np.float32)), "interpolation": "LINEAR"})
                channels.append({"sampler": len(samplers) - 1, "target": {"node": names[bone], "path": path}})
        if not channels:
            # glTF needs at least one channel: add "no change" on the hips.
            samplers.append({"input": w.accessor(np.array([0.0, 1.0], np.float32), minmax=True),
                             "output": w.accessor(np.array([[0, 0, 0, 1]] * 2, np.float32)),
                             "interpolation": "LINEAR"})
            first = names.get("C_Spine00_Hips_XB", next(iter(names.values())))
            channels.append({"sampler": 0, "target": {"node": first, "path": "rotation"}})
        w.g["animations"].append({"name": out_name, "samplers": samplers, "channels": channels})
        print(f"  + additive animation {out_name} ({n / 30.0:.2f}s{', loop' if loop else ''}, {len(channels)} channels)")


def add_aim_offsets(w, skel_offset, offsets):
    """Additive turns made from two of the game's aim poses (its AnimNodeAimOffset
    poses, Aim_<stance>_<F/L/R>_<C/U/D>): what changes from the straight-ahead pose
    to the turned one, as a held additive clip (see add_additive_animations for the
    format). `offsets` is a list of (name, .psa path, turned pose, straight pose).
    main.rs weights them by how far the camera is turned from Jazz's body, so his
    upper body follows the aim while his feet stay put (the full pose is the full
    turn, about 90 degrees)."""
    names = {n.get("name"): i for i, n in enumerate(w.g["nodes"]) if i >= skel_offset}
    for out_name, src, turned, straight in offsets:
        if src is None:
            # No poses: an additive clip that changes nothing.
            hips = names.get("C_Spine00_Hips_XB", next(iter(names.values())))
            w.g["animations"].append({"name": out_name, "samplers": [
                {"input": w.accessor(np.array([0.0, 1.0], np.float32), minmax=True),
                 "output": w.accessor(np.array([[0, 0, 0, 1]] * 2, np.float32)), "interpolation": "LINEAR"}],
                "channels": [{"sampler": 0, "target": {"node": hips, "path": "rotation"}}]})
            continue
        if src not in _anim_docs:
            _anim_docs[src] = Psa(src)
        a, _ = _anim_docs[src].clip(turned)
        b, _ = _anim_docs[src].clip(straight)
        if a is None or b is None:
            raise SystemExit(f"aim pose {turned} or {straight} not found in {src}")
        t = np.array([0.0, 1.0], np.float32)
        samplers, channels = [], []
        for bone in a:
            if bone not in names or "Root_Reference" in bone:
                continue
            qa, qb = a[bone][1][0].astype(np.float64), b[bone][1][0].astype(np.float64)
            r = _qmul(qa, qb * np.array([-1, -1, -1, 1]))
            r = r / np.linalg.norm(r) * np.sign(r[3] + 1e-12)
            d = (a[bone][0][0] - b[bone][0][0]).astype(np.float64)
            for path, v, still in (("rotation", r, np.abs(r[:3]).max() < 1e-4),
                                   ("translation", d, np.abs(d).max() < 1e-5)):
                if still:
                    continue
                samplers.append({"input": w.accessor(t, minmax=True),
                                 "output": w.accessor(np.array([v, v], np.float32)), "interpolation": "LINEAR"})
                channels.append({"sampler": len(samplers) - 1, "target": {"node": names[bone], "path": path}})
        w.g["animations"].append({"name": out_name, "samplers": samplers, "channels": channels})
        print(f"  + aim offset {out_name} ({len(channels)} channels)")


def build_skinned(out_name, part_files, yaw_deg, scale=1.0, add_wheels=False, pose=None, anims=None,
                  additive=None, add_guns=False, robot_wheels=False, more_anims=None, aim_offsets=None,
                  cloud_b=None, more_additive=None, tail_additive=None, tail_anims=None):
    """part_files: list of glTF paths whose meshes are skinned to (versions of) one skeleton.
    The first file's skeleton is kept; other parts are re-bound by joint name."""
    b = Builder()
    w = b.w
    root = b.root("Root", yaw_deg, scale)
    ref = Doc(part_files[0])
    # Copy the skeleton (every node except the mesh node 0).
    skel_offset = len(w.g["nodes"])
    for n in ref.g["nodes"][1:]:
        nn = {k: v for k, v in n.items() if k in ("name", "translation", "rotation", "scale", "matrix")}
        if "children" in n:
            nn["children"] = [c - 1 + skel_offset for c in n["children"]]
        w.node(nn)
    skel_root = skel_offset + (ref.g["nodes"][0]["children"][0] - 1)
    w.g["nodes"][root]["children"].append(skel_root)
    skin = ref.g["skins"][0]
    ref_joint_names = [ref.g["nodes"][j]["name"] for j in skin["joints"]]
    w.g["skins"].append({"joints": [j - 1 + skel_offset for j in skin["joints"]],
                         "inverseBindMatrices": w.accessor(ref.acc(skin["inverseBindMatrices"]).astype(np.float32)),
                         "skeleton": skel_root})
    for f in part_files:
        d = Doc(f)
        names = [d.g["nodes"][j]["name"] for j in d.g["skins"][0]["joints"]]
        jmap = np.array([ref_joint_names.index(n) if n in ref_joint_names else 0 for n in names])
        missing = [n for n in names if n not in ref_joint_names]
        if missing:
            print("  ! joints not in reference skeleton:", missing[:5])
        prims = b.primitives(d, d.g["nodes"][0]["mesh"], jmap)
        if not prims:
            continue
        w.g["meshes"].append({"name": os.path.basename(f)[:-5], "primitives": prims})
        mn = w.node({"name": os.path.basename(f)[:-5], "mesh": len(w.g["meshes"]) - 1, "skin": 0})
        w.g["nodes"][root]["children"].append(mn)
        if cloud_b and os.path.basename(f)[:-5] in cloud_b:
            # A second copy of the gun skinned to Jazz's other set of gun-arm bones
            # (R_CloudB_*, which sit on R_CloudA_* in every body animation). The game
            # puts the weapon being put away there, so it can fold back into the arm
            # (its Unequip clip, PlayOnCloudB) while the new one unfolds on CloudA.
            # His forearms get one too (FOREARMS_CLOUD_B), for switching hands.
            # Same vertices; only the joints differ.
            swap = [n.replace("R_CloudA_", "R_CloudB_").replace("L_CloudA_", "L_CloudB_") for n in names]
            jmap_b = np.array([ref_joint_names.index(s) if s in ref_joint_names else jmap[k]
                               for k, s in enumerate(swap)])
            src = [p for p in d.g["meshes"][d.g["nodes"][0]["mesh"]]["primitives"]
                   if d.g["materials"][p["material"]]["name"] not in DROP]
            prims_b = []
            for pa, ps in zip(prims, src):
                j = jmap_b[d.acc(ps["attributes"]["JOINTS_0"]).astype(np.uint16)].astype(np.uint16)
                attrs = dict(pa["attributes"], JOINTS_0=w.accessor(j, 34962))
                prims_b.append(dict(pa, attributes=attrs))
            name_b = os.path.basename(f)[:-5] + "_CloudB"
            w.g["meshes"].append({"name": name_b, "primitives": prims_b})
            mb = w.node({"name": name_b, "mesh": len(w.g["meshes"]) - 1, "skin": 0})
            w.g["nodes"][root]["children"].append(mb)
    if add_wheels:
        add_wheel_meshes(b, skel_offset, scale)
    if add_guns:
        add_car_guns(b, skel_offset, root)
    if robot_wheels:
        add_robot_wheels(b, skel_offset)
    for joint, child, target in (pose or []):
        aim_joint(w.g, joint, child, target)
    if anims:
        add_animations(w, skel_offset, anims)
        if additive:
            add_additive_animations(w, skel_offset, additive)
        # Clips added since the additive ones (so the indices of those stay put).
        if more_anims:
            add_animations(w, skel_offset, more_anims)
        if aim_offsets:
            add_aim_offsets(w, skel_offset, aim_offsets)
        if more_additive:
            add_additive_animations(w, skel_offset, more_additive)
        # The eight weapons added later and the heavy weapons' stance, after
        # everything else (so every clip before them keeps its index: the classes'
        # files name robot.glb's clips by number).
        if tail_additive:
            add_additive_animations(w, skel_offset, tail_additive)
        if tail_anims:
            add_animations(w, skel_offset, tail_anims)
    size = w.save(os.path.join(OUT, "models", out_name))
    print(f"wrote models/{out_name} ({size/1e6:.1f} MB)")


def vehicle_animations(anim_dir):
    """The car's clips (Jazz_VEH_ANIM, saved by umodel as Animations\\Jazz_VEH_ANIM\\
    Jazz_VEH_ANIM.psa), in the order main.rs (src/car.rs) loads them by index.

    The game's car animation tree (VEH_Proxy_p.VEH_Car_ANIMTREE) has a Driving
    state and a Hovering state. Driving plays the reference pose for Jazz (his set
    has no boost or steering clips; the wheels are turned, steered and sprung by
    SkelControlWheels, done in car.rs). Hovering holds Nav_Hover_Pose_VEH blended
    with the four directional hover poses by the car's velocity, with the
    ADD_Nav_Hover_VEH bob added on top; Nav_BoostToHover_VEH and
    Nav_HoverToBoost_VEH play between the two. The transformation clips play on the
    car at the same time as the robot's, and the guns fold out and away with the
    additive Equip/Unequip clips."""
    psa = psa_file(anim_dir, "Jazz_VEH_ANIM")
    gun_psa = psa_file(anim_dir, "WEP_AssaultRifleVH_ANIM")
    if not os.path.exists(psa) or not os.path.exists(gun_psa):
        print("  ! Jazz_VEH_ANIM.psa or WEP_AssaultRifleVH_ANIM.psa not found - the car will not animate")
        return None, None
    anims = [
        ("drive", None, None),
        ("to_vehicle", psa, "Transform_ToVehicle_VEH"),
        ("to_robot", psa, "Transform_ToRobot_VEH"),
        ("hover", psa, "Nav_Hover_Pose_VEH"),
        ("hover_f", psa, "Nav_Hover_F_VEH"),
        ("hover_b", psa, "Nav_Hover_B_VEH"),
        ("hover_l", psa, "Nav_Hover_L_VEH"),
        ("hover_r", psa, "Nav_Hover_R_VEH"),
        ("drive_to_hover", psa, "Nav_BoostToHover_VEH"),
        ("hover_to_drive", psa, "Nav_HoverToBoost_VEH"),
        # The guns' own clips (WEP_AssaultRifleVH_ANIM, TnWeaponEffectPlayAnimationOnWeaponMesh):
        # folding out, firing, folding away. Both guns play them.
        ("gun_equip", gun_psa, "Weapon_Equip", GUN_PREFIXES),
        ("gun_fire", gun_psa, "Weapon_Fire", GUN_PREFIXES),
        ("gun_unequip", gun_psa, "Weapon_UnEquip", GUN_PREFIXES),
    ]
    # The additive clips leave the car's reference bone alone (they carry an
    # absolute offset on it, not a change).
    keep = lambda b: b != "C_Reference_XR"
    additive = [
        ("hover_bob", 180, True, [(psa, "ADD_Nav_Hover_VEH", 1.0, keep)]),
        ("guns_out", 12, False, [(psa, "ADD_Weapon_Equip_VEH", 1.0, keep)]),
        ("guns_in", 30, False, [(psa, "ADD_Weapon_Unequip_VEH", 1.0, keep)]),
    ]
    return anims, additive


def build_vehicle(anim_dir):
    anims, additive = vehicle_animations(anim_dir)
    build_skinned("vehicle.glb", [os.path.join(SK, "VH_Jazz_SKEL.gltf")], float(os.environ.get("FOC_YAW", "90")),
                  scale=VEHICLE_SCALE, add_wheels=True, add_guns=True, anims=anims, additive=additive)


def add_car_guns(b, skel_offset, root):
    """Jazz's car guns: WEP_AssaultRifleVH_SKEL (the car's weapon mesh,
    AssaultRifle_Vehicle_WEPMESH), one on each of the car's weapon sockets
    (WeaponSocket_Primary on L_GunRobo01_XT, WeaponSocket_Primary2 on R_GunRobo01_XT,
    which is turned half round on that bone). Each copy gets its own skeleton, its
    bones named GunL_* / GunR_*."""
    w = b.w
    path = os.path.join(SK, "WEP_AssaultRifleVH_SKEL.gltf")
    if not os.path.exists(path):
        print("  ! WEP_AssaultRifleVH_SKEL.gltf not found - the car has no guns")
        return
    d = Doc(path)
    nodes = d.g["nodes"]
    skin = d.g["skins"][0]
    names = {n.get("name"): i for i, n in enumerate(w.g["nodes"]) if i >= skel_offset}
    mesh = None
    for prefix, bone, turn in ((GUN_PREFIXES[0], "L_GunRobo01_XT", [0, 0, 0, 1]),
                               (GUN_PREFIXES[1], "R_GunRobo01_XT", [0, 0, 1, 0])):
        if bone not in names:
            continue
        mount = w.node({"name": prefix + "Socket", "rotation": turn, "children": []})
        w.g["nodes"][names[bone]].setdefault("children", []).append(mount)
        offset = len(w.g["nodes"])
        for n in nodes[1:]:
            nn = {k: v for k, v in n.items() if k in ("translation", "rotation", "scale")}
            nn["name"] = prefix + n["name"]
            if "children" in n:
                nn["children"] = [c - 1 + offset for c in n["children"]]
            w.node(nn)
        gun_root = offset + nodes[0]["children"][0] - 1
        w.g["nodes"][mount]["children"].append(gun_root)
        w.g["skins"].append({"joints": [j - 1 + offset for j in skin["joints"]],
                             "inverseBindMatrices": w.accessor(d.acc(skin["inverseBindMatrices"]).astype(np.float32)),
                             "skeleton": gun_root})
        if mesh is None:
            w.g["meshes"].append({"name": "WEP_AssaultRifleVH_SKEL", "primitives": b.primitives(d, nodes[0]["mesh"])})
            mesh = len(w.g["meshes"]) - 1
        mn = w.node({"name": prefix + "Mesh", "mesh": mesh, "skin": len(w.g["skins"]) - 1})
        w.g["nodes"][root]["children"].append(mn)


# Jazz's robot-form wheels: his RB_Jazz_PART_Wheel mesh part hangs the wheel mesh
# (RB_Jazz_STAT_Wheel) on every "Wheel" socket of his robot meshes: one on each
# forearm (RB_Jazz_CHARSKEL_ArmLoL/R) and one behind each knee (..._Legs). The
# sockets as the game stores them: bone, location (Unreal units), rotation
# (pitch, yaw, roll; 65536 = a full turn) and scale. The forearms' wheels also hang
# on the CloudB copies of their bones, with the forearms' CloudB copies (see
# FOREARMS_CLOUD_B).
ROBOT_WHEEL_SOCKETS = [
    ("L_CloudA_ElbowRobo16_XT", (7.065, -13.292, 12.686), (0, -16384, -22691), (0.9, 0.9, 0.7)),
    ("R_CloudA_ElbowRobo16_XT", (-7.065, 13.292, -12.686), (0, -16384, -22691), (0.9, 0.9, -0.7)),
    ("L_CloudB_ElbowRobo16_XT", (7.065, -13.292, 12.686), (0, -16384, -22691), (0.9, 0.9, 0.7)),
    ("R_CloudB_ElbowRobo16_XT", (-7.065, 13.292, -12.686), (0, -16384, -22691), (0.9, 0.9, -0.7)),
    ("R_KneeRobo04_XT", (-10.204, -48.780, 72.508), (14701, 32768, 0), (1.0, 1.0, 1.0)),
    ("L_KneeRobo04_XT", (10.027, 53.352, -71.423), (14701, -32768, 0), (-1.0, -1.0, -1.0)),
]


def _ue_rotator_quat(pitch, yaw, roll):
    """Unreal rotator (65536 units a turn) to a quaternion in Unreal's axes."""
    k = math.pi / 32768.0 / 2.0
    sp, cp = math.sin(pitch * k), math.cos(pitch * k)
    sy, cy = math.sin(yaw * k), math.cos(yaw * k)
    sr, cr = math.sin(roll * k), math.cos(roll * k)
    return (cr * sp * sy - sr * cp * cy, -cr * sp * cy - sr * cp * sy,
            cr * cp * sy - sr * sp * cy, cr * cp * cy + sr * sp * sy)


def add_robot_wheels(b, skel_offset):
    """Hangs the wheel mesh on Jazz's robot wheel sockets (ROBOT_WHEEL_SOCKETS),
    converting each socket from Unreal's axes to the model's the way umodel's glTF
    export converts the meshes and bones (Y and Z swapped)."""
    w = b.w
    path = os.path.join(ST, "RB_Jazz_STAT_Wheel.gltf")
    if not os.path.exists(path):
        print("  ! RB_Jazz_STAT_Wheel.gltf not found - Jazz's robot form has no wheels")
        return
    d = Doc(path)
    w.g["meshes"].append({"name": "robot_wheel", "primitives": b.primitives(d, d.g["nodes"][0]["mesh"])})
    mesh = len(w.g["meshes"]) - 1
    names = {n.get("name"): i for i, n in enumerate(w.g["nodes"]) if i >= skel_offset}
    for bone, loc, rot, scale in ROBOT_WHEEL_SOCKETS:
        if bone not in names:
            continue
        qx, qy, qz, qw = _ue_rotator_quat(*rot)
        # umodel's glTF axes: Unreal's Y and Z swapped (a mirror, so the rotation's
        # axis flips with it).
        node = w.node({"name": bone + "_Wheel", "mesh": mesh,
                       "translation": [loc[0] * 0.01, loc[2] * 0.01, loc[1] * 0.01],
                       "rotation": [qx, qz, qy, -qw],
                       "scale": [scale[0], scale[2], scale[1]]})
        w.g["nodes"][names[bone]].setdefault("children", []).append(node)


def add_wheel_meshes(b, skel_offset, model_scale=1.0):
    """The car's wheels are a separate static mesh in the game (RB_Jazz_STAT_Wheel),
    attached to the skeleton's *_Wheel* bones. Hang one on each wheel bone.
    `model_scale` is the car's scale; the wheel mesh is already at game size, so it
    is scaled back by the same amount. The wheel's axle is along its Y axis; it is
    turned so the axle lines up with the bone's Z axis (the axis the game spins)."""
    w = b.w
    path = os.path.join(ST, "RB_Jazz_STAT_Wheel.gltf")
    if not os.path.exists(path):
        print("  ! RB_Jazz_STAT_Wheel.gltf not found - car has no wheels")
        return
    d = Doc(path)
    w.g["meshes"].append({"name": "wheel", "primitives": b.primitives(d, d.g["nodes"][0]["mesh"])})
    mesh = len(w.g["meshes"]) - 1
    h = math.sqrt(0.5)
    inv = 1.0 / model_scale
    for i, n in enumerate(w.g["nodes"]):
        name = n.get("name", "")
        if i < skel_offset or "_Wheel" not in name or not name.endswith("_XB"):
            continue
        # Wheels on the right side are mirrored so the hub faces outwards on both sides.
        side = -1.0 if name.startswith(WHEEL_MIRROR_PREFIX) else 1.0
        child = w.node({"name": name + "_Tyre", "mesh": mesh, "rotation": [h, 0, 0, h],
                        "scale": [inv, inv * side, inv]})
        n.setdefault("children", []).append(child)


# Jazz's weapons, in the order main.rs's WEAPONS lists them: the first eight, then
# the eight added after them (NEW_WEAPONS: the X18 Scrapmaker, Energon Harvester,
# E.D.K. TechVolt, Corrosive Slime Cannon, Gear Shredder, Thermo Rocket Cannon, A-4
# Pulsar Cannon and Chaos-Rift Combustor).
WEAPON_MESHES = ("RB_AssaultRifleR_SKEL.gltf", "RB_IonBlasterR_SKEL.gltf", "RB_BurstRifleR_SKEL.gltf",
                 "RB_ScatterblasterR_SKEL.gltf", "RB_SniperRifleR_SKEL.gltf", "RB_HeavyPistolR_SKEL.gltf",
                 "RB_BazookaR_SKEL.gltf", "RB_SysCorruptorR_SKEL.gltf",
                 "RB_HeavyMGR_SKEL.gltf", "RB_EmpShotgunR_SKEL.gltf", "RB_RepairBeamR_SKEL.gltf",
                 "RB_RustCannonR_SKEL.gltf", "RB_SawbladeR_SKEL.gltf", "RB_RocketLauncherR_SKEL.gltf",
                 "RB_GrenadeLauncherR_SKEL.gltf", "RB_GrenadeArmR_SKEL.gltf")
# The same guns built onto Jazz's left forearm (the weapons' left-handed meshes,
# HANDEDNESS_Left), for when he switches hands.
WEAPON_MESHES_LEFT = ("RB_AssaultRifleL_SKEL.gltf", "RB_IonBlasterL_SKEL.gltf", "RB_BurstRifleL_SKEL.gltf",
                      "RB_ScatterblasterL_SKEL.gltf", "RB_SniperRifleL_SKEL.gltf", "RB_HeavyPistolL_SKEL.gltf",
                      "RB_BazookaL_SKEL.gltf", "RB_SysCorruptorL_SKEL.gltf",
                      "RB_HeavyMGL_SKEL.gltf", "RB_EmpShotgunL_SKEL.gltf", "RB_RepairBeamL_SKEL.gltf",
                      "RB_RustCannonL_SKEL.gltf", "RB_SawbladeL_SKEL.gltf", "RB_RocketLauncherL_SKEL.gltf",
                      "RB_GrenadeLauncherL_SKEL.gltf", "RB_GrenadeArmL_SKEL.gltf")
# Frames at which each shot of a Photon Burst Rifle burst starts (FireInterval 0.1 s).
BURST_SHOTS = (0, 3, 6)
# Weapons lying on the ground (main.rs, models/weapons/<name>.glb) -> their mesh in robot.glb.
WEAPON_PICKUPS = {"assault_rifle": "RB_AssaultRifleR_SKEL", "ion_blaster": "RB_IonBlasterR_SKEL",
                  "burst_rifle": "RB_BurstRifleR_SKEL", "scatter_blaster": "RB_ScatterblasterR_SKEL",
                  "sniper_rifle": "RB_SniperRifleR_SKEL", "path_blaster": "RB_HeavyPistolR_SKEL",
                  "riot_cannon": "RB_BazookaR_SKEL", "orbital_pistol": "RB_SysCorruptorR_SKEL",
                  "scrapmaker": "RB_HeavyMGR_SKEL", "harvester": "RB_EmpShotgunR_SKEL",
                  "techvolt": "RB_RepairBeamR_SKEL", "slime_cannon": "RB_RustCannonR_SKEL",
                  "gear_shredder": "RB_SawbladeR_SKEL", "thermo_rocket": "RB_RocketLauncherR_SKEL",
                  "pulsar_cannon": "RB_GrenadeLauncherR_SKEL", "combustor": "RB_GrenadeArmR_SKEL"}
# Weapon pickups (TnWeaponMesh.PickupMesh) use the weapon mesh with a PICKUP_*_MATINST:
# the weapon's own texture set, its glow areas lit in the energon colour (cyan,
# Emissive_Intensity 5). In the game they are also drawn with a glowing cyan
# outline; here that is a shell around the gun, pushed out along its (smoothed)
# normals and turned inside out, so only its far side shows around the gun's edges.
PICKUP_GLOW = ([0.0, 1.0, 1.0], 5.0)
OUTLINE_WIDTH = 0.025       # model units (metres: the pickups are at their own size)
OUTLINE_GLOW = ([0.0, 1.0, 1.0], 6.0)
# The heavy weapons' (WT_Secondary's) pickups are yellow, their outline and their
# glow areas alike: their PICKUP_*_MATINSTs' OutlineColor is (1, 0.396, 0) (the heavy
# ammo crate's too) against the others' cyan (the Corrosive Slime Cannon's and Gear
# Shredder's EnergonColor, (0.843, 0.302, 0.029) and (0.8, 0.314, 0), are much the
# same), which the game's picture shows as a pale yellow: the outline a thin light
# yellow line (about sRGB 220, 210, 80 in the user's captures), the glow areas paler
# still (about 240, 230, 150 where brightest). Bevy's tone mapping keeps that colour's
# orange, so these are the colours the game shows, to look the same here.
HEAVY_GLOW = ([1.0, 0.85, 0.3], 3.0)
HEAVY_OUTLINE = ([1.0, 0.85, 0.08], 2.4)
PICKUP_LOOKS = {name: (HEAVY_GLOW, HEAVY_OUTLINE)
                for name in ("scrapmaker", "harvester", "slime_cannon", "gear_shredder", "thermo_rocket",
                             "pulsar_cannon", "combustor")}
OUTLINE_MATERIAL = "PickupOutline"
# The material instance each weapon's pickup is drawn with (its TnWeaponMesh's
# PickupMaterialOverride, put on the mesh's first section: TnWeaponMesh.
# GetValidPickupMesh), by pickup: an instance of the characters' material
# (Iconic_Cars_TEXSET_Mat, TR_BRDF_CustChar_TEXSET_MAT's), as the gun in hand is.
PICKUP_MATERIALS = {"assault_rifle": "PICKUP_AssaultRifle_MATINST",
                    "ion_blaster": "PICKUP_SubsonicRepeater_MATINST",
                    "burst_rifle": "PICKUP_BurstRifle_MATINST", "scatter_blaster": "PICKUP_Shotgun_MatINST",
                    "sniper_rifle": "PICKUP_SniperRifle_MatINST", "path_blaster": "PICKUP_IonBlaster_MATINST",
                    "riot_cannon": "PICKUP_RiotCannon_MATINST", "orbital_pistol": "PICKUP_SysCorruptor_MATINST",
                    "scrapmaker": "PICKUP_HeavyMG_MATINST", "harvester": "PICKUP_EMPShotgun_MATINST",
                    "techvolt": "PICKUP_TechvoltEmitter_MatINST", "slime_cannon": "PICKUP_RustCannon_MATINST",
                    "gear_shredder": "PICKUP_Sawblade_MATINST", "thermo_rocket": "PICKUP_RocketLauncher_MATINST",
                    "pulsar_cannon": "PICKUP_GrenadeLauncher_MATINST",
                    "combustor": "PICKUP_ClusterCannon_MATINST"}
# The forearm cuff every gun mesh is mounted on, its second section (Jazz's arm in
# it): the pickup keeps it in its own material, which the pickup's override leaves
# alone.
ARM_MATERIAL = "RB_ArmUp_Cust_MatINST_INST"
# Every PICKUP_*_MATINST's paint, and the cuff's: Cust_Color_A and Cust_COLOR_B
# 0.3961 (linear grey), MetallicOffset 0.4. The demo paints the pickups so
# (character.rs's PaintCache::pickup, the "pickup_paint" extras on their materials).
PICKUP_PAINT = 0.3961
PICKUP_METALLIC = 0.4
# The bone the right-handed guns lie along: their barrels down its -X, their tops
# up its +Y (main.rs's GunSockets are in its space).
GUN_BONE = "R_Arm03_Elbow_XB"
# Jazz's forearms, also skinned to the CloudB arm bones (as the game has them while
# he switches hands: TR_Iconic_Car_ANM_p.Jazz_Arm_ROBO_ANIM's
# ADD_Weapon_MirrorReverse_ARM folds one forearm away on its CloudB bones while the
# other unfolds, and shrinks their bones to nothing or grows them back as it goes).
FOREARMS_CLOUD_B = ("RB_Jazz_CHARSKEL_ArmLoL", "RB_Jazz_CHARSKEL_ArmLoR")


# The eight weapons added after the first eight, in main.rs's WEAPONS order: each one's
# key, its animation set (exported from all_shared.xxx into the shared export), and
# its clips' prefix with the ends of its equip, unequip and switching-hands clips'
# names (TnWeaponEventEquip / UnEquip / ChangeHandedness).
NEW_WEAPONS = (
    ("heavymg", "Weapon_HeavyMG_ANIM", "ADD_Weapon_HeavyMG_", ("Equip_WEP", "Unequip_WEP", "MirrorReverse_WEP")),
    ("harvester", "WEP_EMPShotgun_ANIM2", "ADD_Weapon_EMPShotgun_", ("Equip", "Unequip", "MirrorReverse")),
    ("techvolt", "Weapon_RepairBeam_ANIM", "ADD_Weapon_RepairBeam_", ("Equip_WEP", "Unequip_WEP", "MirrorReverse_WEP")),
    ("slime", "Weapon_RustCannon_ANIM", "ADD_Weapon_RustCannon_", ("Equip", "Unequip", "MirrorReverse")),
    ("shredder", "Weapon_Sawblade_ANIM", "ADD_Weapon_Sawblade_", ("Equip_WEP", "Unequip_WEP", "MirrorReverse_WEP")),
    ("thermo", "Weapon_RocketLauncher_ANIM", "ADD_Weapon_RocketLauncher_",
     ("Equip_WEP", "Unequip_WEP", "MirrorReverse_WEP")),
    ("pulsar", "Weapon_GrenadeLauncher_ANIM", "ADD_Weapon_PulsarCannon_",
     ("Equip_WEP", "Unequip_WEP", "MirrorReverse_WEP")),
    ("combustor", "WEP_GrenadeArm_ANIM", "ADD_Weapon_GrenadeLauncher_",
     ("Equip_WEP", "Unequip_WEP", "MirrorReverse_WEP")),
)
# The ten strafe directions of a stance, in the order main.rs blends them: the six of
# ANIM_JOG_FL on, then STRAFE_F, STRAFE_B, STRAFE_LB, STRAFE_RB.
STRAFE_TEN = ("FL", "FR", "L", "R", "BL", "BR", "F", "B", "LB", "RB")


def new_weapon_clips(anim_dir, scout_psa, gun, body, cloud_b, clouds):
    """robot.glb's clips for NEW_WEAPONS and the heavy weapons' stance, written after
    all the others (main.rs's ANIM_NEW_WEAPONS on): (additive clips, ordinary clips).

    Additive, six per weapon in NEW_WEAPONS order: its firing clip, its idle loop
    (TnWeaponEffectIdleAnimation), its reload (Reload_Size's small version, as Jazz
    is a small character: main.rs stretches it to the gun's reload time), its equip
    (CAMT_Weapon: the R_CloudA_* gun bones), its unequip (PlayOnCloudB: R_CloudB_*)
    and its switching hands (MirrorReverse); then the extra ones: the Scrapmaker's
    cool-down and warm-up, the TechVolt's cool-down, and the Gear Shredder's charged
    shot and its charging loop. Each firing clip is its weapon's TnWeaponEffectPlay-
    Animation clips as the game plays them (the arm recoils are the WeaponArm_ROBO_ANIM
    clips the Weapon_Recoil_*_S names stand for, as the other guns'):
      * Scrapmaker: from its first shot (StartFire) to letting go, looping, the arm's
        Weapon_Recoil_Rapid_Heavy_S (ADD_Weapon_Recoil_RapidSmall_Heavy_01_WEP, 12
        frames) and the gun's Fire_WEP (12 frames: the barrels turning).
      * Energon Harvester: each shot (FireShot) the gun's Fire and its own arm recoil
        (EMPShotgun_Recoil_S), 19 frames each; one clip holds a burst's two shots, 6
        frames (its 0.2 s FireInterval) apart, as the Photon Burst Rifle's does.
      * TechVolt: while the beam is on, looping, Weapon_Recoil_Rapid_Light_S at 0.8x
        (12 frames -> 15) under the gun's Fire_WEP (30 frames).
      * Slime Cannon: each shot, Weapon_Recoil_Light_S at 2x (30 -> 15) and the gun's
        Fire (30).
      * Gear Shredder: each shot, Weapon_Recoil_HeavyLight_S at 1.667x (30 -> 18) and
        the gun's Fire_WEP (15) for a single blade (its ChargedFire_WEP, 30, for a
        charged shot: the extra clip).
      * Thermo Rocket Cannon: each shot, Weapon_Recoil_Heavy_S at 1.5x (30 -> 20) and
        the gun's Fire_WEP (14).
      * Pulsar Cannon: Weapon_Recoil_HeavyLight_S at 1.5x (30 -> 20) with each shot and
        the gun's Fire_WEP (27) as it fires (StartFire).
      * Chaos-Rift Combustor: each shot, Weapon_Recoil_HeavyLight_S at 2x (30 -> 15)
        and the gun's Fire_WEP (20).

    Ordinary: the heavy weapons' stance (STANCE_HeavyGunArmShootFromHip, the
    Scrapmaker's and the Thermo Rocket Cannon's BaseStance and FineAimStance, as the
    robot's anim tree, TR_Shared_ANIMTREE_p.Robot_ANIMTREE, picks its clips: its
    TnStanceAnimationPickers): standing (Nav_Idle_HeavyWep_Shoot), jogging and walking
    in each of the ten strafe directions (Nav_StrafeJog_ / Nav_StrafeWalk_HeavyWep_
    Shoot_*, STRAFE_TEN's order; the tree's WalkJogBlend blends between the two by
    speed) and turning on the spot (Nav_Shoot_HeavyWep_IdlePivot90_L / R, then
    IdlePivot180_L / R), all from Shared_SCOUT_ROBO_ANIM, Jazz's set."""
    arm = psa_file(anim_dir, "WeaponArm_ROBO_ANIM")
    sets = {key: find("AnimSet", f"{name}.psa") for key, name, _, _ in NEW_WEAPONS}
    missing = [p for p in [arm] + list(sets.values()) if not os.path.exists(p)]
    if missing:
        print("  ! new weapons' animation sets not found:", missing, "- their guns don't animate")
    s = sets
    fire = {
        "heavymg": (12, True, [(arm, "ADD_Weapon_Recoil_RapidSmall_Heavy_01_WEP", 1.0, body),
                               (s["heavymg"], "ADD_Weapon_HeavyMG_Fire_WEP", 1.0, gun)]),
        "harvester": (25, False, [(s["harvester"], "ADD_Weapon_EMPShotgun_Recoil_S", 1.0, body, (0, 6)),
                                  (s["harvester"], "ADD_Weapon_EMPShotgun_Fire", 1.0, gun, (0, 6))]),
        "techvolt": (30, True, [(arm, "ADD_Weapon_Recoil_RapidSmall_Light_01_WEP", 0.8, body),
                                (s["techvolt"], "ADD_Weapon_RepairBeam_Fire_WEP", 1.0, gun)]),
        "slime": (30, False, [(arm, "ADD_Weapon_Recoil_LightSmall_01_WEP", 2.0, body),
                              (s["slime"], "ADD_Weapon_RustCannon_Fire", 1.0, gun)]),
        "shredder": (18, False, [(arm, "ADD_Weapon_Recoil_HeavyLight_Small_01_WEP", 1.667, body),
                                 (s["shredder"], "ADD_Weapon_Sawblade_Fire_WEP", 1.0, gun)]),
        "thermo": (20, False, [(arm, "ADD_Weapon_Recoil_HeavySmall_01_WEP", 1.5, body),
                               (s["thermo"], "ADD_Weapon_RocketLauncher_Fire_WEP", 1.0, gun)]),
        "pulsar": (27, False, [(arm, "ADD_Weapon_Recoil_HeavyLight_Small_01_WEP", 1.5, body),
                               (s["pulsar"], "ADD_Weapon_PulsarCannon_Fire_WEP", 1.0, gun)]),
        "combustor": (20, False, [(arm, "ADD_Weapon_Recoil_HeavyLight_Small_01_WEP", 2.0, body),
                                  (s["combustor"], "ADD_Weapon_GrenadeLauncher_Fire_WEP", 1.0, gun)]),
    }
    # Idle loops (TnWeaponEffectIdleAnimation) and reloads (Reload_Size's _S clip).
    idle = {"heavymg": ("Idle_WEP", 90), "harvester": ("Idle02", 75), "techvolt": ("Idle_WEP", 30),
            "slime": ("Idle", 120), "shredder": ("Idle_WEP", 60), "thermo": ("Idle_WEP", 90),
            "pulsar": ("Idle_WEP", 120), "combustor": ("Idle_WEP", 60)}
    reload = {"heavymg": ("Reload_S_WEP", 105), "harvester": ("Reload_S", 75), "techvolt": ("Reload_S_WEP", 75),
              "slime": ("Reload_S", 75), "shredder": ("Reload_S_WEP", 75), "thermo": ("Reload_S_WEP", 60),
              "pulsar": ("Reload_S_WEP", 75), "combustor": ("Reload_S_WEP", 75)}
    out = []
    for key, _, prefix, (equip, unequip, mirror) in NEW_WEAPONS:
        frames, loop, parts = fire[key]
        out.append((f"{key}_fire", frames, loop, parts))
        name, frames = idle[key]
        out.append((f"{key}_idle", frames, True, [(s[key], prefix + name, 1.0, gun)]))
        name, frames = reload[key]
        out.append((f"{key}_reload", frames, False, [(s[key], prefix + name, 1.0, gun)]))
        out.append((f"{key}_equip", 30, False, [(s[key], prefix + equip, 1.0, gun)]))
        out.append((f"{key}_unequip", 30, False, [(s[key], prefix + unequip, 1.0, cloud_b)]))
        out.append((f"{key}_change_hands", 30, False, [(s[key], prefix + mirror, 1.0, clouds)]))
    out += [
        ("heavymg_cooldown", 30, False, [(s["heavymg"], "ADD_Weapon_HeavyMG_CoolDown_WEP", 1.0, gun)]),
        ("heavymg_warmup", 45, False, [(s["heavymg"], "ADD_Weapon_HeavyMG_WarmUp_WEP", 1.0, gun)]),
        ("techvolt_cooldown", 20, False, [(s["techvolt"], "ADD_Weapon_RepairBeam_CoolDown_WEP", 1.0, gun)]),
        ("shredder_charged_fire", 30, False, [(arm, "ADD_Weapon_Recoil_HeavyLight_Small_01_WEP", 1.667, body),
                                              (s["shredder"], "ADD_Weapon_Sawblade_ChargedFire_WEP", 1.0, gun)]),
        ("shredder_charging", 75, True, [(s["shredder"], "ADD_Weapon_Sawblade_Charging_WEP", 1.0, gun)]),
    ]
    if missing:
        out = [(name, 2, False, []) for name, _, _, _ in out]
    anims = [("heavy_idle", scout_psa, "Nav_Idle_HeavyWep_Shoot")]
    anims += [(f"heavy_jog_{d.lower()}", scout_psa, f"Nav_StrafeJog_HeavyWep_Shoot_{d}") for d in STRAFE_TEN]
    anims += [(f"heavy_walk_{d.lower()}", scout_psa, f"Nav_StrafeWalk_HeavyWep_Shoot_{d}") for d in STRAFE_TEN]
    anims += [(f"heavy_pivot_{d.lower()}", scout_psa, f"Nav_Shoot_HeavyWep_IdlePivot90_{d}") for d in "LR"]
    anims += [(f"heavy_pivot180_{d.lower()}", scout_psa, f"Nav_Shoot_HeavyWep_IdlePivot180_{d}") for d in "LR"]
    return out, anims


def pickup_levelling(frame):
    """The turn that lays a gun level whose GUN_BONE has the turn `frame` (3 x 3): its
    barrel (the bone's -X) straight ahead along -Z and its top (the bone's +Y)
    straight up."""
    e = frame / np.linalg.norm(frame, axis=0)
    barrel, top = e @ np.array([-1.0, 0.0, 0.0]), e @ np.array([0.0, 1.0, 0.0])
    src = np.stack([barrel, top, np.cross(barrel, top)], 1)
    ahead, up = np.array([0.0, 0.0, -1.0]), np.array([0.0, 1.0, 0.0])
    dst = np.stack([ahead, up, np.cross(ahead, up)], 1)
    return dst @ src.T


def build_pickup(out_name, robot_glb, mesh_name, look=None, material=None):
    """A weapon pickup as the game shows it (on a weapon station, in a pod, dropped):
    its TnWeaponMesh's PickupMesh, the gun's mesh unanimated, so in its reference
    pose, every section of it (the gun, and the forearm cuff it is mounted on,
    ARM_MATERIAL), at the weapon's PickupScale (1: no weapon sets another); laid
    level (pickup_levelling: aimed straight ahead, -Z, its top straight up) and
    baked into a static mesh standing on the origin (its lowest point at y = 0,
    centred across), with its glowing outline shell (`look`: its glow and outline
    colours, PICKUP_LOOKS, or the energon cyan's). Its gun section's material carries
    the pickup's paint (`material`, its PICKUP_*_MATINST: the "pickup_paint" extras),
    its cuff its own. Returns where its Pickup socket (TnWeapon.PickupSocket, on
    GUN_BONE at the bone's own place: every gun's) lands in the model, which a weapon
    station holds the gun by (TnWeapon.AlignPickupMeshToSocket: shop.rs)."""
    d = Doc(robot_glb)
    g = d.g
    node = next(i for i, n in enumerate(g["nodes"]) if n.get("name") == mesh_name and "mesh" in n)
    skin = g["skins"][g["nodes"][node]["skin"]]
    ibm = d.acc(skin["inverseBindMatrices"]).reshape(-1, 4, 4).transpose(0, 2, 1)
    bone = next(i for i, n in enumerate(g["nodes"]) if n.get("name") == GUN_BONE)
    # The reference pose: the mesh's own positions, the bone where its inverse
    # bind matrix puts it.
    socket = np.linalg.inv(ibm[skin["joints"].index(bone)])
    level = pickup_levelling(socket[:3, :3])
    b = Builder()
    w = b.w
    prims, parts = [], []
    for p in g["meshes"][g["nodes"][node]["mesh"]]["primitives"]:
        mname = g["materials"][p["material"]]["name"]
        at = p["attributes"]
        pos = (d.acc(at["POSITION"]).astype(np.float64) - socket[:3, 3]) @ level.T

        def turn(v):
            v = v.astype(np.float64) @ level.T
            return v / np.maximum(np.linalg.norm(v, axis=1, keepdims=True), 1e-9)
        nrm = turn(d.acc(at["NORMAL"]))
        tan = d.acc(at["TANGENT"]).astype(np.float64) if "TANGENT" in at else None
        if tan is not None:
            tan = np.concatenate([turn(tan[:, :3]), tan[:, 3:]], 1)
        parts.append((mname, pos, nrm, tan, d.acc(at["TEXCOORD_0"]), d.acc(p["indices"]).reshape(-1)))
    allpos = np.concatenate([q[1] for q in parts])
    centre = (allpos.min(0) + allpos.max(0)) / 2
    centre[1] = allpos.min(0)[1]
    for mname, pos, nrm, tan, uv, idx in parts:
        attrs = {"POSITION": w.accessor((pos - centre).astype(np.float32), 34962, minmax=True),
                 "NORMAL": w.accessor(nrm.astype(np.float32), 34962),
                 "TEXCOORD_0": w.accessor(uv.astype(np.float32), 34962)}
        if tan is not None:
            attrs["TANGENT"] = w.accessor(tan.astype(np.float32), 34962)
        glow = look[0] if look else None
        brdf = mname if mname == ARM_MATERIAL else material
        paint = {"pickup_paint": {"gunpaint": mname, "brdf": brdf, "paint": PICKUP_PAINT,
                                  "metallic": PICKUP_METALLIC}} if brdf else None
        prims.append({"attributes": attrs, "indices": w.accessor(idx.astype(np.uint32), 34963),
                      "material": pickup_material(b, mname, glow, extras=paint)})
    outline = look[1] if look else None
    prims.append(outline_shell(b, [(pos - centre, nrm, idx) for _, pos, nrm, _, _, idx in parts], outline,
                               "PickupOutlineHeavy" if outline else OUTLINE_MATERIAL))
    w.g["meshes"].append({"name": mesh_name + "_Pickup", "primitives": prims})
    w.node({"name": mesh_name + "_Pickup", "mesh": 0}, root=True)
    size = w.save(os.path.join(OUT, "models", out_name))
    ext = allpos.max(0) - allpos.min(0)
    print(f"wrote models/{out_name} ({size/1e6:.1f} MB, {ext[0]:.2f} x {ext[1]:.2f} x {ext[2]:.2f} m)")
    return -centre


def outline_shell(b, parts, glow=None, material=OUTLINE_MATERIAL):
    """The outline: every triangle of the gun again, each corner pushed out by
    OUTLINE_WIDTH along the average normal of all corners at that point (so the
    shell has no cracks at hard edges), wound the other way round so its outside
    is culled. Unlit-looking: black with a strong glow (cyan, or `glow`'s colour
    and strength, in its own `material`)."""
    w = b.w
    pos = np.concatenate([p for p, _, _ in parts])
    nrm = np.concatenate([n for _, n, _ in parts])
    idx, base = [], 0
    for p, _, i in parts:
        idx.append(i.astype(np.int64) + base)
        base += len(p)
    idx = np.concatenate(idx).reshape(-1, 3)
    key = np.round(pos / 1e-4).astype(np.int64)
    _, group = np.unique(key, axis=0, return_inverse=True)
    group = group.reshape(-1)
    smooth = np.zeros((group.max() + 1, 3))
    np.add.at(smooth, group, nrm)
    smooth /= np.maximum(np.linalg.norm(smooth, axis=1, keepdims=True), 1e-9)
    n = smooth[group]
    shell = pos + n * OUTLINE_WIDTH
    colour, strength = glow or OUTLINE_GLOW
    if material not in b.mats:
        b.mats[material] = w.material({
            "name": material,
            "pbrMetallicRoughness": {"baseColorFactor": [0, 0, 0, 1], "metallicFactor": 0.0,
                                     "roughnessFactor": 1.0},
            "emissiveFactor": colour,
            "extensions": {"KHR_materials_emissive_strength": {"emissiveStrength": strength}}})
        used = w.g.setdefault("extensionsUsed", [])
        if "KHR_materials_emissive_strength" not in used:
            used.append("KHR_materials_emissive_strength")
    return {"attributes": {"POSITION": w.accessor(shell.astype(np.float32), 34962, minmax=True),
                           "NORMAL": w.accessor((-n).astype(np.float32), 34962)},
            "indices": w.accessor(idx[:, [0, 2, 1]].reshape(-1).astype(np.uint32), 34963),
            "material": b.mats[material]}


def energon_glow(base):
    """The glow of a texture set drawn by the game's BRDF materials (ENV_BRDF_A_MAT's
    material selector: its masks' red channel at nothing marks energon, lit in the
    material's EnergonColor at its Energon_Intensity; high, the glow areas): both."""
    m = np.asarray(Image.open(tex_path(base + "_Masks_NormY.png")).convert("RGBA"))
    glow = ((m[..., 0] < 40) | (m[..., 0] > 150)).astype(np.uint8) * 255
    return png(np.repeat(glow[..., None], 3, -1))


# Pickups whose glow areas are only their texture set's brightest (its masks' red
# above this, of 255): the heavy ammo crate shares the ammo crate's texture set,
# but in the game its drums' caps (red about 150 to 200 there) don't light up.
GLOW_FLOORS = {"HeavyAmmoPickup_MATINST": 235}


def pickup_material(b, name, glow=None, energon=False, extras=None):
    if name not in TEXSETS:
        return b.material(name)
    w = b.w
    t = texset_images(TEXSETS[name])
    colour, strength = glow or PICKUP_GLOW
    glow_png = energon_glow(TEXSETS[name]) if energon else t["glow"]
    if name in GLOW_FLOORS:
        m = np.asarray(Image.open(tex_path(TEXSETS[name] + "_Masks_NormY.png")).convert("RGBA"))
        floor = GLOW_FLOORS[name]
        g = np.clip((m[..., 0].astype(np.float32) - floor) / max(255 - floor, 1), 0, 1)
        glow_png = png((np.repeat(g[..., None], 3, -1) * 255).astype(np.uint8))
    m = {"name": name + "_Pickup",
         "pbrMetallicRoughness": {"baseColorTexture": {"index": w.texture(name + "_color", t["color"])},
                                  "metallicRoughnessTexture": {"index": w.texture(name + "_mr", t["mr"])},
                                  "metallicFactor": 1.0, "roughnessFactor": 1.0},
         "normalTexture": {"index": w.texture(name + "_normal", t["normal"])},
         "emissiveTexture": {"index": w.texture(name + ("_energon" if energon else "_glow"), glow_png)},
         "emissiveFactor": colour,
         "extensions": {"KHR_materials_emissive_strength": {"emissiveStrength": strength}}}
    if extras:
        m["extras"] = extras
    w.g["extensionsUsed"] = ["KHR_materials_emissive_strength"]
    return w.material(m)


def build_static(out_name, file, yaw_deg=0.0, scale=1.0, translation=(0, 0, 0)):
    b = Builder()
    w = b.w
    root = b.root("Root", yaw_deg, scale, translation)
    d = Doc(file)
    prims = b.primitives(d, d.g["nodes"][0]["mesh"])
    w.g["meshes"].append({"name": os.path.basename(file)[:-5], "primitives": prims})
    w.g["nodes"][root]["children"].append(w.node({"name": "mesh", "mesh": 0}))
    size = w.save(os.path.join(OUT, "models", out_name))
    print(f"wrote models/{out_name} ({size/1e6:.1f} MB)")


# The ammo crate's material (AmmoPickup_MATINST, on Pickup_Unlit_MAT as the weapon
# pickups' are): its glow areas lit in its Emissive_Color (0, 1, 0.94) at its
# EmissiveIntensity 5, and its outline (OutlineColor 0, 6, 6 at OutlineIntensity 10,
# against the weapons' 0, 1, 1 at 2: far brighter) as the weapons' shell, twice as
# bright as theirs.
AMMO_CRATE_GLOW = ([0.0, 1.0, 0.94], 5.0)
AMMO_CRATE_OUTLINE = ([0.0, 1.0, 1.0], 12.0)


def build_ammo_crate(out_name, file):
    """The ammo stations' crate (AmmoPickupLight_STAT), as it stands in the game (its
    origin at its foot, upright), in its texture set with its glow areas lit cyan and
    the pickups' glowing outline shell."""
    b = Builder()
    w = b.w
    d = Doc(file)
    prims, parts = [], []
    for p in d.g["meshes"][d.g["nodes"][0]["mesh"]]["primitives"]:
        mname = d.g["materials"][p["material"]]["name"]
        at = p["attributes"]
        pos = d.acc(at["POSITION"]).astype(np.float32)
        nrm = d.acc(at["NORMAL"]).astype(np.float32)
        idx = d.acc(p["indices"]).reshape(-1)
        attrs = {"POSITION": w.accessor(pos, 34962, minmax=True),
                 "NORMAL": w.accessor(nrm, 34962),
                 "TEXCOORD_0": w.accessor(d.acc(at["TEXCOORD_0"]).astype(np.float32), 34962)}
        if "TANGENT" in at:
            attrs["TANGENT"] = w.accessor(d.acc(at["TANGENT"]).astype(np.float32), 34962)
        prims.append({"attributes": attrs, "indices": w.accessor(idx.astype(np.uint32), 34963),
                      "material": pickup_material(b, mname, AMMO_CRATE_GLOW)})
        parts.append((pos.astype(np.float64), nrm.astype(np.float64), idx))
    prims.append(outline_shell(b, parts, AMMO_CRATE_OUTLINE))
    w.g["meshes"].append({"name": "AmmoPickupLight_STAT", "primitives": prims})
    w.node({"name": "AmmoPickupLight_STAT", "mesh": 0}, root=True)
    size = w.save(os.path.join(OUT, "models", out_name))
    allpos = np.concatenate([q[0] for q in parts])
    ext = allpos.max(0) - allpos.min(0)
    print(f"wrote models/{out_name} ({size/1e6:.1f} MB, {ext[0]:.2f} x {ext[1]:.2f} x {ext[2]:.2f} m)")


def build_weapon_pickups():
    """Every weapon's pickup (WEAPON_PICKUPS, in its PICKUP_LOOKS), and
    models/weapons/sockets.txt: where each model's Pickup socket is ("<model> x y z",
    metres in the model), for the weapon stations."""
    lines = []
    for name, mesh in WEAPON_PICKUPS.items():
        at = build_pickup(f"weapons/{name}.glb", os.path.join(OUT, "models", "robot.glb"), mesh,
                          PICKUP_LOOKS.get(name), PICKUP_MATERIALS[name])
        lines.append("%s %.4f %.4f %.4f\n" % (name, *at))
    with open(os.path.join(OUT, "models", "weapons", "sockets.txt"), "w") as fh:
        fh.write("# Each weapon pickup's Pickup socket (TnWeapon.PickupSocket) in its model, metres.\n")
        fh.writelines(lines)
    print(f"wrote models/weapons/sockets.txt ({len(lines)} weapons)")


# The things Escalation's other purchase points sell, as they stand over their
# bases (props/<name>.glb, main.rs): the heavy ammo crate (TnAmmoCratePickupHeavy's
# AmmoPickupHeavy_STAT) and the equipment (each TnEquipmentPickup's
# PickupFactoryMesh), each in its material's texture set with its glow areas in the
# material's Emissive_Color or EnergonColor and its outline in its OutlineColor:
# model file, export, glow (colour, strength), outline (colour, strength), the
# sections drawn as a see-through tint (colour, alpha) instead, and whether its
# material is the BRDF one (its energon lit: energon_glow) rather than
# Pickup_Unlit_MAT (its glow areas).
STATION_PROPS = (
    # (The heavy crate in the heavy weapons' yellow: only its small markings light up,
    # its drums' caps stay dark metal, as the user's captures show; GLOW_FLOORS.)
    ("ammo_crate_heavy", "AmmoPickupHeavy_STAT", HEAVY_GLOW, HEAVY_OUTLINE, {}, False),
    # (The Diffraction Barrier's shield is drawn by its own material, as an effect:
    # build_weapon_fx.py's Barrier_Pickup_Shield_FX, src/shop.rs.)
    ("pickup_barrier", "MP_Pickup_Barrier_STAT", ([0.0, 0.45, 1.0], 5.0), ([0.0, 0.25, 1.0], 6.0),
     {"MP_Barrier_Distortion_MATINST": None}, True),
    ("pickup_mine_payload", "MP_Pickup_KamikazeMine_STAT", ([0.0, 1.0, 0.94], 3.0), ([1.0, 0.069, 0.011], 6.0), {},
     False),
    ("pickup_energon_recharger", "MP_Pickup_EnergonRecharger_STAT", ([0.0, 0.395, 1.0], 6.25),
     ([0.0, 0.25, 1.0], 6.0), {}, True),
    ("pickup_intercooler", "MP_Pickup_Intercooler_STAT", ([0.0, 0.25, 1.0], 8.0), ([0.0, 0.25, 1.0], 6.0), {}, False),
    # The Dimensional Decimator's (MP_Pickup_BlackHoleGrenade_STAT in
    # WEP_BlackHoleGrenade_MATINST: Emissive_Color and OutlineColor red, 0.8745, 0.0549,
    # 0.0471).
    ("pickup_decimator", "MP_Pickup_BlackHoleGrenade_STAT", ([0.8745, 0.0549, 0.0471], 3.0),
     ([0.8745, 0.0549, 0.0471], 6.0), {}, False),
)


def build_station_prop(out_name, file, glow, outline, tints, energon):
    """A purchase point's goods (STATION_PROPS) as the game stands them: origin and
    turn as the mesh has them, in its texture set with its glow areas lit, the
    pickups' glowing outline shell, and its see-through sections tinted (or left
    out, for those `tints` maps to None: drawn as effects)."""
    b = Builder()
    w = b.w
    d = Doc(file)
    prims, parts = [], []
    for p in d.g["meshes"][d.g["nodes"][0]["mesh"]]["primitives"]:
        mname = d.g["materials"][p["material"]]["name"]
        at = p["attributes"]
        pos = d.acc(at["POSITION"]).astype(np.float32)
        nrm = d.acc(at["NORMAL"]).astype(np.float32)
        idx = d.acc(p["indices"]).reshape(-1)
        attrs = {"POSITION": w.accessor(pos, 34962, minmax=True), "NORMAL": w.accessor(nrm, 34962)}
        if mname in tints:
            if tints[mname] is None:
                continue
            colour, alpha = tints[mname]
            if mname not in b.mats:
                b.mats[mname] = w.material({
                    "name": mname, "alphaMode": "BLEND", "doubleSided": True,
                    "pbrMetallicRoughness": {"baseColorFactor": colour + [alpha], "metallicFactor": 0.0,
                                             "roughnessFactor": 0.3},
                    "emissiveFactor": colour})
            prims.append({"attributes": attrs, "indices": w.accessor(idx.astype(np.uint32), 34963),
                          "material": b.mats[mname]})
            continue
        attrs["TEXCOORD_0"] = w.accessor(d.acc(at["TEXCOORD_0"]).astype(np.float32), 34962)
        if "TANGENT" in at:
            attrs["TANGENT"] = w.accessor(d.acc(at["TANGENT"]).astype(np.float32), 34962)
        prims.append({"attributes": attrs, "indices": w.accessor(idx.astype(np.uint32), 34963),
                      "material": pickup_material(b, mname, glow, energon)})
        parts.append((pos.astype(np.float64), nrm.astype(np.float64), idx))
    prims.append(outline_shell(b, parts, outline))
    w.g["meshes"].append({"name": os.path.basename(file)[:-5], "primitives": prims})
    w.node({"name": os.path.basename(file)[:-5], "mesh": 0}, root=True)
    size = w.save(os.path.join(OUT, "models", out_name))
    allpos = np.concatenate([q[0] for q in parts])
    lo, hi = allpos.min(0), allpos.max(0)
    print(f"wrote models/{out_name} ({size/1e6:.1f} MB, y {lo[1]:.2f} to {hi[1]:.2f} m, "
          f"{hi[0] - lo[0]:.2f} x {hi[2] - lo[2]:.2f} m across)")


def build_station_props():
    for name, mesh, glow, outline, tints, energon in STATION_PROPS:
        f = find("StaticMesh3", mesh + ".gltf")
        if os.path.exists(f):
            build_station_prop(f"props/{name}.glb", f, glow, outline, tints, energon)
        else:
            print(f"  ! no {mesh} export: props/{name}.glb left out")


def build_equipment_models():
    """The support items' own models as they come out in use (src/equipment.rs): the
    Thermo Mine Payload's mines (TnProjectileKamikazeMineKillstreak's AP_KMine_SKEL,
    all_shared.xxx, at its DrawScale 1, with WEP_KamikazeMine_ANIM_02's clips: 0 its
    idle, Nav_KMine_Idle, 1 its homing, Nav_KMine_Activate; umodel's glTF export of
    the mesh and .psa export of the animation set) and the Dimensional Decimator's ball
    (TnRollerMineGravityAura's WEP_GravityBomb_STAT, an Escalation map package's)."""
    mine = find("SkeletalMesh3", "AP_KMine_SKEL.gltf")
    clips = find("AnimSet", "WEP_KamikazeMine_ANIM_02.psa")
    if os.path.exists(mine) and os.path.exists(clips):
        build_skinned("props/thermo_mine.glb", [mine], 0.0, 1.0,
                      anims=[("idle", clips, "Nav_KMine_Idle"), ("activate", clips, "Nav_KMine_Activate")])
    else:
        print("  ! no AP_KMine_SKEL or WEP_KamikazeMine_ANIM_02 export: props/thermo_mine.glb left out")
    ball = find("StaticMesh3", "WEP_GravityBomb_STAT.gltf")
    if os.path.exists(ball):
        build_static("props/decimator_ball.glb", ball)
    else:
        print("  ! no WEP_GravityBomb_STAT export: props/decimator_ball.glb left out")


def build_energon(out_name, file):
    """The energon cube: its mesh (the sides, then the edges) and, as its texture 0,
    the colour clouds its sides' material (PROP_NEU_EnergonCube_Clouds_MAT) samples
    (fractalMaps, repeating), which src/energon.rs uses. Without the texture's export
    a black pixel stands in, leaving the sides their plain cyan."""
    b = Builder()
    w = b.w
    clouds = tex_path("fractalMaps.png")
    if os.path.exists(clouds):
        image = np.asarray(Image.open(clouds).convert("RGB"))
    else:
        print("  ! fractalMaps.png not found (export it from PROP_NEU_Pickups_p): the energon cube's sides are plain cyan")
        image = np.zeros((1, 1, 3), np.uint8)
    tex = w.texture("fractalMaps", png(image))
    root = b.root("Root")
    d = Doc(file)
    prims = b.primitives(d, d.g["nodes"][0]["mesh"])
    # (Hung on the sides' stand-in material as its glow, so it loads with the model,
    # in colour (sRGB), as the game samples it.)
    for m in w.g["materials"]:
        if m["name"] == "PROP_NEU_EnergonCube_Clouds_MAT":
            m["emissiveTexture"] = {"index": tex}
    w.g["meshes"].append({"name": os.path.basename(file)[:-5], "primitives": prims})
    w.g["nodes"][root]["children"].append(w.node({"name": "mesh", "mesh": 0}))
    size = w.save(os.path.join(OUT, "models", out_name))
    print(f"wrote models/{out_name} ({size/1e6:.1f} MB)")


def build_floor(out_name, half, tile):
    b = Builder()
    w = b.w
    root = b.root("Root")
    s = half
    pos = np.array([[-s, 0, -s], [s, 0, -s], [s, 0, s], [-s, 0, s]], np.float32)
    nrm = np.tile(np.array([[0, 1, 0]], np.float32), (4, 1))
    tan = np.tile(np.array([[1, 0, 0, 1]], np.float32), (4, 1))
    r = 2 * s / tile
    uv = np.array([[0, 0], [r, 0], [r, r], [0, r]], np.float32)
    idx = np.array([0, 2, 1, 0, 3, 2], np.uint32)
    prim = {"attributes": {"POSITION": w.accessor(pos, 34962, True), "NORMAL": w.accessor(nrm, 34962),
                           "TANGENT": w.accessor(tan, 34962), "TEXCOORD_0": w.accessor(uv, 34962)},
            "indices": w.accessor(idx, 34963), "material": b.material("ENV_RST_SewerLedgeFloor01_MATINST")}
    w.g["meshes"].append({"name": "floor", "primitives": [prim]})
    w.g["nodes"][root]["children"].append(w.node({"name": "floor", "mesh": 0}))
    size = w.save(os.path.join(OUT, "models", out_name))
    print(f"wrote models/{out_name} ({size/1e6:.1f} MB)")


def build_sky(out_name):
    """Sky dome mesh plus its texture. The game code builds an unlit, fog-free material."""
    w = Writer()
    d = Doc(os.path.join(ST, "ENV_SkyDome_STAT.gltf"))
    p = d.g["meshes"][0]["primitives"][0]
    at = p["attributes"]
    tex = w.texture("sky", png(np.asarray(Image.open(tex_path("IaconSunset_CLR.png")).convert("RGB"))), wrap=33071)
    mat = w.material({"name": "sky", "pbrMetallicRoughness": {"baseColorTexture": {"index": tex}}, "extensions": {"KHR_materials_unlit": {}}})
    w.g["extensionsUsed"] = ["KHR_materials_unlit"]
    prim = {"attributes": {"POSITION": w.accessor(d.acc(at["POSITION"]).astype(np.float32), 34962, True),
                           "NORMAL": w.accessor(d.acc(at["NORMAL"]).astype(np.float32), 34962),
                           "TEXCOORD_0": w.accessor(d.acc(at["TEXCOORD_0"]).astype(np.float32), 34962)},
            "indices": w.accessor(d.acc(p["indices"]).reshape(-1).astype(np.uint32), 34963), "material": mat}
    w.g["meshes"].append({"name": "sky", "primitives": [prim]})
    w.node({"name": "sky", "mesh": 0}, root=True)
    size = w.save(os.path.join(OUT, "models", out_name))
    print(f"wrote models/{out_name} ({size/1e6:.1f} MB)")


if __name__ == "__main__":
    os.makedirs(os.path.join(OUT, "models", "props"), exist_ok=True)
    os.makedirs(os.path.join(OUT, "models", "weapons"), exist_ok=True)
    YAW = float(os.environ.get("FOC_YAW", "90"))  # turns the exported +X forward into Bevy's -Z
    # Jazz's right forearm is replaced by his weapon, the way weapons appear in the game.
    # Every weapon goes into robot.glb as a separate mesh (named after its file); the
    # game shows the one in use. The Neutron Assault Rifle (RB_AssaultRifleR_SKEL) is
    # exported from L03b_CutAndRun_BASE_m like the rest, as are the Nucleon Charge
    # Rifle (RB_SniperRifleR_SKEL) and the Path Blaster (RB_HeavyPistolR_SKEL); the
    # Photon Burst Rifle (RB_BurstRifleR_SKEL), the Scatter Blaster
    # (RB_ScatterblasterR_SKEL), the Riot Cannon (RB_BazookaR_SKEL) and the Orbital
    # Pistol (RB_SysCorruptorR_SKEL) from all_shared.
    # His right forearm (ArmLoR) is there for when the gun is on his left arm.
    jazz = [os.path.join(SK, f"RB_Jazz_CHARSKEL_{p}.gltf") for p in "Legs Chest Head ArmUpL ArmUpR ArmLoL CapL CapR ArmLoR".split()]
    weapons = [find("SkeletalMesh3", f) for f in WEAPON_MESHES]
    missing = [f for f in weapons if not os.path.exists(f)]
    if missing:
        raise SystemExit(f"weapon meshes not found: {missing}")
    weapons_left = [find("SkeletalMesh3", f) for f in WEAPON_MESHES_LEFT]
    if not all(os.path.exists(f) for f in weapons_left):
        print("  ! left-handed weapon meshes (RB_*L_SKEL) not found - Jazz can't switch hands")
        weapons_left = []
    # The exported rest pose has the arms out in an A-pose. Bake a ready-to-fire pose:
    # right arm down with the blaster pointing forward, left arm lowered and relaxed.
    aim = [
        ("R_Arm02_Shoulder_XB", "R_Arm03_Elbow_XB", (0.28, -0.96, 0.05)),
        ("R_Arm03_Elbow_XB", "R_Arm04_Hand_XB", (0.04, -0.12, -1.0)),
        ("L_Arm02_Shoulder_XB", "L_Arm03_Elbow_XB", (-0.30, -0.95, 0.05)),
        ("L_Arm03_Elbow_XB", "L_Arm04_Hand_XB", (-0.15, -0.75, -0.65)),
    ]
    # Animations exported from umodel (Jazz mesh + animation set, glTF, retargeting
    # disabled). Their order here is the order the game expects (see ANIM_* in main.rs).
    anim_dir = os.environ.get("FOC_ANIMS", os.path.join(os.path.dirname(os.path.abspath(SRC)), "Animations"))
    scout = gltf_or_psa(anim_dir, "Shared_SCOUT_ROBO_ANIM")
    nav = gltf_or_psa(anim_dir, "Shared_ROBO_NAV_ANIM")
    # The same set saved by umodel as an ActorX .psa (AnimSet export). umodel's glTF
    # export of this set left out 22 clips, among them the forward/back air dashes.
    nav_psa = psa_file(anim_dir, "Shared_ROBO_NAV_ANIM")
    jazz_anim = gltf_or_psa(anim_dir, "Jazz_ROBO_ANIM_NEW")
    robo = gltf_or_psa(anim_dir, "Shared_ROBO_ANIM")
    death = gltf_or_psa(anim_dir, "Death_Shared_ANIM")
    anims = [
        ("idle", nav, "Aim_Rest_F_C"),
        ("jog_fl", scout, "Nav_StrafeJog_Rest_FL"),
        ("jog_fr", scout, "Nav_StrafeJog_Rest_FR"),
        ("jog_l", scout, "Nav_StrafeJog_Rest_L"),
        ("jog_r", scout, "Nav_StrafeJog_Rest_R"),
        ("jog_bl", scout, "Nav_StrafeJog_Rest_BL"),
        ("jog_br", scout, "Nav_StrafeJog_Rest_BR"),
        ("sprint", nav, "Nav_Sprint_F"),
        ("to_vehicle", jazz_anim, "Transform_ToVehicle_ROBO"),
        ("to_robot", jazz_anim, "Transform_ToRobot_ROBO"),
        ("land", nav, "Nav_Land"),
        ("boost_f", nav, "Nav_Boost_F"),
        ("boost_b", nav, "Nav_Boost_B"),
        ("boost_l", nav, "Nav_Boost_L"),
        ("boost_r", nav, "Nav_Boost_R"),
        ("melee", robo, "Melee_ShieldBash"),
        ("melee_boost", robo, "Melee_Boost"),
        ("air_boost_l", nav, "Nav_Air_Boost_L"),
        ("air_boost_r", nav, "Nav_Air_Boost_R"),
        ("land_hard", nav, "Nav_Land_Hard_01"),
        ("throw", robo, "Grenade_Throw_MP"),
        ("hit", robo, "HitReaction_Descent"),
        ("death", death, "Death_FrontShot_05"),
        ("air_boost_f", nav_psa, "Nav_Air_Boost_F"),
        ("air_boost_b", nav_psa, "Nav_Air_Boost_B"),
    ]
    if not os.path.exists(nav_psa):
        # Without the .psa, keep the clip count the game expects using the side dashes.
        print("  ! Shared_ROBO_NAV_ANIM.psa not found - forward/back air dashes use the side ones")
        anims[-2] = ("air_boost_f", nav, "Nav_Air_Boost_R")
        anims[-1] = ("air_boost_b", nav, "Nav_Air_Boost_L")
    # Weapon stances. Shooting: the gun raised from the hip (STANCE_GunArmShootFromHip),
    # jogging. Fine aim (zoom): aiming down the sights (STANCE_GunArmFineAim), walking.
    # Scout clips from Shared_SCOUT_ROBO_ANIM.psa (umodel's glTF export of that set
    # lacks the forward-diagonal fine-aim walks).
    scout_psa = psa_file(anim_dir, "Shared_SCOUT_ROBO_ANIM")
    stance_src = nav_psa if os.path.exists(nav_psa) else nav
    if os.path.exists(scout_psa):
        anims.append(("shoot_idle", stance_src, "Aim_Shoot_F_C"))
        anims += [(f"shoot_move_{d.lower()}", scout_psa, f"Nav_StrafeJog_Shoot_{d}") for d in ("FL", "FR", "L", "R", "BL", "BR")]
        anims.append(("aim_idle", stance_src, "Aim_FineAim_F_C"))
        anims += [(f"aim_move_{d.lower()}", scout_psa, f"Nav_StrafeWalk_FineAim_{d}") for d in ("FL", "FR", "L", "R", "BL", "BR")]
    else:
        print("  ! Shared_SCOUT_ROBO_ANIM.psa not found - shooting and fine aim use the relaxed stance")
        anims += [("shoot_idle", nav, "Aim_Rest_F_C")] + [(f"shoot_move_{d}", scout, f"Nav_StrafeJog_Rest_{d.upper()}") for d in ("fl", "fr", "l", "r", "bl", "br")]
        anims += [("aim_idle", nav, "Aim_Rest_F_C")] + [(f"aim_move_{d}", scout, f"Nav_StrafeJog_Rest_{d.upper()}") for d in ("fl", "fr", "l", "r", "bl", "br")]
    # Jumping and falling, as Jazz's acrobatics (TR_Acrobatics_p.Bumblebee_JazzAcrobatics)
    # play them: the take-off into the descent loop for a jump, the second descent for
    # stepping off a ledge.
    anims += [("jump", stance_src, "Nav_TakeOff_01"),
              ("jump_descent", stance_src, "Nav_Jump_Descent"),
              ("fall", stance_src, "Nav_Jump_Descent_02")]
    # Additive weapon animations, from the weapons' TnWeaponEffectPlayAnimation data.
    # While firing, both guns loop the arm recoil Weapon_Recoil_Rapid_Light_S
    # (= ADD_Weapon_Recoil_RapidSmall_Light_01_WEP; the Ion Blaster plays it at 1.5x)
    # plus their own Fire_WEP (the rifle's barrels spin). Letting go plays the rifle's
    # CoolDown_WEP; Idle_WEP loops whenever the gun is out. Gun-part animations only
    # move the gun's bones (the game's CAMT_Weapon mask: R_CloudA_*); the recoil moves
    # Jazz's upper body, except the hand, which is folded away inside the gun.
    arm_psa = psa_file(anim_dir, "WeaponArm_ROBO_ANIM")
    rifle_psa = psa_file(anim_dir, "Weapon_AssaultRifle_ANIM")
    ion_psa = psa_file(anim_dir, "Weapon_IonBlaster_ANIM")
    burst_psa = psa_file(anim_dir, "Weapon_PulseRifle_ANIM")
    if not os.path.exists(burst_psa):
        burst_psa = find("AnimSet", "Weapon_PulseRifle_ANIM.psa")
    scatter_psa = psa_file(anim_dir, "Weapon_Shotgun_ANIM")
    if not os.path.exists(scatter_psa):
        scatter_psa = find("AnimSet", "Weapon_Shotgun_ANIM.psa")
    # The Nucleon Charge Rifle's (WEP_SniperRifle_p.Weapon_SniperRifle_ANIM, in the
    # level package with the gun).
    sniper_psa = psa_file(anim_dir, "Weapon_SniperRifle_ANIM")
    if not os.path.exists(sniper_psa):
        sniper_psa = find("AnimSet", "Weapon_SniperRifle_ANIM.psa")
    # The Path Blaster's (WEP_HeavyPistol_p.Weapon_HeavyPistol_ANIM, in the level
    # package with the gun).
    path_psa = psa_file(anim_dir, "Weapon_HeavyPistol_ANIM")
    if not os.path.exists(path_psa):
        path_psa = find("AnimSet", "Weapon_HeavyPistol_ANIM.psa")
    # The Riot Cannon's (WEP_Bazooka_p.Weapon_Bazooka_ANIM, in the all_shared export
    # with the gun: its clips, unlike the others', have no _WEP at the end).
    riot_psa = psa_file(anim_dir, "Weapon_Bazooka_ANIM")
    if not os.path.exists(riot_psa):
        riot_psa = find("AnimSet", "Weapon_Bazooka_ANIM.psa")
    # The Orbital Pistol's (WEP_TracerDart_p.Weapon_TracerDart_ANIM, in the all_shared
    # export with the gun: the campaign's SysCorruptor clips).
    orbital_psa = psa_file(anim_dir, "Weapon_TracerDart_ANIM")
    if not os.path.exists(orbital_psa):
        orbital_psa = find("AnimSet", "Weapon_TracerDart_ANIM.psa")
    gun = lambda b: b.startswith("R_CloudA_")
    body = lambda b: b not in ("R_Arm04_Hand_XB", "R_Weapon01_Wpn_XW")
    recoil = "ADD_Weapon_Recoil_RapidSmall_Light_01_WEP"
    additive = [
        # 12-frame recoil and 16-frame barrel spin repeat together every 48 frames.
        ("rifle_fire", 48, True, [(arm_psa, recoil, 1.0, body),
                                  (rifle_psa, "ADD_Weapon_AssaultRifle_Fire_WEP", 1.0, gun)]),
        ("rifle_cooldown", 31, False, [(rifle_psa, "ADD_Weapon_AssaultRifle_CoolDown_WEP", 1.0, gun)]),
        ("rifle_idle", 75, True, [(rifle_psa, "ADD_Weapon_AssaultRifle_Idle_WEP", 1.0, gun)]),
        # 8 frames of recoil at 1.5x and the 15-frame fire loop meet every 120 frames.
        ("ion_fire", 120, True, [(arm_psa, recoil, 1.5, body),
                                 (ion_psa, "ADD_Weapon_SubsonicRepeater_Fire_WEP", 1.0, gun)]),
        ("ion_idle", 50, True, [(ion_psa, "ADD_Weapon_SubsonicRepeater_Idle_WEP", 1.0, gun)]),
        # Reloads (TnWeaponEventReload, CAMT_Weapon mask; Jazz is a small character, so
        # the _S versions): the gun's parts cycle a fresh clip. main.rs stretches them
        # to the weapons' reload times (1.5 s rifle, 1.25 s Ion Blaster).
        ("rifle_reload", 61, False, [(rifle_psa, "ADD_Weapon_AssaultRifle_Reload_S_WEP", 1.0, gun)]),
        ("ion_reload", 38, False, [(ion_psa, "ADD_Weapon_SubsonicRepeater_Reload_S_WEP", 1.0, gun)]),
        # The Photon Burst Rifle (Weapon_PulseRifle_ANIM). Each shot of a burst (three,
        # 0.1 s = 3 frames apart) kicks the arm with Weapon_Recoil_Light_S
        # (= ADD_Weapon_Recoil_LightSmall_01_WEP at 1.5x) from the start, so one clip
        # holds a whole burst. The gun's Fire_WEP turns its drum one full turn over the
        # burst (restarting it per shot would snap the drum back mid-turn). Idle_WEP
        # loops while it is out; the reload is Reload_S_WEP (stretched to the 2 s reload).
        ("burst_fire", 26, False, [(arm_psa, "ADD_Weapon_Recoil_LightSmall_01_WEP", 1.5, body, BURST_SHOTS),
                                   (burst_psa, "ADD_Weapon_PulseRifle_Fire_WEP", 1.0, gun)]),
        ("burst_idle", 120, True, [(burst_psa, "ADD_Weapon_PulseRifle_Idle_WEP", 1.0, gun)]),
        ("burst_reload", 75, False, [(burst_psa, "ADD_Weapon_PulseRifle_Reload_S_WEP", 1.0, gun)]),
        # The Scatter Blaster (Weapon_Shotgun_ANIM). Each shot (FireShot) kicks the arm
        # with Weapon_Recoil_Light_S at 1.667x (30 frames -> 18) while the gun's
        # Fire_WEP plays at 0.9x (12 frames -> 13.3); main.rs restarts the clip per shot.
        # Idle_WEP loops while it is out; the reload is Reload_S_WEP (2.5 s, as long as
        # the gun's own reload).
        ("scatter_fire", 18, False, [(arm_psa, "ADD_Weapon_Recoil_LightSmall_01_WEP", 1.667, body),
                                     (scatter_psa, "ADD_Weapon_Scatterblaster_Fire_WEP", 0.9, gun)]),
        ("scatter_idle", 90, True, [(scatter_psa, "ADD_Weapon_Scatterblaster_Idle_WEP", 1.0, gun)]),
        ("scatter_reload", 75, False, [(scatter_psa, "ADD_Weapon_Scatterblaster_Reload_S_WEP", 1.0, gun)]),
        # The Nucleon Charge Rifle (Weapon_SniperRifle_ANIM). Each shot (FireShot)
        # kicks the arm with Weapon_Recoil_HeavyLight_S (=
        # ADD_Weapon_Recoil_HeavyLight_Small_01_WEP) at 2x (30 frames -> 15) while the
        # gun's Fire_WEP plays (37 frames); main.rs restarts the clip per shot. Idle_WEP
        # loops while it is out; the reload is Reload_S_WEP (2 s, the gun's reload).
        ("sniper_fire", 37, False, [(arm_psa, "ADD_Weapon_Recoil_HeavyLight_Small_01_WEP", 2.0, body),
                                    (sniper_psa, "ADD_Weapon_SniperRifle_Fire_WEP", 1.0, gun)]),
        ("sniper_idle", 90, True, [(sniper_psa, "ADD_Weapon_SniperRifle_Idle_WEP", 1.0, gun)]),
        ("sniper_reload", 60, False, [(sniper_psa, "ADD_Weapon_SniperRifle_Reload_S_WEP", 1.0, gun)]),
        # The Path Blaster (Weapon_HeavyPistol_ANIM). Each shot (FireShot) kicks the
        # arm with Weapon_Recoil_Light_S (= ADD_Weapon_Recoil_LightSmall_01_WEP) at
        # 2.25x (30 frames -> 13.3) while the gun's Fire_WEP plays (11 frames);
        # main.rs restarts the clip per shot. Idle_WEP loops while it is out; the
        # reload is Reload_S_WEP (2.5 s, the gun's reload).
        ("path_fire", 14, False, [(arm_psa, "ADD_Weapon_Recoil_LightSmall_01_WEP", 2.25, body),
                                  (path_psa, "ADD_Weapon_PathBlaster_Fire_WEP", 1.0, gun)]),
        ("path_idle", 90, True, [(path_psa, "ADD_Weapon_PathBlaster_Idle_WEP", 1.0, gun)]),
        ("path_reload", 75, False, [(path_psa, "ADD_Weapon_PathBlaster_Reload_S_WEP", 1.0, gun)]),
        # The Riot Cannon (Weapon_Bazooka_ANIM). Each shot (FireShot) kicks the arm
        # with Weapon_Recoil_HeavyLight_S (= ADD_Weapon_Recoil_HeavyLight_Small_01_WEP)
        # at 1.8x (30 frames -> 16.7) while the gun's RiotCannon_Fire plays (30
        # frames: its drums turn); main.rs restarts the clip per shot. Its Idle loops
        # while it is out; the reload is Reload_S (2.5 s, the gun's reload).
        ("riot_fire", 30, False, [(arm_psa, "ADD_Weapon_Recoil_HeavyLight_Small_01_WEP", 1.8, body),
                                  (riot_psa, "ADD_Weapon_RiotCannon_Fire", 1.0, gun)]),
        ("riot_idle", 120, True, [(riot_psa, "ADD_Weapon_RiotCannon_Idle", 1.0, gun)]),
        ("riot_reload", 75, False, [(riot_psa, "ADD_Weapon_RiotCannon_Reload_S", 1.0, gun)]),
        # The Orbital Pistol (Weapon_TracerDart_ANIM). Each shot (FireShot) kicks the
        # arm with Weapon_Recoil_Light_S (= ADD_Weapon_Recoil_LightSmall_01_WEP) at 2x
        # (its PlayRate: 30 frames -> 15) while the gun's SysCorruptor_Fire_WEP plays
        # (15 frames); main.rs restarts the clip per shot. Its Idle_WEP loops while it
        # is out; the reload is Reload_S_WEP (30 frames, stretched to the gun's 1.25 s).
        ("orbital_fire", 15, False, [(arm_psa, "ADD_Weapon_Recoil_LightSmall_01_WEP", 2.0, body),
                                     (orbital_psa, "ADD_Weapon_SysCorruptor_Fire_WEP", 1.0, gun)]),
        ("orbital_idle", 60, True, [(orbital_psa, "ADD_Weapon_SysCorruptor_Idle_WEP", 1.0, gun)]),
        ("orbital_reload", 30, False, [(orbital_psa, "ADD_Weapon_SysCorruptor_Reload_S_WEP", 1.0, gun)]),
    ]
    if not all(os.path.exists(p) for p in (arm_psa, rifle_psa, ion_psa, burst_psa, scatter_psa, sniper_psa,
                                           path_psa, riot_psa, orbital_psa)):
        print("  ! weapon animation .psa files not found - guns don't animate when firing")
        additive = [(name, 2, False, []) for name, _, _, _ in additive]
    if not all(os.path.exists(p) for p in (scout, nav, jazz_anim, robo, death)):
        print("  ! animation exports not found in", anim_dir, "- robot will be static")
        anims = None
    # The rest of Jazz's strafing (Shared_SCOUT_ROBO_ANIM, his scout set): each stance
    # has ten directions in the game. Besides the six diagonals and sides above, the
    # straight forward and back clips (the demo used to mix the diagonals for those),
    # and LB / RB: strafing left or right with the hips turned back, the way the game
    # switches to when the movement swings round from the front to the back half
    # (TnStraferAnimBlend). Written after the additive clips, in this order:
    # jog_f, jog_b, jog_lb, jog_rb, then shoot_*, then aim_* (ANIM_STRAFE_EXTRA).
    strafe_sets = (("jog", "Nav_StrafeJog_Rest_", ("F", "b", "LB", "RB")),
                   ("shoot", "Nav_StrafeJog_Shoot_", ("F", "B", "LB", "RB")),
                   ("aim", "Nav_StrafeWalk_FineAim_", ("F", "B", "LB", "RB")))
    if os.path.exists(scout_psa):
        more = [(f"{stance}_{d.lower()}", scout_psa, prefix + d)
                for stance, prefix, dirs in strafe_sets for d in dirs]
    else:
        # Without the .psa, the nearest of the six directions stand in.
        near = {"f": "FL", "b": "BL", "lb": "L", "rb": "R"}
        more = [(f"{stance}_{d}", scout, f"Nav_StrafeJog_Rest_{near[d]}")
                for stance, _, _ in strafe_sets for d in ("f", "b", "lb", "rb")]
    # Turning on the spot (Shared_ROBO_NAV_ANIM): standing still, Jazz's feet stay put
    # while the camera turns, until it has turned far enough that he steps round
    # (main.rs, PIVOT_*). One 90-degree turn each way per stance (relaxed, shooting,
    # fine aim), and the shooting stance's 180-degree turns. Written after the strafe
    # set, in this order (ANIM_PIVOT): rest_l, rest_r, shoot_l, shoot_r, aim_l, aim_r,
    # shoot180_l, shoot180_r. The clips turn the reference bone, which is left out
    # (as for every clip); main.rs turns Jazz the same amount over the clip instead.
    pivots = [(f"{stance}_pivot_{d.lower()}", nav_psa, f"Nav_{game}_IdlePivot90_{d}")
              for stance, game in (("rest", "Rest"), ("shoot", "Shoot"), ("aim", "FineAim"))
              for d in ("L", "R")]
    pivots += [(f"shoot_pivot180_{d.lower()}", nav_psa, f"Nav_Shoot_IdlePivot180_{d}") for d in ("L", "R")]
    # Meanwhile the upper body turns towards the camera: the game's aim offset poses,
    # as additive turns left and right per stance (ANIM_TWIST): rest_l, rest_r,
    # shoot_l, shoot_r, aim_l, aim_r.
    twists = [(f"{stance}_twist_{d.lower()}", nav_psa, f"Aim_{game}_{d}_C", f"Aim_{game}_F_C")
              for stance, game in (("rest", "Rest"), ("shoot", "Shoot"), ("aim", "FineAim"))
              for d in ("L", "R")]
    if not os.path.exists(nav_psa):
        print("  ! Shared_ROBO_NAV_ANIM.psa not found - Jazz turns on the spot without stepping")
        pivots = [(name, nav, "Aim_Rest_F_C") for name, _, _ in pivots]
        twists = [(name, None, None, None) for name, _, _, _ in twists]
    # Switching weapons (the guns' TnWeaponEventEquip / UnEquip animations): the new
    # gun unfolds out of Jazz's forearm (its Equip_WEP clip, CAMT_Weapon mask: the
    # R_CloudA_* gun bones) while the one put away folds back in on the second set of
    # gun bones (Unequip_WEP, PlayOnCloudB: R_CloudB_*), at the same time. Written
    # last, in this order (ANIM_EQUIP): rifle equip, rifle unequip, Ion Blaster equip
    # and unequip, Burst Rifle equip and unequip, Scatter Blaster equip and unequip,
    # Nucleon Charge Rifle equip and unequip (its clip is UnEquip_WEP), Path Blaster
    # equip and unequip, Riot Cannon equip and unequip (its clips have no _WEP),
    # Orbital Pistol equip and unequip.
    cloud_b = lambda b: b.startswith("R_CloudB_")
    clouds = lambda b: "_Cloud" in b
    # Each gun's animation set, its clips' prefix and the ends of its equip, unequip
    # and switching hands clips' names.
    gun_sets = (("rifle", rifle_psa, "ADD_Weapon_AssaultRifle_", ("Equip_WEP", "Unequip_WEP", "MirrorReverse_WEP")),
                ("ion", ion_psa, "ADD_Weapon_SubsonicRepeater_", ("Equip_WEP", "Unequip_WEP", "MirrorReverse_WEP")),
                ("burst", burst_psa, "ADD_Weapon_PulseRifle_", ("Equip_WEP", "Unequip_WEP", "MirrorReverse_WEP")),
                ("scatter", scatter_psa, "ADD_Weapon_Scatterblaster_", ("Equip_WEP", "Unequip_WEP", "MirrorReverse_WEP")),
                ("sniper", sniper_psa, "ADD_Weapon_SniperRifle_", ("Equip_WEP", "UnEquip_WEP", "MirrorReverse_WEP")),
                ("path", path_psa, "ADD_Weapon_PathBlaster_", ("Equip_WEP", "Unequip_WEP", "MirrorReverse_WEP")),
                ("riot", riot_psa, "ADD_Weapon_RiotCannon_", ("Equip", "Unequip", "MirrorReverse")),
                ("orbital", orbital_psa, "ADD_Weapon_SysCorruptor_", ("Equip_WEP", "Unequip_WEP", "MirrorReverse_WEP")))
    equips = []
    for name, psa, prefix, (equip, unequip, _) in gun_sets:
        equips.append((f"{name}_equip", 30, False, [(psa, prefix + equip, 1.0, gun)]))
        equips.append((f"{name}_unequip", 30, False, [(psa, prefix + unequip, 1.0, cloud_b)]))
    # Switching hands (TnWeaponEventChangeHandedness): MirrorReverse_WEP folds the gun
    # into the left forearm while it unfolds out of the right one (played mirrored,
    # the other way round). Written after the switches (ANIM_HAND_SWITCH): rifle, Ion
    # Blaster, Burst Rifle, Scatter Blaster, Nucleon Charge Rifle, Path Blaster, Riot
    # Cannon, Orbital Pistol.
    for name, psa, prefix, (_, _, mirror) in gun_sets:
        equips.append((f"{name}_change_hands", 30, False, [(psa, prefix + mirror, 1.0, clouds)]))
    if not all(os.path.exists(p) for p in (rifle_psa, ion_psa, burst_psa, scatter_psa, sniper_psa, path_psa,
                                           riot_psa, orbital_psa)):
        equips = [(name, 2, False, []) for name, _, _, _ in equips]
    # Meanwhile Jazz's forearms change over (Jazz_Arm_ROBO_ANIM, the animation set his
    # forearm parts append): ADD_Weapon_MirrorReverse_ARM unfolds the forearm of the
    # arm the gun leaves out of it, and folds the other away (both on their CloudB
    # bones, where the forearms' CloudB copies are skinned), the hand drawing back
    # into the forearm or coming out of it, the fingers closing or opening, and the
    # arms moving with it. Its spine and leg motion is left out (it would fight the
    # running and jumping). Written last (ANIM_ARM_SWITCH).
    arm_psa = psa_file(anim_dir, "Jazz_Arm_ROBO_ANIM")
    if not os.path.exists(arm_psa):
        arm_psa = find("AnimSet", "Jazz_Arm_ROBO_ANIM.psa")
    arm_bones = lambda b: b[:2] in ("L_", "R_") and "_CloudA_" not in b and not any(
        k in b for k in ("_Leg", "Knee", "_Face"))
    if os.path.exists(arm_psa):
        equips.append(("arm_change_hands", 30, False, [(arm_psa, "ADD_Weapon_MirrorReverse_ARM", 1.0, arm_bones)]))
    else:
        print("  ! Jazz_Arm_ROBO_ANIM.psa not found - Jazz's forearms don't fold while he switches hands")
        equips.append(("arm_change_hands", 2, False, []))
    tail_additive, tail_anims = new_weapon_clips(anim_dir, scout_psa, gun, body, cloud_b, clouds)
    if anims is None:
        more = twists = equips = tail_additive = tail_anims = None
    else:
        more = more + pivots
    build_skinned("robot.glb", jazz + weapons + weapons_left, YAW, pose=aim, anims=anims, additive=additive,
                  robot_wheels=True, more_anims=more, aim_offsets=twists,
                  cloud_b=[os.path.basename(f)[:-5] for f in weapons + weapons_left] + list(FOREARMS_CLOUD_B),
                  more_additive=equips, tail_additive=tail_additive, tail_anims=tail_anims)
    # Every clip again, mirrored left for right, for the gun on his left arm.
    if anims is not None and weapons_left:
        additive_names = {c[0] for c in (additive or []) + (twists or []) + (equips or []) + (tail_additive or [])}
        size, n = mirror_anims.mirror(os.path.join(OUT, "models", "robot.glb"), jazz[0],
                                      os.path.join(OUT, "models", "robot_mirror.glb"),
                                      lambda i, name: name in additive_names)
        print(f"wrote models/robot_mirror.glb ({n} mirrored clips, {size / 1e6:.1f} MB)")
    build_weapon_pickups()
    build_vehicle(anim_dir)
    build_skinned("enemy.glb", [os.path.join(SK, "AP_AttackSentry_SKEL.gltf")], YAW)
    build_static("props/piston.glb", os.path.join(ST, "PROP_UND_PistonSet_C_STAT.gltf"))
    build_static("props/wall.glb", os.path.join(ST, "ENV_RST_EXT_Wall01_STAT.gltf"))
    build_static("props/pipes.glb", os.path.join(ST, "vt_pipeset_02_STAT.gltf"), translation=(0, 3.68, 0))
    build_static("props/vent.glb", os.path.join(ST, "ENV_RST_VentCover01_STAT.gltf"))

    build_energon("props/energon.glb", os.path.join(ST, "PROP_NEU_EnergonCube_STAT.gltf"))
    # The ammo stations' crate (see the top of this file).
    crate = find("StaticMesh3", "AmmoPickupLight_STAT.gltf")
    if os.path.exists(crate) and os.path.exists(tex_path("PROP_NEU_AmmoPickup_NEW_texset_Color_NormX.png")):
        build_ammo_crate("props/ammo_crate.glb", crate)
    else:
        print("  ! no AmmoPickupLight_STAT export (or its textures): props/ammo_crate.glb left out")
    # The heavy ammo crate and the equipment (see STATION_PROPS).
    build_station_props()
    # The Escalation health station's base (see the top of this file).
    health_base = find("StaticMesh3", "ESC_PickupBase_Health_STAT.gltf")
    if os.path.exists(health_base) and os.path.exists(tex_path("MP_PickupBase_TEXSET_Color_NormX.png")):
        build_static("props/health_base.glb", health_base)
    else:
        print("  ! no ESC_PickupBase_Health_STAT export (or its textures): props/health_base.glb left out")
    build_static("props/grenade.glb", os.path.join(ST, "WEP_Grenade_Flak_STAT.gltf"))
    build_floor("floor.glb", half=float(os.environ.get("FOC_FLOOR_HALF", "130")), tile=20.0)
    build_sky("sky.glb")
