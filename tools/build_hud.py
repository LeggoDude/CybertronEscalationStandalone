"""Builds the demo's HUD from Fall of Cybertron's own HUD movie.

usage: python build_hud.py <CookedPC folder> <umodel Texture2D folder> <foc_esc/assets folder>

The game's HUD is a Scaleform (Flash) movie, UI_GFxHud_p.Hud_GFX, cooked into
TransGame.xxx; its fonts are the movie UI_GFxFonts_p.Fonts_EFIGS_GFX in
Startup_int.xxx. This script reads both straight from the packages and writes:

  assets/hud/hud.txt        the parts of the movie the demo uses: clips (sprites) and
                            their timelines, the textured quads, bar morphs, lines and
                            text fields inside them, frame labels and stop frames
  assets/hud/img/*.png      the images those parts use (copied from the umodel export)
  assets/hud/font_*.png     glyph atlases rasterised from the game's fonts

The images come from umodel's export of TransGame.xxx (run it first):

  umodel -game=trans -export -png -path=<CookedPC> -out=<dir> TransGame.xxx

and pass <dir>/TransGame/Texture2D as the second argument.

hud.txt is plain text, one record per line (all positions in the movie's pixels,
y down, as in Flash):

  image <name> <file> <width> <height>
  font <face> <file> <atlas w> <atlas h> <em px> <ascent> <descent> <leading>
  glyph <face> <code> <u0> <v0> <u1> <v1> <x0> <y0> <x1> <y1> <advance>
      (x/y relative to the pen on the baseline, in ems)
  quad <def> <image|-> <x0> <y0> <x1> <y1> <u0> <v0> <u1> <v1> <r> <g> <b> <a>
  bar <def> <image> <start x0 x1 y0 y1> <end x0 x1 y0 y1> <uv matrix a b c d tx ty>
  line <def> <width> <r> <g> <b> <a> <x y ...>
  text <def> <x0> <y0> <x1> <y1> <face> <size> <rrggbbaa> <align> <spacing> |<text>
  mask <def> <depth> <clip depth> <x0> <y0> <x1> <y1>
      (a mask at <depth> shows the depths above it, up to <clip depth>, only inside
      that rectangle: the bounds of the mask's shapes, in the sprite's coordinates)
  sprite <def> <frames> <name|->
  grid <def> <x0> <y0> <x1> <y1> <bx0> <by0> <bx1> <by1>
      (its scaling grid, DefineScalingGrid, and the bounds of what it shows: scaled,
      the sprite keeps the parts outside the grid at their own size, stretching the
      part inside it, 9-slice)
  label <def> <frame> <name>
  stop <def> <frame>
  goto <def> <frame> <label>        (frame script: gotoAndPlay(label))
  key <def> <frame> <depth> <child|-> <name|-> <a> <b> <c> <d> <tx> <ty>
      <mr> <mg> <mb> <ma> <ar> <ag> <ab> <aa> <ratio> <blend>
      (the state of a depth from that frame on; "-" child = removed)
  symbol <name> <def>               (clips the HUD attaches at run time)
  root <def>

Only part of the movie is kept (see KEEP and SYMBOLS): the multiplayer scoreboards,
objectives, flags and so on are left out.
"""
import os
import re
import shutil
import struct
import sys

from PIL import Image, ImageChops, ImageDraw

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
sys.path.insert(0, os.path.join(HERE, "ue3"))
import avm1_dis  # noqa: E402
import gfx_io  # noqa: E402
from layout import props_any  # noqa: E402
from ue3pkg import Package  # noqa: E402

HUD_MC = 950  # the movie's hud_mc
# Which depths of each container to keep (others are dropped). Containers not
# listed keep everything.
KEEP = {
    950: [5, 7, 13, 27, 43, 61, 81, 105, 107, 121, 123],  # scope, corners, centre,
                                          # prompts, the level-up corner (midRight_mc), glows
    787: [3],                             # topLeft_mc: the game messages (sprite 785)
    800: [1, 13],                         # bottomLeft_mc: health and shield; the
                                          # team's lives (Escalation's)
    817: [9, 11, 13, 15],                 # topRight_mc: the item bought (sprite 811),
                                          # Escalation's wave (sprite 812), currency
                                          # (energon), the countdown to the next wave
                                          # (sprite 816: src/waves.rs)
    863: [1, 17],                         # bottomRight_mc: weapon, ammo, equipment; ability
    862: [1],                             # ability_mc: the robot's (the car's has none)
    887: [1, 19],                         # midCenter_mc: crosshair group, damage ring
    879: [3, 9, 11, 13, 15, 17],          # warnings, the A-4 Pulsar's and Chaos-Rift's
                                          # detonate prompts, pings, crosshair, reload ring
    886: [1],                             # damage indicators (no missile warning)
    921: [3, 7, 9, 21],                   # topCenter_mc: announcements, the point
                                          # transactions (experience: src/xp.rs), "an
                                          # ally is downed" (src/revive.rs), counter
    924: [1],                             # midRight_mc: the level-up notices
    831: [1, 8],                          # equipment_mc: the defensive and offensive
                                          # slots
    944: [3, 5, 13],                      # bottomCenter_mc: contextual commands, the
                                          # revive's bar and the downed meter
                                          # (src/revive.rs)
}
# Clips the HUD's scripts attach (attachMovie) that the demo uses.
# (The movie's own marker for a downed teammate, mc_markerDownedAlly, never shows:
# its OnVisibleChange hides it whenever markers_mc would show it. A downed
# teammate's mc_markerPawn shows him down instead: its downed_mc, src/revive.rs.)
SYMBOLS = [
    # The Heal Beam's and Hover Slam's ability icons (updateRbIcon's
    # getAnimatedIconId: clips, src/abilities.rs): the Heal Beam's showing its juice
    # (progress_mc's frames, flash_mc as it is spent), the Hover Slam's arrow
    # turning over while it is active (arrow_mc).
    "mc_abilityIconHealBeam", "mc_abilityIconHover",
    "mc_crosshairRifle", "mc_ammoBarSegmented", "mc_ammoBarSolid", "mc_ammoBarSegment",
    "mc_currentWeaponIcon", "mc_damageIndicatorArrow", "mc_causedDamagePing",
    "mc_ammoWarningLow", "mc_ammoWarningEmpty", "mc_ammoWarningReload",
    "mc_ammoAddedNotification", "mc_currencyEarnedRightAlign",
    # The Photon Burst Rifle: its reticle, the scope shown while fine aiming
    # (scopeManager_mc) and the "Hold E to pickup" line (notifyManager_mc).
    "mc_crosshairTactical", "mc_tacticalScope", "mc_contextualCommandLine",
    # The Scatter Blaster's reticle (the game's for every TnWeaponShotgun).
    "mc_crosshairShotgun",
    # The Path Blaster's (TnWeaponHeavyPistol's): three prongs, as the rifle's.
    "mc_crosshairPistol",
    # The Riot Cannon's (updateCrosshair's for TnWeaponBazooka, as for the grenade
    # launcher and the tank cannon): three arcs, pushed out by the spread.
    "mc_crosshairCannon",
    # The vehicles' rockets' (updateCrosshair's for TnWeaponRocketVehicle, the
    # Destroyer trucks', and TnWeaponHomingRocketVehicle, the Scientist jets').
    "mc_crosshairRocket",
    # The Nucleon Charge Rifle's scope (scopeManager_mc attaches it for
    # TnWeaponSniperRifle's fine aim): its charge readout, bars and lines.
    "mc_sniperScope",
    # The marker over each other player in a match: his name and health
    # (markers_mc, beside hud_mc).
    "mc_markerPawn",
    # A buff's icon on a marker (mc_markerPawn's buffs_mc: updateBuffs), for the
    # sentries the Orbital Pistol marks.
    "mc_markerPawnBuffIcon",
    # updateCrosshair's for TnWeaponAoESlow (the Corrosive Slime Cannon) and
    # TnWeaponClusterCannon (the Chaos-Rift Combustor), for TnWeaponChainBeam (the
    # E.D.K. TechVolt) and for TnWeaponSawblade (the Gear Shredder). (The X18
    # Scrapmaker has the rifle's, the Energon Harvester the shotgun's, the A-4
    # Pulsar Cannon the cannon's and the Thermo Rocket Cannon the rocket one.)
    "mc_crosshairMortar", "mc_crosshairChainBeam", "mc_crosshairSawblade",
    # The Thermo Rocket Cannon's lock on a target (markers_mc: a LockOn marker).
    "mc_markerLockOn",
    # The detonate prompts under the crosshair: the A-4 Pulsar Cannon's while one
    # of its rounds is out (ActiveGrenadeCount) and the Chaos-Rift Combustor's
    # while its bomb can be burst (CanClusterBomb).
    "mc_activeGrenadePromptText", "mc_clusterBombPromptText",
    # Experience (src/xp.rs): an award's point transaction ("+50" and its label, the
    # TransactionQueue's under topCenter_mc) and the level-up notice (midRight_mc's
    # NotifyLevelUp; the class's icon in it is SHARED_SYMBOLS').
    "mc_pointTransaction", "mc_pointTransactionLabel", "mc_levelUpNotify",
    # A support item bought at an equipment station (topRight_mc's sprite 811:
    # NotifyStoreItemUnlocked's storeItemUnlocked attaches it), and the game's
    # messages (topLeft_mc's sprite 785: gameMessage, e.g. "Thermo Mine Deployed"),
    # each line's parts in their colours.
    "mc_itemUnlocked", "mc_gameMessage", "mc_gameMessageColor",
    # Escalation's waves (src/waves.rs): the wave the team is on, "WAVE 1 OF 15"
    # (topRight_mc's sprite 812: updateCurrentWave attaches it), and the countdown to
    # the next (sprite 816 attaches the single-team game's scoreboard, its clock).
    "mc_currentWaveMessage", "mc_scoreboardSingleTeam",
    # An enemy grenade near the player (markers_mc's 'Grenade' type: Escalation's
    # soldiers' grenades, src/ai/grenade.rs): its icon over it, blinking, and, kept
    # at the screen's edge while it is off the screen, an arrow pointing to it.
    "mc_markerGrenade",
]
# Clips the HUD attaches that the movie imports from the shared icons
# (../_Shared/SharedIcons.swf: UI_GFxShared_p.SharedIcons_GFX, in UI_FrontEnd_m.xxx):
# the classes' icons the level-up notice shows (it imports the Scout's alone, but
# names all four). Each is a white shape, rasterised as an image here (as
# RASTER_SHAPES), its clip and shape numbered from SHARED_ID_BASE up.
SHARED_MOVIE = ("UI_FrontEnd_m.xxx", "UI_GFxShared_p.SharedIcons_GFX")
SHARED_SYMBOLS = [
    "mc_specialtyIconScout", "mc_specialtyIconScientist", "mc_specialtyIconLeader",
    "mc_specialtyIconSoldier",
]
SHARED_ID_BASE = 100000
# The art the HUD movie imports from the shared movies (ImportAssets2) that its
# kept parts show, under the HUD's own ids for them: SharedAssets' (UI_GFxShared_p.
# SharedAssets_GFX, in TransGame.xxx) faction emblems on the team's lives and the
# gradient glow behind them (and behind the ability meter), each a bitmap shape;
# SharedIcons' downed-ally icon (a bitmap: the downed meter's and the downed
# marker's). The shapes are grafted in from GRAFT_ID_BASE up.
SHARED_ASSETS_MOVIE = ("TransGame.xxx", "UI_GFxShared_p.SharedAssets_GFX")
IMPORTED_ART = [(603, "mc_gradientGlowArt"), (604, "mc_emblemDecepticonArt"), (606, "mc_emblemAutobotArt")]
IMPORTED_IMAGES = [(100, "Icon_DownedAlly")]
GRAFT_ID_BASE = 200000
# The controller's buttons the HUD shows while a controller plays (PreferGamepad):
# the pictures it imports from the button icons movie (ImportAssets2 from
# ../ButtonIcons/ButtonIcons.swf: UI_GFxButtonIcons_p.ButtonIcons_GFX, in
# TransGame.xxx), Gamepad_R1 (RB) on the ability's key (rb_mc's xbox frame), X on
# the reload prompt, LT and RT on the grenade prompts and so on. Each is an image,
# kept under the HUD's own id for it (the HUD's shapes fill with it).
BUTTON_ICONS_MOVIE = ("TransGame.xxx", "UI_GFxButtonIcons_p.ButtonIcons_GFX")
BUTTON_ICONS_URL = "../ButtonIcons/ButtonIcons.swf"
# The respawn screen (UI_GFxRespawn_p.MultiplayerRespawn_GFX, in mp_shared.xxx:
# TnHUD's while the player is dead), drawn over the HUD while an Escalation player
# is out till the round ends (src/revive.rs, src/hud.rs): its soft-edged darkening
# (screenSoftEdges_mc), its line (respawnTimer_mc: "Waiting for Next Wave to
# Respawn") and the spectating group (sprite 13: the player watched, "[ Cycle
# Players ]"). Each is grafted in as a symbol, with everything under it, its
# characters numbered from RESPAWN_ID_BASE up; its texts take the fonts it imports
# ($TitleFont, $NormalFont) by name. Its one image is mp_shared.xxx's.
RESPAWN_MOVIE = ("mp_shared.xxx", "UI_GFxRespawn_p.MultiplayerRespawn_GFX")
RESPAWN_SYMBOLS = [("respawn_edges", 6), ("respawn_timer", 16), ("respawn_spectate", 13)]
RESPAWN_ID_BASE = 300000
# The packages whose Texture2Ds the movies' images are, in the order they're looked
# in.
IMAGE_PACKAGES = ["TransGame.xxx", "mp_shared.xxx"]
# Images loaded by name at run time (BitmapData.loadBitmap).
EXTRA_IMAGES = [
    # The spectating group's controller hint, its bumpers' pictures (the respawn
    # screen's buttonHints_txt image substitutions: UI_GFxGamepadButtons_p's).
    "GamepadButtonLB", "GamepadButtonRB",
    "ItemWireframeAssaultRifle", "ItemWireframeIonBlaster", "ItemWireframeBurstRifle", "ItemWireframeShotgun",
    "ItemWireframeSniperRifle", "ItemWireframeHeavyPistol", "ItemWireframeBazooka", "GrenadeIconFrag",
    # The Orbital Pistol's: its ItemWireframeTracerDart, which the weapon icon's
    # checkLinkageId turns into the campaign gun's.
    "ItemWireframeSysCorruptor",
    # The eight Titan, Scientist and Destroyer weapons': ItemWireframe<class> (the
    # Gear Shredder's TnWeaponSawblade: ItemWireframeSawBlade, the package's name
    # for it; the Thermo Rocket Cannon's TnWeaponRocketLauncher: checkLinkageId
    # turns ItemWireframeRocketLauncher into ItemWireframeHomingRocket).
    "ItemWireframeHeavyMG", "ItemWireframeDrainGun", "ItemWireframeChainBeam",
    "ItemWireframeAoESlow", "ItemWireframeSawBlade", "ItemWireframeHomingRocket",
    "ItemWireframeGrenadeLauncher", "ItemWireframeClusterCannon",
    # The Orbital Pistol's marks on a marker's buffs (getBuffIconLinkageId:
    # TnBuffHardLocked, TnBuffHardLocked2 and TnBuffHardLocked3).
    "BuffIconHardLocked", "BuffIconHardLocked2", "BuffIconHardLocked3",
    # The Infiltrator's EMP grenade (TnAbilityGrenade -> AbilityIconGrenade, which
    # robotAbility_mc's checkForSharedIcon turns into this one).
    "AbilityIconFlashBangAbility",
    "LowHealthTunnelVisionMask_CLR", "LowHealthScanline_CLR", "LowHealthScanlinePanner_CLR",
    "LowHealthScanlinesOpacityMask_CLR",
    # Ammo picked up, as mc_ammoAddedNotification lists it over the ammo count: the
    # weapon's small kill-feed icon (updateAmmoAdded's getLinkageId: TnWeapon<X> ->
    # GameMessageIcon<X>; the Orbital Pistol's TnWeaponTracerDart -> the System
    # Corruptor's; the Thermo Rocket Cannon's TnWeaponRocketLauncher; the vehicles'
    # weapons -> GameMessageIconCar, Truck, Tank and Jet).
    "GameMessageIconAssaultRifle", "GameMessageIconIonBlaster", "GameMessageIconBurstRifle",
    "GameMessageIconShotgun", "GameMessageIconSniperRifle", "GameMessageIconHeavyPistol",
    "GameMessageIconBazooka", "GameMessageIconSysCorruptor", "GameMessageIconHeavyMG",
    "GameMessageIconDrainGun", "GameMessageIconChainBeam", "GameMessageIconAoESlow",
    "GameMessageIconSawBlade", "GameMessageIconRocketLauncher", "GameMessageIconGrenadeLauncher",
    "GameMessageIconClusterCannon", "GameMessageIconCar", "GameMessageIconTruck",
    "GameMessageIconTank", "GameMessageIconJet",
    # The support items bought at the equipment stations, in the equipment slots
    # (offensive_mc and defensive_mc: updateEquipment turns the ability's class,
    # TnKillstreakMinePooper, TnAbilityBarrier, TnKillStreakHealthRegen and
    # TnKillstreakFastAbilityCooldown, into AbilityIcon<name>).
    "AbilityIconMinePooper", "AbilityIconBarrier", "AbilityIconHealthRegen",
    "AbilityIconFastAbilityCooldown",
    # The Dimensional Decimator's (TnAbilityRollerSphereGravityAura), and its kills'
    # kill-feed icon ([GravBomb]: TnDamageTypeGravityBomb).
    "AbilityIconRollerSphereGravityAura", "GameMessageIconGravBomb",
    # The classes' abilities (src/abilities.rs): robotAbility_mc's icons (AbilityIcon
    # + the ability's class, TnAbility left off; the Heal Beam's and Hover Slam's
    # are clips: SYMBOLS).
    "AbilityIconCloaking", "AbilityIconWhirlwind", "AbilityIconShield",
    "AbilityIconSpawnSentry",
    # The support items' pictures in mc_itemUnlocked (sprite 714: 'ItemWireframe' +
    # filterItemId(itemId), the item's UniqueId with its "Streak" left off).
    "ItemWireframeMinePooper", "ItemWireframeBarrier", "ItemWireframeHealthRegen",
    "ItemWireframeFastAbilityCooldown", "ItemWireframeRollerSphereGravityAura",
    # The kill feed's other icons (sprite 785's GameMessageIconBitmaps, in place of
    # their words in the death messages: src/killfeed.rs's ICONS).
    "GameMessageIconMelee", "GameMessageIconRam", "GameMessageIconKamikazeMines",
    "GameMessageIconDecoyTrap", "GameMessageIconRollerSphere", "GameMessageIconTransformDisrupt",
    "GameMessageIconShockwave", "GameMessageIconDrain", "GameMessageIconSentry",
    "GameMessageIconRocket", "GameMessageIconSuicide", "GameMessageIconHeadshot",
    "GameMessageIconGroundPound", "GameMessageIconFireDot", "GameMessageIconAutobot",
    "GameMessageIconDecepticon",
]
# Fonts: Fonts_EFIGS_GFX maps $NormalFont to Register Demi Bold, $NumberFontWide
# to Energon Numbers Wide and so on.
FACES = {
    "$NormalFont": "RegisterDemiBold",
    "$TitleFont": "RegisterMedium",
    "$HeaderFont": "RegisterLight",
    "$NumberFont": "Energon Numbers",
    "$NumberFontWide": "Energon Numbers Wide",
}
# Shapes whose fills aren't rectangles that are drawn as images instead (rasterised
# here RASTER_SCALE times the movie's size, then drawn as a textured quad over their
# bounds): the marker's arrow (mc_markerPawn's icon_mc, a chevron pointing down, shown
# over the sentries the Orbital Pistol reveals) and the grenade marker's (534, the
# chevron mc_markerGrenade's arrow_mc points with). Other such shapes are drawn as
# their bounds.
RASTER_SHAPES = {667, 534}
RASTER_SCALE = 4
# Shapes drawn with the halo a copy of them under them gives: a copy with a knockout
# DropShadowFilter at distance 0 (only the shadow drawn: the shape's outline blurred,
# black, which the movie's colour transforms then turn the shape's colour). The
# grenade marker's arrow is so: sprite 536 places it (535) twice, the copy under it
# (depth 1) with the filter (blur 10 by 10, strength 2, 3 passes). The halo is baked
# into the shape's image (RASTER_SHAPES), the filtered copy left out (GLOW_COPIES:
# sprite, depth). Shape -> (blur x, blur y in the movie's pixels, strength, passes).
GLOW_SHAPES = {534: (10.0, 10.0, 2.0, 3)}
GLOW_COPIES = {(536, 1)}
FONT_EM_PX = 64   # glyph atlas resolution (pixels per em)
FONT_PAD = 3


def with_glow(canvas, blur_x, blur_y, strength, passes):
    """The shape drawn on `canvas` (RASTER_SCALE times the movie's size) over the
    halo of a knockout DropShadowFilter at distance 0 on a copy of it under it
    (GLOW_SHAPES): its alpha box-blurred `passes` times (each box `blur_x` by `blur_y`
    of the movie's pixels, as Flash blurs), times `strength`, in black, cut out where
    the shape is (knockout), with the shape drawn over it."""
    import numpy as np
    rgba = np.asarray(canvas, dtype=np.float64) / 255.0
    shape_a = rgba[..., 3]

    def box(a, size, axis):
        # A box of `size` pixels (a fraction spread over the two end pixels), centred.
        n = max(size, 1.0)
        whole = int(n)
        frac = n - whole
        r = whole // 2
        k = np.ones(2 * r + 1 + (2 if frac > 0 else 0))
        if frac > 0:
            k[0] = k[-1] = frac / 2.0
        k /= k.sum()
        return np.apply_along_axis(lambda v: np.convolve(v, k, mode="same"), axis, a)

    halo = shape_a.copy()
    for _ in range(passes):
        halo = box(halo, blur_x * RASTER_SCALE, 1)
        halo = box(halo, blur_y * RASTER_SCALE, 0)
    halo = np.clip(halo * strength, 0.0, 1.0) * (1.0 - shape_a)
    out_a = shape_a + halo * (1.0 - shape_a)
    rgb = rgba[..., :3] * shape_a[..., None]
    safe = np.where(out_a > 0.0, out_a, 1.0)
    out = np.dstack([rgb / safe[..., None], out_a])
    return Image.fromarray(np.round(np.clip(out, 0.0, 1.0) * 255.0).astype(np.uint8), "RGBA")


def swf_movie(pkg_path, full_name):
    p = Package(pkg_path)
    for i, e in enumerate(p.exports):
        if p.full_name(i + 1) == full_name:
            pr, _ = props_any(p, e, (4, 0, 8, 12))
            d = pr["RawData"][1]
            n = struct.unpack_from("<i", d)[0]
            return d[4:4 + n]
    raise SystemExit("%s not found in %s" % (full_name, pkg_path))


def auto_sized_text(movie):
    """The text fields the movie's scripts size to their text (a sprite's script
    setting a child field's autoSize to 'left', 'center' or 'right'), by field: the
    side kept in place, which the demo draws as the field's alignment."""
    out = {}
    for c in movie.chars.values():
        if c.get("kind") != "sprite":
            continue
        named = {pl["name"]: pl.get("id") for f in c["frames"][:1] for pl in f.values() if pl.get("name")}
        for code, body in c["tags"]:
            if code != 12:
                continue
            dis = []
            avm1_dis.disasm(body, dis)
            for line in avm1_dis.decompile(dis):
                m = re.search(r"(?<![\w.])(\w+)\.autoSize = '(left|center|right)'", line)
                if m and m.group(1) in named:
                    t = movie.chars.get(named[m.group(1)])
                    if t is not None and t.get("kind") == "text":
                        out[t["id"]] = m.group(2)
    return out


class Exporter:
    def __init__(self, movie, tex_dir, out_dir, cooked=None):
        self.m = movie
        self.tex_dir = tex_dir
        # (Images umodel's export lacks are read from TransGame.xxx itself.)
        self.cooked = cooked
        self.out = out_dir
        self.lines = []
        self.done = set()
        self.images = {}   # movie image id -> name
        self.named_images = set()
        self.faces = set()
        self.warnings = []
        self.auto_align = auto_sized_text(movie)
        # (A plain field's face is its font's, as the movie imports it.)
        self.font_faces = imported_fonts(movie.tags)

    # -- images --
    def image_name(self, cid):
        info = self.m.images.get(cid)
        if not info:
            return None
        name = os.path.splitext(info["name"])[0]
        self.images[cid] = name
        return name

    def copy_images(self):
        os.makedirs(os.path.join(self.out, "img"), exist_ok=True)
        names = set(self.images.values()) | set(EXTRA_IMAGES)
        missing = []
        for name in sorted(names):
            src = os.path.join(self.tex_dir, name + ".png")
            dst = os.path.join(self.out, "img", name + ".png")
            if os.path.exists(src):
                shutil.copyfile(src, dst)
            elif not self.package_image(name, dst):
                missing.append(name)
                continue
            w, h = Image.open(dst).size
            self.lines.insert(0, "image %s img/%s.png %d %d" % (name, name, w, h))
        if missing:
            print("  ! images not found in the umodel export:", ", ".join(missing))

    def package_image(self, name, dst):
        """Writes the Texture2D `name` (its largest mip) of the first of
        IMAGE_PACKAGES that has it to `dst`; False when none does."""
        if not self.cooked:
            return False
        import texture  # noqa: E402  (tools/ue3)
        if getattr(self, "_textures", None) is None:
            self._textures = []
            for file in IMAGE_PACKAGES:
                pkg = Package(os.path.join(self.cooked, file))
                found = {}
                for i, e in enumerate(pkg.exports):
                    if pkg.class_name(e) == "Texture2D":
                        found.setdefault(e["name"], i + 1)
                self._textures.append((pkg, found))
        for pkg, found in self._textures:
            if name in found:
                rgba, _ = texture.read_texture(pkg, found[name], self.cooked)
                Image.fromarray(rgba, "RGBA").save(dst)
                return True
        return False

    # -- definitions --
    def define(self, cid):
        if cid in self.done or cid is None:
            return
        self.done.add(cid)
        c = self.m.chars.get(cid)
        if c is None:
            morph = self.m.morphs.get(cid)
            if morph:
                self.bar(cid, morph)
            else:
                self.warnings.append("character %d not found" % cid)
            return
        k = c["kind"]
        if k == "shape":
            self.shape(c)
        elif k == "text":
            self.text(c)
        elif k == "sprite":
            self.sprite(c)
        elif k == "image":
            name = self.image_name(cid)
            w, h = c["w"], c["h"]
            self.lines.append("quad %d %s 0 0 %g %g 0 0 1 1 1 1 1 1" % (cid, name, w, h))

    def raster_shape(self, c, name=None, out_id=None):
        """Shape `c` as an image (RASTER_SHAPES): its fills drawn in their own colours
        (colour transforms apply to the quad as to any other), with a pixel of the
        movie's to spare round it; written as `name` (HudShape<id>) for the
        character `out_id` (its own id)."""
        import gfx_render
        cid = c["id"]
        pts = [(s_[i], s_[i + 1]) for _, _, _, _, segs in c["paths"] for s_ in segs for i in (1, len(s_) - 2)]
        glow = GLOW_SHAPES.get(cid)
        # (Room for its halo: the blur reaches out about one and a half times its size.)
        pad = 1.0 + (1.5 * max(glow[0], glow[1]) if glow else 0.0)
        x0 = min(p[0] for p in pts) - pad
        y0 = min(p[1] for p in pts) - pad
        x1 = max(p[0] for p in pts) + pad
        y1 = max(p[1] for p in pts) + pad
        w = int(round((x1 - x0) * RASTER_SCALE))
        h = int(round((y1 - y0) * RASTER_SCALE))
        canvas = Image.new("RGBA", (w, h), (0, 0, 0, 0))
        r = gfx_render.Renderer(self.m, self.tex_dir, scale=RASTER_SCALE)
        r.draw(canvas, cid, (1.0, 0.0, 0.0, 1.0, -x0, -y0), gfx_render.CX_IDENT)
        if glow:
            canvas = with_glow(canvas, *glow)
        name = name or "HudShape%d" % cid
        os.makedirs(os.path.join(self.out, "img"), exist_ok=True)
        canvas.save(os.path.join(self.out, "img", name + ".png"))
        self.lines.insert(0, "image %s img/%s.png %d %d" % (name, name, w, h))
        self.lines.append("quad %d %s %g %g %g %g 0 0 1 1 1 1 1 1" % (
            cid if out_id is None else out_id, name, x0, y0, x1, y1))

    def shape(self, c):
        cid = c["id"]
        if cid in RASTER_SHAPES:
            self.raster_shape(c)
            return
        for set_i, f0, f1, ln, segs in c["paths"]:
            fills, lines = c["styles"][set_i]
            pts = []
            for s in segs:
                pts.append((s[1], s[2]))
                pts.append((s[-2], s[-1]))
            for fi in (f0, f1):
                if not fi:
                    continue
                f = fills[fi - 1]
                if f.get("bitmap") == 65535:
                    continue
                xs = [p[0] for p in pts]; ys = [p[1] for p in pts]
                x0, x1, y0, y1 = min(xs), max(xs), min(ys), max(ys)
                corners = {(round(p[0], 1), round(p[1], 1)) for p in pts}
                if len(corners) > 4 or any(s[0] == "Q" for s in segs):
                    self.warnings.append("shape %d: fill is not a rectangle, drawn as its bounds" % cid)
                if "bitmap" in f:
                    name = self.image_name(f["bitmap"])
                    if not name:
                        continue
                    info = self.m.images[f["bitmap"]]
                    a, b, cc, d, tx, ty = f["matrix"]
                    a, b, cc, d = a / 20, b / 20, cc / 20, d / 20
                    det = a * d - b * cc
                    ia, ib, ic, id_ = d / det, -b / det, -cc / det, a / det

                    def uv(x, y):
                        px = ia * (x - tx) + ic * (y - ty)
                        py = ib * (x - tx) + id_ * (y - ty)
                        return px / info["w"], py / info["h"]
                    u0, v0 = uv(x0, y0)
                    u1, v1 = uv(x1, y1)
                    self.lines.append("quad %d %s %g %g %g %g %.5f %.5f %.5f %.5f 1 1 1 1"
                                      % (cid, name, x0, y0, x1, y1, u0, v0, u1, v1))
                else:
                    col = f.get("color")
                    if col is None:
                        st = f.get("stops") or [(0, (255, 255, 255, 255))]
                        col = tuple(sum(s[1][i] for s in st) // len(st) for i in range(4))
                    self.lines.append("quad %d - %g %g %g %g 0 0 1 1 %.4f %.4f %.4f %.4f"
                                      % (cid, x0, y0, x1, y1, *(v / 255 for v in col)))
            if ln:
                st = lines[ln - 1]
                pts = []
                for s in segs:
                    flat = gfx_render._flatten(s, 6)
                    pts.extend(flat if not pts else flat[1:])
                col = st["color"]
                self.lines.append("line %d %g %.4f %.4f %.4f %.4f %s" % (
                    cid, st["width"], *(v / 255 for v in col),
                    " ".join("%g %g" % (round(x, 2), round(y, 2)) for x, y in pts)))

    def bar(self, cid, mo):
        name = self.image_name(mo["bitmap"])
        info = self.m.images[mo["bitmap"]]
        a, b, c, d, tx, ty = mo["matrix"]
        a, b, c, d = a / 20, b / 20, c / 20, d / 20
        det = a * d - b * c
        # uv = inverse(bitmap matrix) * p / size
        ia, ib, ic, id_ = d / det, -b / det, -c / det, a / det
        w, h = info["w"], info["h"]
        uva, uvb, uvc, uvd = ia / w, ib / h, ic / w, id_ / h
        uvtx = -(ia * tx + ic * ty) / w
        uvty = -(ib * tx + id_ * ty) / h
        s, e = mo["start"], mo["end"]
        self.lines.append("bar %d %s %g %g %g %g %g %g %g %g %.6f %.6f %.6f %.6f %.6f %.6f" % (
            cid, name, *s, *e, uva, uvb, uvc, uvd, uvtx, uvty))

    def text(self, c):
        html = c.get("text") or ""
        face = re.search(r'face="([^"]+)"', html)
        # (A plain field, not HTML: its own font's face and alignment.)
        plain_face = self.font_faces.get(c.get("font")) if not c.get("html") else None
        face = face.group(1) if face else (plain_face or "$NormalFont")
        size = re.search(r'size="([0-9.]+)"', html)
        size = float(size.group(1)) if size else c.get("height", 12)
        col = re.search(r'color="#([0-9A-Fa-f]{6})"', html)
        rgba = ("%02x%02x%02x" % c["color"][:3] if "color" in c else "ffffff") if not col else col.group(1).lower()
        rgba += "%02x" % (c.get("color", (0, 0, 0, 255))[3])
        align = re.search(r'align="(\w+)"', html)
        aligns = {0: "left", 1: "right", 2: "center", 3: "justify"}
        align = align.group(1) if align else (
            aligns.get(c.get("align", 0), "left") if not c.get("html") else "left")
        # (A field its script sizes to its text keeps that side, or its centre, in
        # place, as the alignment does.)
        align = self.auto_align.get(c["id"], align)
        spacing = re.search(r'letterSpacing="(-?[0-9.]+)"', html)
        spacing = float(spacing.group(1)) if spacing else 0.0
        plain = re.sub(r"<[^>]+>", "", html).replace("&nbsp;", " ").replace("&amp;", "&")
        x0, x1, y0, y1 = c["bounds"]
        self.faces.add(face)
        self.lines.append("text %d %g %g %g %g %s %g %s %s %g |%s" % (
            c["id"], x0, y0, x1, y1, face, size, rgba, align, spacing, plain))

    def sprite(self, c):
        cid = c["id"]
        keep = KEEP.get(cid)
        inv = {v: k for k, v in self.m.exports.items()}
        self.lines.append("sprite %d %d %s" % (cid, len(c["frames"]), inv.get(cid, "-")))
        grid = self.m.grids.get(cid)
        bounds = self.bounds(cid)
        if grid is not None and bounds is not None:
            gx0, gx1, gy0, gy1 = grid
            self.lines.append("grid %d %g %g %g %g %g %g %g %g" % ((cid, gx0, gy0, gx1, gy1) + tuple(bounds)))
        for name, f in sorted(c["labels"].items(), key=lambda x: x[1]):
            self.lines.append("label %d %d %s" % (cid, f, name))
        # Frame scripts the player needs: stop() and gotoAndPlay(label).
        frame = 0
        for code, body in c["tags"]:
            if code == 1:
                frame += 1
            elif code == 12:
                acts = frame_actions(body)
                for a in acts:
                    if a == ("stop",):
                        self.lines.append("stop %d %d" % (cid, frame))
                    elif a[0] == "goto":
                        self.lines.append("goto %d %d %s" % (cid, frame, a[1]))
        depths = sorted({d for f in c["frames"] for d in f})
        for d in depths:
            if keep is not None and d not in keep:
                continue
            if (cid, d) in GLOW_COPIES:
                continue
            last = None
            masks = set()
            for i, f in enumerate(c["frames"]):
                pl = f.get(d)
                if pl is not None and "clip" in pl:
                    # Masks are not drawn; their bounds clip the depths above them.
                    rect = self.mask_rect(pl)
                    if rect is not None:
                        masks.add((pl["clip"],) + rect)
                    pl = None
                key = None if pl is None else self.place_key(pl)
                if key != last:
                    if key is None:
                        self.lines.append("key %d %d %d -" % (cid, i, d))
                    else:
                        self.lines.append("key %d %d %d %s" % (cid, i, d, key))
                        self.define(pl.get("id"))
                    last = key
            if len(masks) > 1:
                self.warnings.append("sprite %d: the mask at depth %d changes; only one is kept" % (cid, d))
            for clip, x0, y0, x1, y1 in sorted(masks)[:1]:
                self.lines.append("mask %d %d %d %g %g %g %g" % (cid, d, clip, x0, y0, x1, y1))

    def bounds(self, cid, m=gfx_io.IDENT):
        """Bounds (x0, y0, x1, y1) of a character's shapes under matrix m."""
        c = self.m.chars.get(cid)
        if c is None:
            return None
        if c["kind"] == "shape":
            x0, x1, y0, y1 = c["bounds"]
            pts = [gfx_render._apply(m, x, y) for x, y in ((x0, y0), (x1, y0), (x1, y1), (x0, y1))]
            return (min(p[0] for p in pts), min(p[1] for p in pts),
                    max(p[0] for p in pts), max(p[1] for p in pts))
        if c["kind"] == "sprite" and c["frames"]:
            out = None
            for pl in c["frames"][0].values():
                b = self.bounds(pl.get("id"), gfx_io.mat_mul(m, pl.get("matrix", gfx_io.IDENT)))
                if b is not None:
                    out = b if out is None else (min(out[0], b[0]), min(out[1], b[1]),
                                                 max(out[2], b[2]), max(out[3], b[3]))
            return out
        return None

    def mask_rect(self, pl):
        return self.bounds(pl.get("id"), pl.get("matrix", gfx_io.IDENT))

    def place_key(self, pl):
        a, b, c, d, tx, ty = pl.get("matrix", gfx_io.IDENT)
        mul, add = pl.get("cxform", ([1.0, 1.0, 1.0, 1.0], [0, 0, 0, 0]))
        return "%d %s %.5f %.5f %.5f %.5f %.3f %.3f %.4f %.4f %.4f %.4f %d %d %d %d %d %d" % (
            pl["id"], pl.get("name") or "-", a, b, c, d, tx, ty, *mul, *add,
            pl.get("ratio", 0), pl.get("blend", 0))


def frame_actions(body):
    """A frame script's stop() and gotoAndPlay(label) (goToLabel then play), skipping
    the bodies of any functions it defines (class scripts are the demo's job)."""
    out = []
    i = 0
    label = None
    while i < len(body):
        op = body[i]
        i += 1
        if op == 0:
            break
        ln = 0
        if op >= 0x80:
            ln = struct.unpack_from("<H", body, i)[0]
            i += 2
        if op == 0x07:
            out.append(("stop",))
        elif op == 0x8C:
            label = body[i:i + ln].split(b"\0")[0].decode("latin-1")
        elif op == 0x06 and label:
            out.append(("goto", label))
            label = None
        elif op == 0x8E:  # DefineFunction2: skip its code
            j = body.index(b"\0", i) + 1
            nparams = struct.unpack_from("<H", body, j)[0]
            j += 5
            for _ in range(nparams):
                j = body.index(b"\0", j + 1) + 1
            i = j + 2 + struct.unpack_from("<H", body, j)[0]
            continue
        elif op == 0x9B:  # DefineFunction
            j = body.index(b"\0", i) + 1
            nparams = struct.unpack_from("<H", body, j)[0]
            j += 2
            for _ in range(nparams):
                j = body.index(b"\0", j) + 1
            i = j + 2 + struct.unpack_from("<H", body, j)[0]
            continue
        i += ln
    return out


def parse_morphs(tags):
    morphs = {}
    for code, body in tags:
        if code != 46:
            continue
        b = gfx_io.Bits(body)
        cid = b.u16()
        sb = b.rect(); eb = b.rect(); b.u32()
        n = b.u8()
        for _ in range(n):
            t = b.u8()
            if t in (0x40, 0x41, 0x42, 0x43):
                bid = b.u16(); m1 = b.matrix(); b.matrix()
                morphs[cid] = {"bitmap": bid, "matrix": m1, "start": sb, "end": eb}
            break
    return morphs


def bake_low_health(tex_dir, out, ex):
    """LowHealthScreenEffect_M adds red scanlines to the scene: LowHealthScanline_CLR
    tiled 80 times down the screen, faded by LowHealthScanlinesOpacityMask_CLR (bright
    at the edges). The demo draws them over the scene, so they are baked into one
    image here; the material's panning noise is left to the demo (a flicker)."""
    try:
        line = Image.open(os.path.join(tex_dir, "LowHealthScanline_CLR.png")).convert("L")
        mask = Image.open(os.path.join(tex_dir, "LowHealthScanlinesOpacityMask_CLR.png")).convert("L")
    except OSError:
        ex.warnings.append("low-health textures not found: no scanlines")
        return
    w, h = 128, 1280
    scan = line.resize((1, 16), Image.BILINEAR)
    col = Image.new("L", (1, h))
    for y in range(h):
        col.putpixel((0, y), scan.getpixel((0, y % 16)))
    col = col.resize((w, h))
    m = mask.resize((w, h), Image.BILINEAR)
    alpha = ImageChops.multiply(col, m)
    white = Image.new("L", (w, h), 255)
    Image.merge("RGBA", (white, white, white, alpha)).save(os.path.join(out, "img", "LowHealthScanlines.png"))
    ex.lines.insert(0, "image LowHealthScanlines img/LowHealthScanlines.png %d %d" % (w, h))
    # The tunnel vision: the scene times (mask + 0.3), so the demo draws black over
    # it with alpha from 1 - mask.
    try:
        tunnel = Image.open(os.path.join(tex_dir, "LowHealthTunnelVisionMask_CLR.png")).convert("L")
    except OSError:
        return
    inv = tunnel.point(lambda v: 255 - v)
    black = Image.new("L", tunnel.size, 0)
    Image.merge("RGBA", (black, black, black, inv)).save(os.path.join(out, "img", "LowHealthVignette.png"))
    ex.lines.insert(0, "image LowHealthVignette img/LowHealthVignette.png %d %d" % tunnel.size)


# -- fonts --------------------------------------------------------------------

def rasterise_font(font, out_dir, slug, chars):
    em = FONT_EM_PX
    ss = 4
    scale = em / 1024.0
    cells = []
    for code in chars:
        if code not in font["glyphs"]:
            continue
        loops, adv = font["glyphs"][code]
        if loops:
            xs = [p[0] for l in loops for p in l]; ys = [p[1] for l in loops for p in l]
            x0, x1, y0, y1 = min(xs), max(xs), min(ys), max(ys)
        else:
            x0 = x1 = y0 = y1 = 0.0
        cells.append((code, loops, adv, (x0, y0, x1, y1)))
    # Pack in rows.
    atlas_w = 1024
    x = y = FONT_PAD
    row_h = 0
    placed = []
    for code, loops, adv, (x0, y0, x1, y1) in cells:
        w = int((x1 - x0) * scale) + 2 * FONT_PAD + 2
        h = int((y1 - y0) * scale) + 2 * FONT_PAD + 2
        if x + w > atlas_w:
            x = FONT_PAD; y += row_h; row_h = 0
        placed.append((code, loops, adv, (x0, y0, x1, y1), x, y, w, h))
        x += w; row_h = max(row_h, h)
    atlas_h = 1
    while atlas_h < y + row_h + FONT_PAD:
        atlas_h *= 2
    atlas = Image.new("L", (atlas_w, atlas_h), 0)
    records = []
    for code, loops, adv, (x0, y0, x1, y1), px, py, w, h in placed:
        if loops:
            big = Image.new("1", (w * ss, h * ss), 0)
            for loop in loops:
                m = Image.new("1", big.size, 0)
                ImageDraw.Draw(m).polygon(
                    [((p[0] - x0) * scale * ss + (FONT_PAD + 1) * ss,
                      (p[1] - y0) * scale * ss + (FONT_PAD + 1) * ss) for p in loop], fill=1)
                big = ImageChops.logical_xor(big, m)
            small = big.convert("L").resize((w, h), Image.BOX)
            atlas.paste(small, (px, py))
        # Quad covering the cell, in ems relative to the pen (baseline), and its uv.
        gx0 = x0 / 1024.0 - (FONT_PAD + 1) / em
        gy0 = y0 / 1024.0 - (FONT_PAD + 1) / em
        gx1 = gx0 + w / em
        gy1 = gy0 + h / em
        records.append((code, px / atlas_w, py / atlas_h, (px + w) / atlas_w, (py + h) / atlas_h,
                        gx0, gy0, gx1, gy1, adv / 1024.0))
    rgba = Image.merge("RGBA", (Image.new("L", atlas.size, 255),) * 3 + (atlas,))
    file = "font_%s.png" % slug
    rgba.save(os.path.join(out_dir, file))
    return file, atlas_w, atlas_h, records


def shared_symbols(cooked, tex_dir, out, ex):
    """SHARED_SYMBOLS from the shared icons movie: each a one-frame clip of its
    shapes, rasterised, under ids of their own (SHARED_ID_BASE plus the shared
    movie's)."""
    pkg, full_name = SHARED_MOVIE
    raw = swf_movie(os.path.join(cooked, pkg), full_name)
    tmp = os.path.join(out, "SharedIcons.gfx")
    open(tmp, "wb").write(raw)
    movie = gfx_io.Movie(tmp)
    os.remove(tmp)
    shared = Exporter(movie, tex_dir, out)
    for symbol in SHARED_SYMBOLS:
        sid = movie.exports.get(symbol)
        c = movie.chars.get(sid) if sid is not None else None
        if c is None or c["kind"] != "sprite" or not c["frames"]:
            ex.warnings.append("shared symbol %s not found" % symbol)
            continue
        new = SHARED_ID_BASE + sid
        ex.lines.append("sprite %d 1 %s" % (new, symbol))
        for d, pl in sorted(c["frames"][0].items()):
            shape = movie.chars.get(pl.get("id"))
            if shape is None or shape["kind"] != "shape":
                ex.warnings.append("shared symbol %s: depth %d is not a shape" % (symbol, d))
                continue
            child = SHARED_ID_BASE + shape["id"]
            shared.raster_shape(shape, "Shared" + symbol[3].upper() + symbol[4:], child)
            ex.lines.append("key %d 0 %d %s" % (new, d, shared.place_key(dict(pl, id=child))))
        ex.lines.append("symbol %s %d" % (symbol, new))
    ex.lines = [l for l in shared.lines if l.startswith("image ")] + ex.lines + \
        [l for l in shared.lines if not l.startswith("image ")]


def graft_imports(cooked, out, movie):
    """IMPORTED_ART and IMPORTED_IMAGES into `movie` under the HUD's ids for them:
    each symbol's sprite and its bitmap shapes (their images among the HUD's), each
    image as one of the HUD's own."""
    pkg, full_name = SHARED_ASSETS_MOVIE
    raw = swf_movie(os.path.join(cooked, pkg), full_name)
    tmp = os.path.join(out, "SharedAssets.gfx")
    open(tmp, "wb").write(raw)
    shared = gfx_io.Movie(tmp)
    os.remove(tmp)
    import copy
    for hud_id, symbol in IMPORTED_ART:
        sid = shared.exports.get(symbol)
        c = shared.chars.get(sid) if sid is not None else None
        if c is None or c["kind"] != "sprite":
            print("  ! imported art %s not found" % symbol)
            continue
        frames = []
        for f in c["frames"]:
            frame = {}
            for d, pl in f.items():
                shape = shared.chars.get(pl.get("id"))
                if shape is None or shape["kind"] != "shape":
                    continue
                new = copy.deepcopy(shape)
                new["id"] = GRAFT_ID_BASE + shape["id"]
                for fills, _lines in new["styles"]:
                    for fill in fills:
                        b = fill.get("bitmap")
                        if b is not None and b != 65535 and b in shared.images:
                            fill["bitmap"] = GRAFT_ID_BASE + b
                            movie.images[GRAFT_ID_BASE + b] = dict(shared.images[b], id=GRAFT_ID_BASE + b)
                movie.chars[new["id"]] = new
                frame[d] = dict(pl, id=new["id"])
            frames.append(frame)
        movie.chars[hud_id] = dict(copy.deepcopy(c), id=hud_id, frames=frames, tags=[])
    tmp = os.path.join(out, "SharedIcons.gfx")
    open(tmp, "wb").write(swf_movie(os.path.join(cooked, SHARED_MOVIE[0]), SHARED_MOVIE[1]))
    icons = gfx_io.Movie(tmp)
    os.remove(tmp)
    for hud_id, name in IMPORTED_IMAGES:
        iid = icons.exports.get(name)
        info = icons.images.get(iid) if iid is not None else None
        if info is None:
            print("  ! imported image %s not found" % name)
            continue
        movie.images[hud_id] = dict(info, id=hud_id)
    pkg, full_name = BUTTON_ICONS_MOVIE
    tmp = os.path.join(out, "ButtonIcons.gfx")
    open(tmp, "wb").write(swf_movie(os.path.join(cooked, pkg), full_name))
    buttons = gfx_io.Movie(tmp)
    os.remove(tmp)
    for hud_id, name in sorted(imported_fonts(movie.tags, BUTTON_ICONS_URL).items()):
        iid = buttons.exports.get(name)
        info = buttons.images.get(iid) if iid is not None else None
        if info is None:
            print("  ! imported button %s not found" % name)
            continue
        movie.images[hud_id] = dict(info, id=hud_id)


def imported_fonts(tags, url=None):
    """A movie's imported fonts (ImportAssets2: its fonts library's) by id: their
    names ($NormalFont and the like). With `url`, what it imports from that movie
    alone (whatever they are)."""
    out = {}
    for code, body in tags:
        if code != 71:
            continue
        end = body.index(b"\0")
        if url is not None and body[:end].decode("latin-1") != url:
            continue
        pos = end + 3  # the url, then ImportAssets2's two reserved bytes
        count = struct.unpack_from("<H", body, pos)[0]
        pos += 2
        for _ in range(count):
            cid = struct.unpack_from("<H", body, pos)[0]
            end = body.index(b"\0", pos + 2)
            out[cid] = body[pos + 2:end].decode("latin-1")
            pos = end + 1
    return out


def graft_respawn(cooked, out, movie):
    """RESPAWN_SYMBOLS into `movie`: each sprite and everything under it numbered
    from RESPAWN_ID_BASE up (its scripts kept, for their text fields' autoSize), its
    texts as HTML in the faces their fonts are imported as, its image as one of the
    HUD's; each exported under its symbol's name."""
    import copy
    pkg, full_name = RESPAWN_MOVIE
    raw = swf_movie(os.path.join(cooked, pkg), full_name)
    tmp = os.path.join(out, "Respawn.gfx")
    open(tmp, "wb").write(raw)
    src = gfx_io.Movie(tmp)
    os.remove(tmp)
    faces = imported_fonts(src.tags)
    aligns = {0: "left", 1: "right", 2: "center", 3: "justify"}

    def graft(cid):
        new = RESPAWN_ID_BASE + cid
        if new in movie.chars or new in movie.images:
            return new
        if cid in src.images:
            movie.images[new] = dict(src.images[cid], id=new)
            return new
        c = src.chars.get(cid)
        if c is None:
            print("  ! respawn screen: character %d not found" % cid)
            return new
        c = copy.deepcopy(c)
        c["id"] = new
        movie.chars[new] = c
        if c["kind"] == "sprite":
            for frame in c["frames"]:
                for pl in frame.values():
                    if pl.get("id") is not None:
                        pl["id"] = graft(pl["id"])
        elif c["kind"] == "shape":
            for fills, _lines in c["styles"]:
                for fill in fills:
                    b = fill.get("bitmap")
                    if b is not None and b != 65535:
                        fill["bitmap"] = graft(b)
        elif c["kind"] == "text":
            html = c.get("text") or ""
            plain = re.sub(r"<[^>]+>", "", html) if c.get("html") else html
            r, g, b, _a = c.get("color", (255, 255, 255, 255))
            c["text"] = '<p align="%s"><font face="%s" size="%g" color="#%02x%02x%02x">%s</font></p>' % (
                aligns.get(c.get("align", 0), "left"), faces.get(c.get("font"), "$NormalFont"),
                c.get("height", 12), r, g, b, plain)
            c["html"] = True
        return new

    for name, cid in RESPAWN_SYMBOLS:
        movie.exports[name] = graft(cid)


def main(cooked, tex_dir, assets):
    out = os.path.join(assets, "hud")
    os.makedirs(out, exist_ok=True)
    print("reading the HUD movie from TransGame.xxx")
    raw = swf_movie(os.path.join(cooked, "TransGame.xxx"), "UI_GFxHud_p.Hud_GFX")
    tmp = os.path.join(out, "Hud_GFX.gfx")
    open(tmp, "wb").write(raw)
    movie = gfx_io.Movie(tmp)
    os.remove(tmp)
    movie.morphs = parse_morphs(movie.tags)
    graft_imports(cooked, out, movie)
    graft_respawn(cooked, out, movie)

    ex = Exporter(movie, tex_dir, out, cooked)
    ex.lines.append("root %d" % HUD_MC)
    ex.define(HUD_MC)
    for name in SYMBOLS + [name for name, _ in RESPAWN_SYMBOLS]:
        cid = movie.exports.get(name)
        if cid is None:
            ex.warnings.append("symbol %s not found" % name)
            continue
        ex.define(cid)
        ex.lines.append("symbol %s %d" % (name, cid))
    shared_symbols(cooked, tex_dir, out, ex)
    ex.copy_images()
    bake_low_health(tex_dir, out, ex)

    print("reading the fonts from Startup_int.xxx")
    raw = swf_movie(os.path.join(cooked, "Startup_int.xxx"), "UI_GFxFonts_p.Fonts_EFIGS_GFX")
    tmp = os.path.join(out, "Fonts.gfx")
    open(tmp, "wb").write(raw)
    _, _, ftags = gfx_io.load(tmp)
    os.remove(tmp)
    fonts = {}
    for code, body in ftags:
        if code == 75:
            f = gfx_io.parse_font3(body)
            fonts[f["name"]] = f
    # ASCII, and the dash the scope's range finder shows with nothing in range.
    chars = list(range(32, 127)) + [0x2014]
    for face in sorted(ex.faces | {"$NormalFont", "$NumberFontWide"}):
        fname = FACES.get(face)
        if fname not in fonts:
            ex.warnings.append("font %s (%s) not found" % (face, fname))
            continue
        f = fonts[fname]
        slug = fname.replace(" ", "")
        file, w, h, recs = rasterise_font(f, out, slug, chars)
        ex.lines.append("font %s %s %d %d %d %.5f %.5f %.5f" % (
            face, file, w, h, FONT_EM_PX, f["ascent"] / 1024, f["descent"] / 1024, f["leading"] / 1024))
        for r in recs:
            ex.lines.append("glyph %s %d %.5f %.5f %.5f %.5f %.5f %.5f %.5f %.5f %.5f" % ((face,) + r))

    with open(os.path.join(out, "hud.txt"), "w") as fh:
        fh.write("# Fall of Cybertron HUD (UI_GFxHud_p.Hud_GFX), exported by tools/build_hud.py\n")
        fh.write("\n".join(ex.lines) + "\n")
    for w in sorted(set(ex.warnings)):
        print("  !", w)
    print("wrote %s (%d records, %d images)" % (os.path.join(out, "hud.txt"), len(ex.lines), len(set(ex.images.values()))))


import gfx_render  # noqa: E402  (curve flattening for line shapes)

if __name__ == "__main__":
    if len(sys.argv) != 4:
        sys.exit(__doc__)
    main(*sys.argv[1:])
