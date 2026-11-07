"""Build the assets of the demo's Customize screen (src/customize.rs) and of its
in-game Choose Character screen (src/choose.rs) from the game.

The game's multiplayer character customization is the Scaleform movie
UI_GFxCustomCharacter_p.CustomCharacter_GFX (in UI_Lobby_m.xxx) played over the
garage level UI_CustomCharacter_m (built separately by tools/build_garage.py). This
script gathers what the demo needs besides the garage:

  assets/customize/img/*.png     the movie's images (and the CLIK components' and
                                 shared icons it imports), named by what they are,
                                 plus a few the movie draws as vector shapes
  assets/customize/customize.txt its text (UIText.int) and the game data its lists
                                 show (TransCustomization.ini, TransWeapons.ini and
                                 their names and descriptions from TransGame.int);
                                 built into the demo as it is compiled (so that
                                 its unlocks can't be edited): build the demo
                                 again after this has changed it
  assets/models/robot_cust.glb   Jazz's customization pose (Cust_Idle) and his idle
                                 holding a gun (the Scout's Nav_Idle_Shoot), as clips
                                 for robot.glb's skeleton

In a match, the builds made there are chosen from the game's Choose Character screen,
UI_GFxCustomize_p.CustomTransformers_GFX (in mp_shared.xxx, the multiplayer UI
controller's InGameLobbyUI, and the pause menu's Choose Character), and a character
chosen mid-match is announced by UI_GFxOnlineNotifies_p.OnlineNotify_GFX (in
TransGame.xxx). Their images come straight from those packages (no export needed):

  assets/choose/img/*.png        the two movies' images, the vehicle icons and the
                                 backdrop they draw as vector shapes (in white, or
                                 black for the backdrop: the demo tints them)

Their text goes into customize.txt with the Customize screen's.

usage: python build_customize.py <TransGame folder> <foc_esc/assets> <animations folder> <texture folder>...
       python build_customize.py --parts <TransGame folder> <foc_esc/assets>
           rewrites only customize.txt's part and armorset lines (every class's)
       python build_customize.py --pad-icons <TransGame folder> <foc_esc/assets>
           writes only the hint bar's controller button pictures (PAD_ICONS)

  <TransGame folder>  the game's TransGame folder (Config/PC/Cooked/Coalesced.ini,
                      Localization/Cooked/PC/Coalesced.int, CookedPC/UI_Lobby_m.xxx,
                      CookedPC/mp_shared.xxx, CookedPC/TransGame.xxx)
  <animations folder> umodel's export of Jazz's animation sets (the one
                      build_assets.py uses): Shared_ROBO_ANIM/RB_Jazz_CHARSKEL_ArmLoL.gltf
                      and Shared_SCOUT_ROBO_ANIM/Shared_SCOUT_ROBO_ANIM.psa
  <texture folder>... umodel Texture2D exports searched for the images, e.g.
      umodel -game=trans -export -png -path=<CookedPC> UI_Lobby_m.xxx      (CustomCharacter_GFX_*)
      umodel -game=trans -export -png -path=<CookedPC> UI_FrontEnd_m.xxx   (CLIKComponents_GFX_*, Icon_Class*)
      umodel -game=trans -export -png -path=<CookedPC> TransGame.xxx       (ItemWireframe*)

customize.txt holds "string <key> <text>" lines (menu.txt's format) and tab-separated
data lines:
  class    <specialty> <robot speed> <vehicle speed> <health> <aux part type>
  part     <id> <part type> <name> <specialties> <price> <lock>
           (every class's: its parts, bonus heads, DLC and the wheels it can fit, with
           the classes each is for; the decal sets, every class's. The price is its
           RequiresCurrency in shards, 0 for a free part; a DLC part, which the game
           sells separately, has its type's price as the game's own sets have it. The
           lock is what it needs besides: - nothing, dlc a DLC part (sold for its
           price in the demo), max its class at the top level or Prime Mode
           (RequiresMaxLevelOrPrime), prime Prime Mode (RequiresPrime), prime_max both
           (RequiresMaxLevelAndPrime), prime_all Prime Mode and every class at the top
           level (RequiresAllMaxLevelAndPrime))
  armorset <id> <name> <part ids, comma-separated> <specialty>   (the DLC's too, in the
           game's order)
  swatch   <faction 0/1> <id> <primary r g b> <secondary r g b>   (the ini's linear 0-255)
  voice    <specialty> <id> <name>
  weapon   <id> <specialties> <type> <level> <damage> <range> <accuracy> <fire rate> <name> <description>
  wupgrade <id> <weapon> <level> <name> <description>
  ability  <id> <specialties> <level> <name> <description>
  skill    <id> <ability> <level> <name> <description>
"""
import math
import os
import re
import shutil
import sys

import numpy as np
from PIL import Image, ImageDraw

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
sys.path.insert(0, os.path.join(HERE, "ue3"))
import build_menu  # noqa: E402
import gfx_render  # noqa: E402
from gltf_io import Doc, Writer  # noqa: E402
from psa_io import Psa  # noqa: E402

# Images: output name -> texture names to look for (the first found wins).
IMAGES = {
    # Class select (sprites 89, 93, 101, 107, 113-120)
    "slot_up.png": ["CustomCharacter_GFX_I4B"],
    "slot_over.png": ["CustomCharacter_GFX_I53"],
    "slot_disabled.png": ["CustomCharacter_GFX_I4E"],
    "class_plate.png": ["CustomCharacter_GFX_I5B"],
    "xp_marker.png": ["CustomCharacter_GFX_I63"],
    "level_hex.png": ["CustomCharacter_GFX_I69"],
    "icon_scout.png": ["Icon_ClassScout"],
    "icon_leader.png": ["Icon_ClassLeader"],
    "icon_soldier.png": ["Icon_ClassSoldier"],
    "icon_scientist.png": ["Icon_ClassScientist"],
    # Editor pages (menu_PanelBase, TitleLabel) and info panels
    "panel.png": ["CustomCharacter_GFX_I9C"],
    "title_plate.png": ["CustomCharacter_GFX_I9F"],
    "wheel_a.png": ["CustomCharacter_GFX_I96"],
    "wheel_b.png": ["CustomCharacter_GFX_I99"],
    "info_panel.png": ["CustomCharacter_GFX_IA6"],
    "info_panel_large.png": ["ContextPanelLargeNew"],
    "info_title.png": ["CustomCharacter_GFX_IA9"],
    "sub_title.png": ["CustomCharacter_GFX_IB2"],
    "upgrade_bar.png": ["CustomCharacter_GFX_ICF"],
    "upgrade_arrow.png": ["CustomCharacter_GFX_ID2"],
    "ruler.png": ["CustomCharacter_GFX_IBD"],
    "stat_damage.png": ["CustomCharacter_GFX_ICA"],
    "stat_range.png": ["CustomCharacter_GFX_ICB"],
    "stat_accuracy.png": ["CustomCharacter_GFX_IC9"],
    "stat_fire_rate.png": ["CustomCharacter_GFX_ICC"],
    "stat_robot_speed.png": ["CustomCharacter_GFX_IDD"],
    "stat_vehicle_speed.png": ["Icon_StatVehicleSpeed"],
    "stat_health.png": ["CustomCharacter_GFX_IDE"],
    # Rows (3DButton, 3DListItemRenderer, CustomizeColorSwatchButton), the scroll
    # bar, the slider and the swatch grid
    "bar_up.png": ["CLIKComponents_GFX_IF2", "CustomCharacter_GFX_I37"],
    "bar_over.png": ["CLIKComponents_GFX_I105", "CustomCharacter_GFX_I3F"],
    "lock.png": ["CLIKComponents_GFX_IF6"],
    "scroll_track.png": ["CLIKComponents_GFX_I4", "CustomCharacter_GFX_I31"],
    "scroll_thumb.png": ["CLIKComponents_GFX_I7", "CustomCharacter_GFX_I2A"],
    "scroll_thumb_over.png": ["CLIKComponents_GFX_IA", "CustomCharacter_GFX_I2D"],
    "slider_arrow.png": ["CLIKComponents_GFX_I116", "CustomCharacter_GFX_I1E"],
    "slider_bar.png": ["CLIKComponents_GFX_I11A", "CustomCharacter_GFX_I22"],
    "slider_track.png": ["CLIKComponents_GFX_I11E", "CustomCharacter_GFX_I26"],
    "swatch_frame.png": ["CustomCharacter_GFX_IE6"],
    "swatch_frame_over.png": ["CustomCharacter_GFX_IE9"],
    # The shop's notice (mc_notifyChallenge): its panel and its corner strokes
    "notify_bg.png": ["CustomCharacter_GFX_IA"],
    "notify_frame.png": ["CustomCharacter_GFX_ID"],
}
# Weapon and ability pictures for the Loadouts page's info panel (ItemWireframe<id>,
# with the movie's remaps), as wire_<id>.png.
WIREFRAME_REMAP = {"FastAbilityCooldownStreak": "FastAbilityCooldown", "OrbitalReconStreak": "OrbitalRecon",
                   "MinePooperStreak": "MinePooper", "HealthRegenStreak": "HealthRegen",
                   "TracerDart": "SysCorruptor", "Barrier": "DifBarrier", "ShieldAbility": "Barrier",
                   "HealBeam": "HealthRegen"}

# The movie's text (UIText.int sections), and a few from other sections; the
# Customization section and the last two are the Choose Character screen's.
STRING_SECTIONS = ["CustomCharacter", "Customization"]
STRINGS = ["ButtonHints.Back", "ButtonHints.Close", "Shared.None",
           "CharacterSelect.CharacterLevel", "MessagePrompts.CharacterSelectedInGame"]

# The Choose Character screen's images: output name -> (package in CookedPC, texture).
CHOOSE_TEXTURES = {
    # A weapon slot's card (sprite 181) and the ability's (sprite 190).
    "card_weapon.png": ("mp_shared.xxx", "UI_GFxCustomize_p.CustomTransformers_GFX_IB3"),
    "card_ability.png": ("mp_shared.xxx", "UI_GFxCustomize_p.CustomTransformers_GFX_IBC"),
    # The list's selection bar (selection_mc, sprite 210's first frame).
    "select_bar.png": ("mp_shared.xxx", "UI_GFxCustomize_p.CustomTransformers_GFX_ICE"),
    # The corners round an icon (sprite 99).
    "corner.png": ("mp_shared.xxx", "UI_GFxCustomize_p.CustomTransformers_GFX_I61"),
    # OnlineNotify_GFX's message: its panel (sprite 3) and the strokes over its
    # top and bottom corners (sprite 6).
    "notify_panel.png": ("TransGame.xxx", "UI_GFxOnlineNotifies_p.OnlineNotify_GFX_I1"),
    "notify_corner.png": ("TransGame.xxx", "UI_GFxOnlineNotifies_p.OnlineNotify_GFX_I4"),
}
# The hint bar's controller buttons: the button icons the movie's own hint
# renderer puts before a hint's label (hms.data.ButtonIconData: "Gamepad_" and the
# button's id, from UI_GFxButtonIcons_p), for the buttons the demo's hints are on
# (src/customize.rs, Hint::pad_button).
PAD_ICONS = ["Gamepad_X", "Gamepad_Y", "Gamepad_B", "Gamepad_L3", "Gamepad_R3", "Gamepad_DRIGHT"]
# Its vector shapes, drawn in white at this many pixels per movie pixel: the
# classes' vehicle icons (mc_chassisIcon*, shown 2.49 times their size) and the
# vehicles behind the class list (mc_customSelection's shape 205).
CHOOSE_SHAPES = {
    "chassis_car.png": (352, 8.0),
    "chassis_truck.png": (376, 8.0),
    "chassis_tank.png": (304, 8.0),
    "chassis_jet.png": (328, 8.0),
    "class_vehicles.png": (205, 2.0),
}

SPECIALTIES = ["Scout", "Leader", "Soldier", "Scientist"]
# Every class's parts and armor sets (each part with the classes it is offered to:
# a few of the Infiltrator's wheels fit the Destroyer too), in the game's order. A
# class whose characters the demo has (models/<class>, build_class.py) offers its
# own; the others the Infiltrator's, whose characters stand in for theirs. The
# decal sets (DecalSetNone and the eight the shop sells: tools/build_decals.py) are
# every class's.
# What a part needs besides its price (its Requires* flags, as
# TnXpManager.GetOptionLockStatus checks them in this order): its class at the top
# level or Prime Mode, Prime Mode, both, or Prime Mode and every class at the top.
LOCKS = [("RequiresMaxLevelOrPrime", "max"), ("RequiresPrime", "prime"),
         ("RequiresMaxLevelAndPrime", "prime_max"), ("RequiresAllMaxLevelAndPrime", "prime_all")]


def part_prices(parts):
    """Each part type's price (RequiresCurrency) as the game's own sets have it: the
    price the DLC's parts (LockedForDLC, sold separately and priced 0) take in the
    demo, which sells them like the rest."""
    prices = {}
    for _, f in parts:
        price = int(first(f, "RequiresCurrency", "0") or 0)
        if "LockedForDLC" not in f and price:
            prices[first(f, "PartType")] = max(prices.get(first(f, "PartType"), 0), price)
    return prices


def part_lines(cust, names):
    """The part and armorset lines, every class's."""
    def name_of(uid, kind):
        return first(names.get(f"{uid} {kind}", {}), "FriendlyName", uid)

    def classes(f):
        return [c for c in first(f, "SpecialtyRestriction").split(",") if c in SPECIALTIES]

    def price_and_lock(f):
        kind = first(f, "PartType")
        if "LockedForDLC" in f and first(f, "LockedForDLC").lower() == "true":
            return str(standard.get(kind, 0)), "dlc"
        lock = next((name for key, name in LOCKS if first(f, key).lower() == "true"), "-")
        return str(int(first(f, "RequiresCurrency", "0") or 0)), lock

    parts = providers(cust, "TnDataProvider_Part")
    standard = part_prices(parts)
    lines = []
    for uid, f in parts:
        kind = first(f, "PartType")
        if kind == "DecalSet":
            lines.append("\t".join(["part", uid, kind, name_of(uid, "TnDataProvider_Part"), ",".join(SPECIALTIES),
                                    *price_and_lock(f)]))
        elif kind != "DecalSet" and classes(f):
            lines.append("\t".join(["part", uid, kind, name_of(uid, "TnDataProvider_Part"), ",".join(classes(f)),
                                    *price_and_lock(f)]))
    for uid, f in providers(cust, "TnDataProvider_ArmorSet"):
        if classes(f):
            lines.append("\t".join(["armorset", uid, name_of(uid, "TnDataProvider_ArmorSet"),
                                     first(f, "PartsInSet"), classes(f)[0]]))
    return lines


def update_parts(transgame, assets):
    """Rewrites only customize.txt's part and armorset lines (every class's), from
    the game's ini: what --parts does, without the images' exports."""
    import build_menu
    cfg = build_menu.coalesced(os.path.join(transgame, "Config", "PC", "Cooked", "Coalesced.ini"))
    loc = build_menu.coalesced(os.path.join(transgame, "Localization", "Cooked", "PC", "Coalesced.int"))
    cust = ini_sections(cfg["TransGame/Config/PC/Cooked/TransCustomization.ini"])
    names = ini_sections(loc["TransGame/Localization/INT/TransGame.int"])
    path = os.path.join(assets, "customize", "customize.txt")
    old = open(path, encoding="utf-8").read().split("\n")
    new = part_lines(cust, names)
    at = next((i for i, line in enumerate(old) if line.split("\t")[0] in ("part", "armorset")), len(old))
    kept = [line for line in old if line.split("\t")[0] not in ("part", "armorset")]
    lines = kept[:at] + new + kept[at:]
    while lines and not lines[-1]:
        lines.pop()
    with open(path, "w", encoding="utf-8") as fh:
        fh.write("\n".join(lines) + "\n")
    print(f"wrote customize/customize.txt ({len(new)} part and armorset lines)")
    print("  (built into the demo: build it again, cargo build --release, to use it)")


def find_texture(dirs, name):
    for d in dirs:
        for root, _, files in os.walk(d):
            if name + ".png" in files:
                return os.path.join(root, name + ".png")
    return None


def ini_sections(text):
    """[name] -> {key: [values]} (keys can repeat)."""
    out = {}
    cur = None
    for line in text.splitlines():
        line = line.strip()
        if line.startswith("[") and line.endswith("]"):
            cur = out.setdefault(line[1:-1], {})
        elif cur is not None and "=" in line and not line.startswith(";"):
            k, v = line.split("=", 1)
            v = re.sub(r"\s*//.*$", "", v.strip())
            cur.setdefault(k.strip(), []).append(v)
    return out


def providers(sections, kind):
    """The rows of one data provider class, in ini order: (id, fields)."""
    return [(name.split(" ")[0], f) for name, f in sections.items() if name.endswith(" " + kind)]


def first(f, key, default=""):
    return f.get(key, [default])[0]


def gradient(path, w, h, c0, c1):
    """A horizontal gradient, colour c0 (RGBA) at the left to c1 at the right."""
    t = np.linspace(0.0, 1.0, w)[None, :, None]
    a = np.array(c0, np.float32)[None, None, :]
    b = np.array(c1, np.float32)[None, None, :]
    row = (a + (b - a) * t).round().clip(0, 255).astype(np.uint8)
    Image.fromarray(np.repeat(row, h, axis=0), "RGBA").save(path)


def polygon_image(path, size, origin, polys, ss=4):
    """White polygons (points in movie pixels, each with an alpha) drawn into an
    image `size` movie pixels big whose top-left is `origin`."""
    w, h = size
    im = Image.new("RGBA", (w * ss, h * ss), (255, 255, 255, 0))
    for pts, alpha in polys:
        layer = Image.new("L", im.size, 0)
        ImageDraw.Draw(layer).polygon([((x - origin[0]) * ss, (y - origin[1]) * ss) for x, y in pts], fill=alpha)
        a = np.maximum(np.asarray(im)[..., 3], np.asarray(layer))
        im.putalpha(Image.fromarray(a))
    im.resize((w, h), Image.LANCZOS).save(path)


def swatch_half(path, poly):
    """A half of a colour-grid tile (SwatchListItemRenderer): the polygon in a 35x35
    square (-17.5..17.5) filled with the movie's vertical gradient, white at ratio 51
    to #646464 at 255 (top to bottom); tinted per swatch by the demo."""
    ss = 4
    n = 35 * ss
    mask = Image.new("L", (n, n), 0)
    ImageDraw.Draw(mask).polygon([((x + 17.5) * ss, (y + 17.5) * ss) for x, y in poly], fill=255)
    t = (np.arange(n) + 0.5) / n  # 0 at the top
    stop0 = 51 / 255.0
    k = np.clip((t - stop0) / (1 - stop0), 0, 1)
    v = (255 + (100 - 255) * k).round().astype(np.uint8)
    rgb = np.repeat(np.repeat(v[:, None], n, axis=1)[..., None], 3, axis=2)
    im = Image.fromarray(np.dstack([rgb, np.asarray(mask)]), "RGBA")
    im.resize((35, 35), Image.LANCZOS).save(path)


def spinner(path, size=192):
    """The Randomize pulse (sprite 246): a ring of four white quarter arcs, drawn in
    white (the demo tints, fades and turns it)."""
    ss = 4
    n = size * ss
    im = Image.new("L", (n, n), 0)
    d = ImageDraw.Draw(im)
    r_out, r_in = n * 0.49, n * 0.40
    c = n / 2
    for q in range(4):
        a0 = q * 90 + 8
        d.pieslice([c - r_out, c - r_out, c + r_out, c + r_out], a0, a0 + 74, fill=255)
    d.ellipse([c - r_in, c - r_in, c + r_in, c + r_in], fill=0)
    a = im.resize((size, size), Image.LANCZOS)
    out = Image.new("RGBA", (size, size), (255, 255, 255, 0))
    out.putalpha(a)
    out.save(path)


def cust_idle(anims, robot_glb, out_path):
    """Jazz's customization poses on a copy of robot.glb's node tree without its
    meshes, so its bones get the same animation targets as robot.glb's:

      cust_idle    Cust_Idle (TR_Iconic_Car_ANM_p.Shared_ROBO_ANIM, a still two-frame
                   clip): the robot's idle in the customization stance
                   (STANCE_Customization's pick in the robot anim tree's IdleStance,
                   TR_Shared_ANIMTREE_p.Robot_ANIMTREE)
      cust_weapon  Nav_Idle_Shoot (the Scout's, Shared_SCOUT_ROBO_ANIM.psa: the
                   Infiltrator's set over the shared ones), looping: his idle with
                   a gun. Given its weapon, the preview pawn's stance is pushed to
                   STANCE_GunArmShootFromHip (TnCharacterScriptBinding.
                   GiveWeaponNextFrame), whose IdleStance pick that is."""
    # Cust_Idle's first pose, per bone and channel: from umodel's glTF export of Jazz
    # with the set (saved from its viewer) or, without one, the set's .psa.
    gltf = os.path.join(anims, "Shared_ROBO_ANIM", "RB_Jazz_CHARSKEL_ArmLoL.gltf")
    pose = []
    if os.path.exists(gltf):
        src = Doc(gltf)
        clip = next((a for a in src.g.get("animations", []) if a["name"] == "Cust_Idle"), None)
        if clip is None:
            raise SystemExit("Cust_Idle not found in the Shared_ROBO_ANIM export")
        for c in clip["channels"]:
            v = src.acc(clip["samplers"][c["sampler"]]["output"])
            pose.append((src.g["nodes"][c["target"]["node"]]["name"], c["target"]["path"], v[:1]))
    else:
        bones, _ = Psa(os.path.join(anims, "Shared_ROBO_ANIM", "Shared_ROBO_ANIM.psa")).clip("Cust_Idle")
        if bones is None:
            raise SystemExit("Cust_Idle not found in Shared_ROBO_ANIM.psa")
        for bone, (pos, rot) in bones.items():
            pose += [(bone, "translation", pos[:1]), (bone, "rotation", rot[:1])]
    robot = Doc(robot_glb)
    w = Writer()
    for n in robot.g["nodes"]:
        w.g["nodes"].append({k: v for k, v in n.items() if k not in ("mesh", "skin")})
    w.g["scenes"] = [{"nodes": robot.g["scenes"][robot.g.get("scene", 0)]["nodes"]}]
    names = {n.get("name"): i for i, n in enumerate(w.g["nodes"])}

    def wanted(bone, path):
        # As build_assets.py: the reference bone carries root motion, left out.
        return bone in names and "Root_Reference" not in bone and path in ("translation", "rotation")

    samplers, channels = [], []
    t = np.array([0.0, 1.0], np.float32)
    for bone, path, v in pose:
        if not wanted(bone, path):
            continue
        v = np.repeat(v[:1].astype(np.float32), 2, axis=0)
        samplers.append({"input": w.accessor(t, minmax=True), "output": w.accessor(v), "interpolation": "LINEAR"})
        channels.append({"sampler": len(samplers) - 1, "target": {"node": names[bone], "path": path}})
    w.g["animations"] = [{"name": "cust_idle", "samplers": samplers, "channels": channels}]
    report = [f"cust_idle, {len(channels)} channels"]

    psa_path = os.path.join(anims, "Shared_SCOUT_ROBO_ANIM", "Shared_SCOUT_ROBO_ANIM.psa")
    bones, dt = (Psa(psa_path).clip("Nav_Idle_Shoot") if os.path.exists(psa_path) else (None, None))
    if bones is None:
        print("  ! Nav_Idle_Shoot not found (Shared_SCOUT_ROBO_ANIM.psa) - the preview robot holds")
        print("    his gun in the customization pose")
    else:
        # A loop: its first frame again at the end, as the game wraps it.
        frames = len(next(iter(bones.values()))[0])
        t = np.arange(frames + 1, dtype=np.float32) * np.float32(dt)
        tin = w.accessor(t, minmax=True)
        samplers, channels = [], []
        for bone, (pos, rot) in bones.items():
            for path, v in (("translation", pos), ("rotation", rot)):
                if not wanted(bone, path):
                    continue
                v = np.concatenate([v, v[:1]]).astype(np.float32)
                samplers.append({"input": tin, "output": w.accessor(v), "interpolation": "LINEAR"})
                channels.append({"sampler": len(samplers) - 1, "target": {"node": names[bone], "path": path}})
        w.g["animations"].append({"name": "cust_weapon", "samplers": samplers, "channels": channels})
        report.append(f"cust_weapon, {len(channels)} channels, {frames * dt:.2f} s")
    size = w.save(out_path)
    print(f"wrote models/robot_cust.glb ({'; '.join(report)}, {size // 1024} KB)")


def backdrop(movie, path):
    """The Choose Character screen's backdrop (the root's shape 424): its two
    panels (the band behind the title and the panel behind the character's
    loadout) filled with a radial gradient, black at (459.95, 189) fading out
    555 x 595 pixels away, and its two hairlines, as a 1120 x 720 image of the
    stage, black with the shape's alpha."""
    c = movie.chars[424]
    fills, lines = c["styles"][0]
    g = fills[0]
    a, _, _, d, tx, ty = g["matrix"]
    ss = 4
    w, h = 1120, 720
    mask = Image.new("1", (w * ss, h * ss), 0)
    from PIL import ImageChops
    for _, f0, f1, _, segs in c["paths"]:
        if not (f0 or f1):
            continue
        for loop in gfx_render._loops(segs):
            if len(loop) > 2:
                m = Image.new("1", mask.size, 0)
                ImageDraw.Draw(m).polygon([(x * ss, y * ss) for x, y in loop], fill=1)
                mask = ImageChops.logical_xor(mask, m)
    cover = np.asarray(mask.convert("L").resize((w, h), Image.LANCZOS), np.float32) / 255.0
    # The gradient square is 1638.4 pixels across (32768 twips) before its matrix.
    yy, xx = np.mgrid[0:h, 0:w].astype(np.float32) + 0.5
    r = np.hypot((xx - tx) / (a * 819.2), (yy - ty) / (d * 819.2))
    (r0, c0), (r1, c1) = g["stops"][0], g["stops"][-1]
    k = np.clip((r * 255.0 - r0) / max(r1 - r0, 1), 0.0, 1.0)
    alpha = (c0[3] + (c1[3] - c0[3]) * k) / 255.0 * cover
    im = Image.fromarray((alpha * 255.0).round().clip(0, 255).astype(np.uint8), "L")
    # The hairlines (line style 1: 0.1 pixels, drawn one pixel wide).
    draw = ImageDraw.Draw(im)
    for _, _, _, ln, segs in c["paths"]:
        if ln:
            for seg in segs:
                draw.line([(seg[1], seg[2]), (seg[-2], seg[-1])], fill=lines[ln - 1]["color"][3], width=1)
    out = Image.new("RGBA", (w, h), (0, 0, 0, 0))
    out.putalpha(im)
    out.save(path)


def choose_assets(transgame, assets):
    """The Choose Character screen's images (assets/choose/img)."""
    out = os.path.join(assets, "choose", "img")
    os.makedirs(out, exist_ok=True)
    cooked = os.path.join(transgame, "CookedPC")
    print("copying the Choose Character screen's images")
    import texture
    from ue3pkg import Package
    packages = {}
    for name, (pkg_file, full) in CHOOSE_TEXTURES.items():
        if pkg_file not in packages:
            packages[pkg_file] = Package(os.path.join(cooked, pkg_file))
        pkg = packages[pkg_file]
        idx = next((i + 1 for i in range(len(pkg.exports)) if pkg.full_name(i + 1) == full), None)
        if idx is None:
            print(f"  ! {full} not in {pkg_file}")
            continue
        rgba, _ = texture.read_texture(pkg, idx, cooked)
        Image.fromarray(np.ascontiguousarray(rgba), "RGBA").save(os.path.join(out, name))
    print("drawing its vector shapes")
    movie = build_menu.load_movie(os.path.join(cooked, "mp_shared.xxx"),
                                  "UI_GFxCustomize_p.CustomTransformers_GFX", os.path.join(out, "movie.gfx"))
    # Where each shape's image goes, in the shape's own pixels (rasterize draws its
    # outline's extent plus a pixel all round): "<image> <x0> <y0> <x1> <y1>".
    rects = []
    for name, (cid, scale) in CHOOSE_SHAPES.items():
        build_menu.rasterize(movie, cid, scale, os.path.join(out, name))
        pts = [p for loop in build_menu.shape_loops(movie, cid) if len(loop) > 2 for p in loop]
        xs, ys = [x for x, _ in pts], [y for _, y in pts]
        rects.append(f"{os.path.splitext(name)[0]} {min(xs) - 1:.2f} {min(ys) - 1:.2f} {max(xs) + 1:.2f} {max(ys) + 1:.2f}")
    with open(os.path.join(out, "..", "shapes.txt"), "w", encoding="utf-8") as fh:
        fh.write("\n".join(rects) + "\n")
    backdrop(movie, os.path.join(out, "backdrop.png"))
    # background_BLACK_mc's ramp (shape 422): black down to y 473, fading out by y
    # 715; the demo draws the black above the ramp itself.
    ramp = np.linspace(255.0, 0.0, 256)[:, None].repeat(4, axis=1)
    im = Image.new("RGBA", (4, 256), (0, 0, 0, 0))
    im.putalpha(Image.fromarray(ramp.round().astype(np.uint8), "L"))
    im.save(os.path.join(out, "fade.png"))


def pad_icons(transgame, assets):
    """The hint bar's controller button pictures (assets/customize/img), from
    TransGame.xxx's UI_GFxButtonIcons_p."""
    out = os.path.join(assets, "customize", "img")
    os.makedirs(out, exist_ok=True)
    cooked = os.path.join(transgame, "CookedPC")
    print("copying the controller's button pictures")
    import texture
    from ue3pkg import Package
    pkg = Package(os.path.join(cooked, "TransGame.xxx"))
    for name in PAD_ICONS:
        full = "UI_GFxButtonIcons_p." + name
        idx = next((i + 1 for i in range(len(pkg.exports)) if pkg.full_name(i + 1) == full), None)
        if idx is None:
            print(f"  ! {full} not in TransGame.xxx")
            continue
        rgba, _ = texture.read_texture(pkg, idx, cooked)
        Image.fromarray(np.ascontiguousarray(rgba), "RGBA").save(os.path.join(out, name + ".png"))


def main(transgame, assets, anims, tex_dirs):
    choose_assets(transgame, assets)
    pad_icons(transgame, assets)
    out = os.path.join(assets, "customize")
    img = os.path.join(out, "img")
    os.makedirs(img, exist_ok=True)

    print("copying images")
    for name, sources in IMAGES.items():
        path = next((p for p in (find_texture(tex_dirs, s) for s in sources) if p), None)
        if path is None:
            print(f"  ! none of {sources} in the texture exports")
            continue
        shutil.copyfile(path, os.path.join(img, name))

    print("drawing the movie's vector shapes")
    # CustomizeStatBar (sprite 74): track #61DBDB, bar #F2914C -> #F8DFA1 (shape 72).
    gradient(os.path.join(img, "xp_bar.png"), 200, 8, (242, 145, 76, 255), (248, 223, 161, 255))
    # CustomizeCompareStatBar (sprite 196): (242,110,76,204) -> (248,160,95,255) (shape 193).
    gradient(os.path.join(img, "stat_bar.png"), 200, 8, (242, 110, 76, 204), (248, 160, 95, 255))
    # CustomizeColorSwatchButton's chips (146): primary and secondary, with their facets.
    polygon_image(os.path.join(img, "chip_primary.png"), (20, 40), (0, 0),
                  [([(13.55, 32.65), (13.55, 10.2), (19.45, 0), (5.9, 0), (0, 10.2), (0, 32.65)], 255),
                   ([(0, 32.65), (13.55, 32.65), (17.2, 39), (3.65, 39)], 127)])
    polygon_image(os.path.join(img, "chip_secondary.png"), (22, 40), (0, 0),
                  [([(15.3, 32.65), (15.3, 10.2), (21.2, 0), (5.9, 0), (0, 10.2), (0, 32.65)], 255),
                   ([(0, 32.65), (15.3, 32.65), (18.95, 39), (3.65, 39)], 127)])
    # SwatchListItemRenderer (236): the tile's two halves.
    swatch_half(os.path.join(img, "swatch_primary.png"), [(-10.1, 17.5), (10.1, -17.5), (-17.5, -17.5), (-17.5, 17.5)])
    swatch_half(os.path.join(img, "swatch_secondary.png"), [(9.3, -17.5), (-10.9, 17.5), (17.5, 17.5), (17.5, -17.5)])
    spinner(os.path.join(img, "spinner.png"))
    # The currency ("shards") symbol, shape 95 of the movie.
    cooked = os.path.join(transgame, "CookedPC")
    try:
        m = build_menu.load_movie(os.path.join(cooked, "UI_Lobby_m.xxx"),
                                  "UI_GFxCustomCharacter_p.CustomCharacter_GFX", os.path.join(out, "movie.gfx"))
        build_menu.rasterize(m, 95, 3.0, os.path.join(img, "currency.png"))
    except Exception as e:  # noqa: BLE001
        print("  ! couldn't draw the currency symbol:", e)

    print("reading the game's text and data")
    loc = build_menu.coalesced(os.path.join(transgame, "Localization", "Cooked", "PC", "Coalesced.int"))
    cfg = build_menu.coalesced(os.path.join(transgame, "Config", "PC", "Cooked", "Coalesced.ini"))
    uitext = ini_sections(loc["TransGame/Localization/INT/UIText.int"])
    names = ini_sections(loc["TransGame/Localization/INT/TransGame.int"])
    cust = ini_sections(cfg["TransGame/Config/PC/Cooked/TransCustomization.ini"])
    weap = ini_sections(cfg["TransGame/Config/PC/Cooked/TransWeapons.ini"])

    lines = ["# The Customize screen's text and data, made by tools/build_customize.py"]
    for sec in STRING_SECTIONS:
        for k, v in uitext.get(sec, {}).items():
            lines.append(f"string {sec}.{k} {v[0]}")
    for key in STRINGS:
        sec, k = key.split(".", 1)
        v = uitext.get(sec, {}).get(k)
        if v:
            lines.append(f"string {key} {v[0]}")
        else:
            print("  ! no text for", key)

    def name_of(uid, kind):
        f = names.get(f"{uid} {kind}", {})
        desc = first(f, "MpDescription") or first(f, "Description")
        return first(f, "FriendlyName", uid), desc.replace("\t", " ")

    wires = set()
    for uid, f in providers(cust, "TnDataProvider_Specialty"):
        aux = next((p for p in first(f, "RequiredPartTypes").split(",") if p in ("Wheels", "Wings", "Cannon")), "Wheels")
        lines.append("\t".join(["class", uid, first(f, "RobotSpeed", "0"), first(f, "VehicleSpeed", "0"),
                                first(f, "Health", "0"), aux]))
    lines += part_lines(cust, names)
    for uid, f in providers(cust, "TnDataProvider_ColorSwatch"):
        c = [first(f, k, "0") for k in ("Primary_R", "Primary_G", "Primary_B", "Secondary_R", "Secondary_G", "Secondary_B")]
        lines.append("\t".join(["swatch", first(f, "FactionRestriction", "0"), uid] + c))
    for uid, f in providers(cust, "TnDataProvider_RobotVoice"):
        lines.append("\t".join(["voice", first(f, "SpecialtyRestriction"), uid, name_of(uid, "TnDataProvider_RobotVoice")[0]]))
    for uid, f in providers(weap, "TnDataProvider_Weapon"):
        if "MP" not in f.get("SupportedGames", []):
            continue
        wtype = first(f, "WeaponType", "0").split(",")[0]
        if wtype not in ("1", "2"):
            continue
        n, d = name_of(uid, "TnDataProvider_Weapon")
        lines.append("\t".join(["weapon", uid, first(f, "SpecialtyRestriction"), wtype, first(f, "RequiresLevel", "0"),
                                first(f, "DamageRating", "0"), first(f, "RangeRating", "0"),
                                first(f, "AccuracyRating", "0"), first(f, "FireRateRating", "0"), n, d]))
        wires.add(uid)
    for uid, f in providers(weap, "TnDataProvider_WeaponUpgrade"):
        if "MP" not in f.get("SupportedGames", []):
            continue
        n, d = name_of(uid, "TnDataProvider_WeaponUpgrade")
        lines.append("\t".join(["wupgrade", uid, first(f, "WeaponRestriction"), first(f, "RequiresLevel", "0"), n, d]))
    for uid, f in providers(cust, "TnDataProvider_Ability"):
        specs = first(f, "SpecialtyRestriction")
        if not any(s in specs.split(",") for s in SPECIALTIES):
            continue
        n, d = name_of(uid, "TnDataProvider_Ability")
        lines.append("\t".join(["ability", uid, specs, first(f, "RequiresLevel", "0"), n, d]))
        wires.add(uid)
    for uid, f in providers(cust, "TnDataProvider_Skill"):
        ability = first(f, "AbilityRestriction")
        if not ability:
            continue
        n, d = name_of(uid, "TnDataProvider_Skill")
        lines.append("\t".join(["skill", uid, ability, first(f, "RequiresLevel", "0"), n, d]))

    print("copying the weapon and ability pictures")
    for uid in sorted(wires):
        src = find_texture(tex_dirs, "ItemWireframe" + WIREFRAME_REMAP.get(uid, uid))
        src = src or find_texture(tex_dirs, "ItemWireframeUnknown")
        if src:
            shutil.copyfile(src, os.path.join(img, f"wire_{uid}.png"))
        else:
            print("  ! no picture for", uid)

    with open(os.path.join(out, "customize.txt"), "w", encoding="utf-8") as fh:
        fh.write("\n".join(lines) + "\n")
    print(f"wrote customize/customize.txt ({len(lines)} lines)")
    print("  (built into the demo: build it again, cargo build --release, to use it)")

    cust_idle(anims, os.path.join(assets, "models", "robot.glb"), os.path.join(assets, "models", "robot_cust.glb"))


if __name__ == "__main__":
    if len(sys.argv) == 4 and sys.argv[1] == "--parts":
        update_parts(sys.argv[2], sys.argv[3])
    elif len(sys.argv) == 4 and sys.argv[1] == "--pad-icons":
        pad_icons(sys.argv[2], sys.argv[3])
    elif len(sys.argv) < 5:
        raise SystemExit(__doc__)
    else:
        main(sys.argv[1], sys.argv[2], sys.argv[3], sys.argv[4:])
