"""Build the multiplayer customization garage (UI_CustomCharacter_m) for the Bevy demo.

The game's Customize screen (UI_GFxCustomCharacter_p.CustomCharacter_GFX, in
UI_Lobby_m.xxx) is played over this level: UI_Lobby_m streams it in (its "Customize"
level-streaming state), four preview characters stand on its platforms (its
Kismet's PawnFactory actions spawn them at four markers), and the camera moves
between the views of CAM_UI_Strategies_p.Customization_STRATEGY (in TransGame.xxx).

Inputs
  * the umodel export of UI_CustomCharacter_m (StaticMesh3 + Texture2D):
        umodel -game=trans -export -gltf -png -path=<CookedPC> -out=<dir> UI_CustomCharacter_m.xxx
    Its textures' full-size mips live in the packages the level's props come from,
    which umodel reads from <CookedPC> (PROP_NEU_WallProps_p, ENV_IAC_Decagon_p,
    ENV_NEU_vintageprops_p, AAA_RS_ENV_01_p and 20 more: umodel names any that are
    missing as it exports).
  * <CookedPC>: UI_CustomCharacter_m.xxx and its _LM.xxx (the placements, light maps,
    lights and fog) and TransGame.xxx (the camera views).

Outputs (under <foc_esc/assets>)
  * models/garage.glb             the garage: every placed static mesh, light-mapped
                                  as build_map.py does the maps (the physics props,
                                  which the game lights where they stand, lit by its
                                  lights as vertex lighting: actor_light). The platforms' light
                                  beams and the glow over the focused class are nodes
                                  with an "effect" extra (src/customize.rs shows the
                                  focused class's glow), drawn with their game
                                  materials as the maps' effect meshes are (levelfx.py)
  * levels/garage_fx.txt, textures/levelfx/, shaders/levelfx/garage.wgsl and
    shaders/levelfx.wgsl          its effects (levelfx.py, src/levelfx.rs): the beams'
                                  and glows' materials, its lens flares (the soft
                                  glows on the wall lamps and over the platforms) and
                                  the steam over the platforms' lamps
  * levels/garage_lightmaps/      the light map pages
  * levels/garage_lights.txt      the lights, fog and bloom (lighting.rs's format)
  * levels/garage_points.txt      the preview spots and the camera views (below)

garage_points.txt, in Bevy space (metres, y up):
  preview <specialty> x y z  fx fz            (feet, and the way the character faces)
  glow <specialty> x y z                      (the glow sphere over that class)
  camera <id> absolute x y z  fx fy fz        (position and forward direction)
  camera <id> orbit ox oy oz  lx ly lz        (orbit point and camera, relative to
                                               the preview character's feet)
  fov <degrees>  lerp <seconds>               (horizontal field of view, blend time)

usage: python build_garage.py <UI_CustomCharacter_m export dir> <CookedPC> <foc_esc/assets>
"""
import json
import os
import struct
import sys

import numpy as np
from PIL import Image

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
sys.path.insert(0, os.path.join(HERE, "ue3"))
import build_level as bl  # noqa: E402
import levelfx  # noqa: E402
import lighting  # noqa: E402
from gltf_io import Doc, Writer  # noqa: E402
from layout import extract, material_texsets, props_any, read_struct_array  # noqa: E402
from ue3pkg import Package, Reader  # noqa: E402

LEVEL = "UI_CustomCharacter_m.xxx"
NAME = "garage"
LIGHTMAP_DIR = "levels/garage_lightmaps"
# Effect meshes, drawn by the demo with its own additive material: the blue light
# beams over the platforms (blubeams_MATINST, a LightVolume_Base_MAT) and the glow
# over the focused class (ccGlow_MAT_INST, GlowSphereCheap_Parent_MAT; its Kismet
# shows one for FOCUS_<class>In and hides it for FOCUS_<class>Out).
EFFECTS = {"LightBeam_Cone_STAT": "beam", "GlowSphere2Sided_STAT": "glow"}
# The four preview characters: the markers their PawnFactory actions spawn them at
# (Main_Sequence.PreviewCharacters: PreviewScout and so on), by specialty.
MARKERS = {
    "Scout": "SkeletalMeshActor_7948",
    "Leader": "SkeletalMeshActor_5615",
    "Soldier": "HmSkeletalMeshActor_2429",
    "Scientist": "SkeletalMeshActor_1905",
}
CAMERAS = "CAM_UI_Strategies_p.Customization_STRATEGY.TnCustomizationCameraBehavior_6570"


def bevy(v):
    """Unreal position (cm) -> Bevy (m)."""
    return (v[0] * 0.01, v[2] * 0.01, v[1] * 0.01)


def bevy_dir(pitch, yaw):
    """Forward direction of an Unreal rotation (degrees), in Bevy axes."""
    p, y = np.radians(pitch), np.radians(yaw)
    return (np.cos(p) * np.cos(y), np.sin(p), np.cos(p) * np.sin(y))


# UE3's light map basis (levelfx_shader.wgsl's FX_LM_BASIS): three directions in a
# vertex's tangent frame (tangent, Unreal's bitangent, normal).
LM_BASIS = np.array([[0.0, 0.81649658, 0.57735027],
                     [-0.70710678, -0.40824829, 0.57735027],
                     [0.70710678, -0.40824829, 0.57735027]])


def actor_light(lights, ps, T):
    """The light the level's lights give a mesh the game moves (a KActor: it has no
    light map, and its DynamicLightEnvironment gathers the lights onto it where it
    stands), placed with T (Bevy space), as a 1D light map holds light: per vertex of
    its primitives `ps` (their "_pos", "_nrm" and "_tan"), the light on a surface
    facing along its normal (RGB), and its light from each of the basis directions.
    Each light's own falloff and colour (UE3's: (1 - (d / radius)^2)^falloff), from
    in front of the surface only; nothing shadows it."""
    A, t = T[:3, :3], T[:3, 3]
    mirrored = np.sign(np.linalg.det(A))
    pos = np.concatenate([q["_pos"] for q in ps]).astype(np.float64) @ A.T + t
    n = np.concatenate([q["_nrm"] for q in ps]).astype(np.float64) @ np.linalg.inv(A)
    n /= np.maximum(np.linalg.norm(n, axis=1, keepdims=True), 1e-9)
    tan = np.concatenate([q["_tan"] for q in ps]).astype(np.float64)
    tg = tan[:, :3] @ A.T
    tg -= n * (tg * n).sum(1, keepdims=True)
    tg /= np.maximum(np.linalg.norm(tg, axis=1, keepdims=True), 1e-9)
    # (As the shader takes it: Bevy's bitangent runs the other way.)
    bt = -np.cross(n, tg) * (np.where(tan[:, 3:4] < 0, -1.0, 1.0) * mirrored)
    light = np.zeros((len(pos), 3, 3))  # vertex, basis direction, RGB
    for lt in lights:
        to = np.array(bevy(lt["position"])) - pos
        d = np.maximum(np.linalg.norm(to, axis=1), 1e-6)
        dirn = to / d[:, None]
        reach = np.clip(1.0 - (d / (lt["radius"] * 0.01)) ** 2, 0.0, 1.0) ** lt["falloff"]
        if lt["class"].startswith("Spot"):
            axis = np.array([lt["direction"][0], lt["direction"][2], lt["direction"][1]])
            cos_in, cos_out = np.cos(np.radians(lt["inner"])), np.cos(np.radians(lt["outer"]))
            reach *= np.clip(((-dirn * axis).sum(1) - cos_out) / max(cos_in - cos_out, 1e-4), 0.0, 1.0) ** 2
        facing = (dirn * n).sum(1)
        reach *= facing > 0.0
        if not reach.any():
            continue
        colour = (np.array(lt["color"], np.float64) / 255.0) ** 2.2 * lt["brightness"]
        local = np.stack([(dirn * tg).sum(1), (dirn * bt).sum(1), facing], 1)
        light += (reach[:, None] * np.clip(local @ LM_BASIS.T, 0.0, None))[:, :, None] * colour
    return (light.sum(1) / np.sqrt(3.0)).astype(np.float32), light.sum(2)


class GarageMats(bl.Mats):
    """build_level's texture-set materials, plus the garage's others."""

    def get(self, mat):
        if mat in self.cache:
            return self.cache[mat]
        w = self.w
        gm = None
        if mat and mat.endswith("_CLR_Mat") and bl.find_tex(mat[:-len("_Mat")]):
            # Plain textures named after the material: X_CLR, X_NORM (Unreal's
            # green-down normals) and X_GLOW.
            base = mat[:-len("_CLR_Mat")]
            color = bl.load_rgba(bl.find_tex(base + "_CLR"))[..., :3]
            h, wd = color.shape[:2]
            gm = {"name": mat, "pbrMetallicRoughness": {"baseColorTexture": {"index": w.texture(base + "_c", bl.jpg(color))},
                                                        "metallicFactor": 0.0, "roughnessFactor": 0.75}}
            if bl.find_tex(base + "_NORM"):
                n = np.asarray(Image.open(bl.find_tex(base + "_NORM")).convert("RGB").resize((wd, h), Image.BILINEAR)).copy()
                n[..., 1] = 255 - n[..., 1]
                gm["normalTexture"] = {"index": w.texture(base + "_n", bl.jpg(n, 88, 512))}
            if bl.find_tex(base + "_GLOW"):
                g = np.asarray(Image.open(bl.find_tex(base + "_GLOW")).convert("RGB").resize((wd, h), Image.BILINEAR))
                gm["emissiveTexture"] = {"index": w.texture(base + "_e", bl.jpg(g, 80, 512))}
                gm["emissiveFactor"] = [1, 1, 1]
        elif mat == "IAC_Glass_DecoEnd_MATINST":
            # The floor slices' glass ends: ENV_ForceField_MATINST, translucent teal
            # waves (WaveColor) at OpacityIntensity 0.1 with an edge glow.
            gm = {"name": mat, "alphaMode": "BLEND", "doubleSided": True,
                  "pbrMetallicRoughness": {"baseColorFactor": [0.0, 0.25, 0.3, 0.25], "metallicFactor": 0.0, "roughnessFactor": 0.2},
                  "emissiveFactor": [0.005 * 0.5, 0.619 * 0.5, 0.759 * 0.5]}
        elif mat == "MonitorScreenActiveOrange_MATINST" and bl.find_tex("ScreenText"):
            # MonitorScreen_Parent_MAT: the screen's text in Screen_Color.
            t = bl.load_rgba(bl.find_tex("ScreenText"))[..., :3].astype(np.float32)
            lum = t.mean(-1, keepdims=True) / 255
            em = (lum * np.array([0.932, 0.431, 0.133]) * 255).clip(0, 255).astype(np.uint8)
            gm = {"name": mat, "pbrMetallicRoughness": {"baseColorFactor": [0.02, 0.02, 0.02, 1], "metallicFactor": 0.0, "roughnessFactor": 0.3},
                  "emissiveTexture": {"index": w.texture("screen_e", bl.jpg(em))}, "emissiveFactor": [1, 1, 1]}
        elif mat in ("blubeams_MATINST", "ccGlow_MAT_INST"):
            # (Without its game material, levelfx.rs's: the demo's stand-in effect.)
            gm = {"name": mat, "pbrMetallicRoughness": {"baseColorFactor": [0.2, 0.5, 0.9, 1], "metallicFactor": 0.0}}
        if gm is None:
            return super().get(mat)
        self.cache[mat] = w.material(gm)
        return self.cache[mat]


def actor_props(pkg, name):
    for i, e in enumerate(pkg.exports):
        if e["name"] == name and pkg.full_name(i + 1).startswith("TheWorld.PersistentLevel."):
            pr, _ = props_any(pkg, e, (4, 30, 0, 8))
            return pr
    raise SystemExit(f"{name} not found in {LEVEL}")


def vec3(v):
    return struct.unpack("<3f", v[2][:12])


def rot3(v):
    return [x * 360.0 / 65536.0 for x in struct.unpack("<3i", v[2][:12])]


def camera_views(cooked):
    """The customization camera views (TnCustomizationCameraBehavior.CameraDatas)."""
    pkg = Package(os.path.join(cooked, "TransGame.xxx"))
    i = next(i for i, e in enumerate(pkg.exports) if pkg.full_name(i + 1) == CAMERAS)
    pr, _ = props_any(pkg, pkg.exports[i], (4, 0, 8, 12, 16, 20, 24))
    views = []
    for item in read_struct_array(pkg, pr["CameraDatas"][1]):
        cid = item["CameraId"][1]
        n = struct.unpack_from("<i", cid)[0]
        views.append({"id": cid[4:4 + n - 1].decode("latin-1"),
                      "type": item["CameraType"],
                      "location": vec3(item["Location"]),
                      "rotation": rot3(item["Rotation"]),
                      "orbit": vec3(item["OrbitPointOffset"]),
                      "offset": vec3(item["LocationOffsetFromOrbitPoint"])})
    return pr.get("Fov", 70.0), pr.get("LerpTime", 1.0), views


def main(src, cooked, out):
    bl.configure(src, cooked, out)
    level_path = os.path.join(cooked, LEVEL)
    pkg, placements = extract(level_path)
    # (With the directions its light comes from: the surfaces drawn with their game
    # materials shade with them.)
    light = bl.Lighting(level_path, model=None, lightmap_dir=LIGHTMAP_DIR, directions=True)
    w = Writer()
    mats = GarageMats(w, material_texsets(pkg))
    root = w.node({"name": "Garage", "children": []}, root=True)
    # Its effects (levelfx.py): the effect meshes' materials, emitters and lens flares.
    lfx = levelfx.LevelFx(NAME, cooked, [LEVEL, "mp_shared.xxx", "all_shared.xxx"])
    fx_mats = {}
    surface_mats = {}

    # The preview spots, for the glow spheres' classes too.
    spots = {}
    for spec, marker in MARKERS.items():
        pr = actor_props(pkg, marker)
        loc = vec3(pr["Location"])
        yaw = rot3(pr["Rotation"])[1] if "Rotation" in pr else 0.0
        spots[spec] = (loc, yaw)

    docs, meshes = {}, {}
    placed = skipped = 0
    lit = {"2d": 0, "1d": 0, "moved": 0, "none": 0}
    # (The lights a moved prop takes its light from, actor_light.)
    level_lights = [lt for lt in lighting.lights(pkg)
                    if lt["enabled"] and "position" in lt and lt["brightness"] > 0]
    glows = []
    for it in placements:
        name = bl.last(it["mesh"])
        f = os.path.join(bl.ST, name + ".gltf")
        if not os.path.exists(f):
            skipped += 1
            continue
        if name not in docs:
            docs[name] = Doc(f)
        d = docs[name]
        over = tuple(bl.last(m) for m in it.get("materials") or [])
        key = (name, over)
        if key not in meshes:
            ps, first = [], 0
            for pi, p in enumerate(d.g["meshes"][0]["primitives"]):
                mname = d.g["materials"][p["material"]]["name"] if "material" in p else None
                if pi < len(over) and over[pi]:
                    mname = over[pi]
                at = p["attributes"]
                nverts = d.g["accessors"][at["POSITION"]]["count"]
                start, first = first, first + nverts
                if nverts == 0 or d.g["accessors"][p["indices"]]["count"] == 0:
                    continue
                # (An effect mesh: drawn with its game material by levelfx.rs.)
                fi = lfx.material(mname) if name in EFFECTS else None
                if fi is not None:
                    if fi not in fx_mats:
                        fx_mats[fi] = w.material(levelfx.fx_gltf_material(NAME, fi, mname))
                    vals = levelfx.fx_mesh_attributes(d, at, nverts, lfx.mesh_has_colours(name))
                    attrs = {k: w.accessor(v, 34962, k == "POSITION") for k, v in
                             zip(("POSITION", "NORMAL", "TANGENT", "TEXCOORD_0", "TEXCOORD_1", "COLOR_0"), vals)}
                    ps.append({"attributes": attrs,
                               "indices": w.accessor(d.acc(p["indices"]).reshape(-1).astype(np.uint32), 34963),
                               "material": fx_mats[fi], "_range": (start, nverts)})
                    continue
                # A surface drawn with its game material (levelfx.rs's SurfaceMaterial,
                # where it has a light map): shaded as the game shades it, with its
                # normal map and highlights from its light map's light; its glTF
                # material (the texture set's, with extras "surface:<n>") stands in
                # where it has none. It takes levelfx.rs's vertex data (its colours
                # white: vertex lighting puts its light there).
                si = lfx.surface(mname)
                if si is not None:
                    if si not in surface_mats:
                        gm = dict(w.g["materials"][mats.get(mname)])
                        gm["name"] = f"surface_{mname}"
                        gm["extras"] = {"foc": f"surface:{si}"}
                        surface_mats[si] = w.material(gm)
                    vals = list(levelfx.fx_mesh_attributes(d, at, nverts))
                    vals[5] = np.ones((nverts, 4), np.float32)
                    attrs = {k: w.accessor(v, 34962, k == "POSITION") for k, v in
                             zip(("POSITION", "NORMAL", "TANGENT", "TEXCOORD_0", "TEXCOORD_1", "COLOR_0"), vals)}
                    ps.append({"attributes": attrs,
                               "indices": w.accessor(d.acc(p["indices"]).reshape(-1).astype(np.uint32), 34963),
                               "material": surface_mats[si], "_range": (start, nverts),
                               "_uv1": "TEXCOORD_1" in at, "_pos": vals[0], "_nrm": vals[1], "_tan": vals[2]})
                    continue
                attrs = {"POSITION": w.accessor(d.acc(at["POSITION"]).astype(np.float32), 34962, True),
                         "NORMAL": w.accessor(d.acc(at["NORMAL"]).astype(np.float32), 34962),
                         "TEXCOORD_0": w.accessor(d.acc(at["TEXCOORD_0"]).astype(np.float32), 34962)}
                if "TANGENT" in at:
                    attrs["TANGENT"] = w.accessor(d.acc(at["TANGENT"]).astype(np.float32), 34962)
                if "TEXCOORD_1" in at:
                    attrs["TEXCOORD_1"] = w.accessor(d.acc(at["TEXCOORD_1"]).astype(np.float32), 34962)
                ps.append({"attributes": attrs, "indices": w.accessor(d.acc(p["indices"]).reshape(-1).astype(np.uint32), 34963),
                           "material": mats.get(mname), "_range": (start, nverts), "_uv1": "TEXCOORD_1" in at})
            if ps:
                w.g["meshes"].append({"name": name, "primitives": [{k: v for k, v in q.items() if not k.startswith("_")}
                                                                   for q in ps]})
                meshes[key] = (len(w.g["meshes"]) - 1, ps, first)
            else:
                meshes[key] = None
        if meshes[key] is None:
            skipped += 1
            continue
        mesh_index, ps, nverts = meshes[key]
        M = np.array(it["matrix"], np.float64).reshape(4, 4)
        A = bl.P @ M[:3, :3].T @ bl.P
        t = bl.P @ M[3, :3] * 0.01
        T = np.eye(4); T[:3, :3] = A; T[:3, 3] = t
        node = {"name": name, "mesh": mesh_index, "matrix": [float(f"{x:.7g}") for x in T.T.ravel()]}
        kind = "none"
        if name in EFFECTS:
            extras = {"effect": EFFECTS[name]}
            if EFFECTS[name] == "glow":
                # Which class's platform it is over.
                loc = M[3, :3]
                spec = min(spots, key=lambda s: np.hypot(*(np.array(spots[s][0][:2]) - loc[:2])))
                extras["class"] = spec
                glows.append((spec, bevy(loc)))
            node["extras"] = extras
        else:
            lm = light.meshes.get(it.get("component"))
            if lm is not None and lm["kind"] == "2d" and lm["dirmax"] in light.baked and all(q.get("_uv1", True) for q in ps):
                (sx, sy), (bx, by) = lm["coord_scale"], lm["coord_bias"]
                node["extras"] = light.extras(lm["dirmax"], (bx, by, bx + sx, by + sy))
                kind = "2d"
            else:
                vertex_light = None
                if lm is not None and lm["kind"] == "1d" and len(lm["samples"]) == nverts:
                    # Vertex lighting, as build_level.py does it.
                    irr = lighting.vertex_irradiance(lm)
                    vertex_light = (irr, lighting.vertex_directions(lm), "1d")
                elif it.get("class") == "KActor" and all("_pos" in q for q in ps):
                    # Something the game moves (a physics prop): lit by its lights
                    # where it stands (actor_light), shaded with its game material.
                    irr, by_dir = actor_light(level_lights, ps, T)
                    dirs = by_dir / np.maximum(by_dir.sum(1, keepdims=True), 1e-9)
                    vertex_light = (irr, np.where(by_dir.sum(1, keepdims=True) > 1e-9, dirs, 1.0 / 3.0), "moved")
                if vertex_light is not None:
                    irr, dirs, kind = vertex_light
                    # (The page holds up to LIGHTMAP_RANGE: brighter vertices stop there.)
                    peak = np.clip(irr.max(0), 1e-4, bl.LIGHTMAP_RANGE)
                    rgba = np.concatenate([irr / peak, np.ones((len(irr), 1), np.float32)], 1)
                    rgba = np.round(np.clip(rgba, 0, 1) * 255).astype(np.uint8)
                    # (Where its light comes from: the vertices' directions, weighed by
                    # their light.)
                    weight = irr.sum(1, keepdims=True) + 1e-6
                    u, v = light.vertex_texel(peak, (dirs * weight).sum(0) / weight.sum())
                    prims = []
                    for q in ps:
                        start, n = q["_range"]
                        attrs = dict(q["attributes"])
                        attrs["COLOR_0"] = w.accessor(rgba[start:start + n], 34962, normalized=True)
                        if "TEXCOORD_1" not in attrs:
                            attrs["TEXCOORD_1"] = bl.zero_uv(w, n)
                        prims.append({"attributes": attrs, "indices": q["indices"], "material": q["material"]})
                    w.g["meshes"].append({"name": name + "_vertexlit", "primitives": prims})
                    node["mesh"] = len(w.g["meshes"]) - 1
                    node["extras"] = light.extras("vertex", (u, v, u, v))
        lit[kind] += 1
        w.g["nodes"][root]["children"].append(w.node(node))
        placed += 1
    print(f"static meshes: {placed} placed, {skipped} skipped, {len(meshes)} unique")
    if light.ok:
        print(f"  lighting: {lit['2d']} light-mapped, {lit['1d']} vertex-lit, {lit['moved']} lit as they stand "
              f"(moved props), {lit['none']} without light")
    if mats.missing:
        print("  materials without textures:", sorted(m for m in mats.missing if m))

    os.makedirs(os.path.join(out, "models"), exist_ok=True)
    os.makedirs(os.path.join(out, "levels"), exist_ok=True)
    size = w.save(os.path.join(out, "models", f"{NAME}.glb"))
    print(f"wrote models/{NAME}.glb ({size / 1e6:.1f} MB)")
    if light.ok:
        n = light.write_pages()
        print(f"wrote {LIGHTMAP_DIR}/ ({n} pages)")
        bl.write_lights(light.pkg, NAME, "Customization garage lighting, read from UI_CustomCharacter_m.xxx by tools/build_garage.py",
                        height_fogs=True)
    lfx.scan(Package(level_path))
    # Its captured cube map, which its surfaces reflect.
    if lfx.reflection_cube(Package(level_path), out):
        print(f"  reflection cube map: {lfx.cube[0]} ({lfx.cube[1]} mips)")
    lfx.write(out)

    fov, lerp, views = camera_views(cooked)
    lines = ["# The customization garage's preview spots and camera views, in Bevy space (metres),",
             "# from UI_CustomCharacter_m.xxx and TransGame.xxx by tools/build_garage.py.",
             "# preview <specialty> x y z  fx fz",
             "# glow <specialty> x y z",
             "# camera <id> absolute x y z  fx fy fz",
             "# camera <id> orbit ox oy oz  lx ly lz",
             "# fov <horizontal degrees> lerp <seconds>"]
    for spec, (loc, yaw) in spots.items():
        x, y, z = bevy(loc)
        fx, _, fz = bevy_dir(0.0, yaw)
        lines.append("preview %s %.3f %.3f %.3f  %.4f %.4f" % (spec, x, y, z, fx, fz))
    for spec, (x, y, z) in glows:
        lines.append("glow %s %.3f %.3f %.3f" % (spec, x, y, z))
    for v in views:
        if v["type"] == "CCT_Absolute":
            x, y, z = bevy(v["location"])
            fx, fy, fz = bevy_dir(v["rotation"][0], v["rotation"][1])
            lines.append("camera %s absolute %.3f %.3f %.3f  %.4f %.4f %.4f" % (v["id"], x, y, z, fx, fy, fz))
        else:
            ox, oy, oz = bevy(v["orbit"])
            lx, ly, lz = bevy(v["offset"])
            lines.append("camera %s orbit %.3f %.3f %.3f  %.3f %.3f %.3f" % (v["id"], ox, oy, oz, lx, ly, lz))
    lines.append("fov %.2f lerp %.3f" % (fov, lerp))
    with open(os.path.join(out, "levels", f"{NAME}_points.txt"), "w") as fh:
        fh.write("\n".join(lines) + "\n")
    print(f"wrote levels/{NAME}_points.txt ({len(spots)} previews, {len(views)} camera views)")


if __name__ == "__main__":
    if len(sys.argv) != 4:
        raise SystemExit(__doc__)
    main(*sys.argv[1:4])
