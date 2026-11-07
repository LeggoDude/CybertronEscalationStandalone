"""Build the images of the demo's pre-match scoreboard (src/pregame.rs) and its
scoreboard in a match (src/scoreboard.rs) from the game.

usage: python build_pregame.py <CookedPC folder> <foc_esc/assets folder>

Once a player has chosen his character, Escalation shows the map through its level
camera with the countdown to the match over it: the multiplayer UI controller's
GameStartUI, the movie UI_GFxPreGameCountdown_p.PreGameCountdown_GFX, with the
players' list (UI_GFxPlayerList_p.PlayerList_GFX, loaded into it in its 'InGame'
view). Both movies are in mp_shared.xxx. Their bars are vector shapes, drawn here
into images at twice the movie's size (in white: the demo tints them); the rest
comes from their own images:

  assets/pregame/img/bar_top.png      the band under the countdown (PreGame shape 13,
                                      414 x 25 pixels, with its notches and its
                                      slanted end)
  assets/pregame/img/bar_wide.png     the long band over it (shape 26, 300 x 25)
  assets/pregame/img/hint_bar.png     the band over the button hints (shape 5,
                                      400 x 14, notched)
  assets/pregame/img/shade.png        the gradient the movie darkens the view with
                                      (shape 3's, top to bottom, 4 x 256)
  assets/pregame/img/select_glow.png  the focused row's highlight (the player list's
                                      PlayerList_GFX_I1C, 256 x 32, as it is)

The scoreboard Tab shows in a match is the movie UI_GFxInGameStats_p.InGameStats_GFX
(TnHUD's ScoreboardMovie, in TransGame.xxx) with the players' list in it. Its
gradient over the view (its shape 1) has the countdown's stops, so it is shade.png
too; the list shows each player's class as the lobby's view of it does, with the
shared icons' class symbols (UI_GFxShared_p.SharedIcons_GFX, in mp_shared.xxx):

  assets/pregame/img/specialty_<class>.png  the classes' symbols (mc_specialtyIconScout,
                                      Leader, Soldier and Scientist: white shapes, at
                                      four times the movie's size)

The mode icon and the connection bars are the lobby's (tools/build_menu.py:
assets/menu/img/lobby_mode_icon.png and lobby_ping_*.png).
"""
import os
import struct
import sys

import numpy as np
from PIL import Image, ImageChops, ImageDraw

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
sys.path.insert(0, os.path.join(HERE, "ue3"))
import gfx_io  # noqa: E402
import gfx_render  # noqa: E402
from layout import props_any  # noqa: E402
from ue3pkg import Package  # noqa: E402

PACKAGE = "mp_shared.xxx"
MOVIE = "UI_GFxPreGameCountdown_p.PreGameCountdown_GFX"
# The shapes drawn as images: (file, shape id).
SHAPES = [("bar_top.png", 13), ("bar_wide.png", 26), ("hint_bar.png", 5)]
SHADE_SHAPE = 3
GLOW_TEXTURE = "UI_GFxPlayerList_p.PlayerList_GFX_I1C"
SCALE = 2
# The classes' symbols in the scoreboard: (file, shared icons' symbol).
SHARED_MOVIE = "UI_GFxShared_p.SharedIcons_GFX"
SPECIALTY_ICONS = [("specialty_scout.png", "mc_specialtyIconScout"),
                   ("specialty_leader.png", "mc_specialtyIconLeader"),
                   ("specialty_soldier.png", "mc_specialtyIconSoldier"),
                   ("specialty_scientist.png", "mc_specialtyIconScientist")]
ICON_SCALE = 4


def swf_movie(pkg, full_name):
    for i, e in enumerate(pkg.exports):
        if pkg.full_name(i + 1) == full_name:
            pr, _ = props_any(pkg, e, (4, 0, 8, 12))
            d = pr["RawData"][1]
            n = struct.unpack_from("<i", d)[0]
            return d[4:4 + n]
    raise SystemExit("%s not found" % full_name)


def shape_image(movie, sid, path, ss=4, scale=SCALE):
    """A shape's filled area (its loops, even-odd) in white, over its bounds, at
    `scale` pixels to the movie's."""
    c = movie.chars[sid]
    x0, x1, y0, y1 = c["bounds"]
    w, h = int(round((x1 - x0) * scale)), int(round((y1 - y0) * scale))
    k = scale * ss
    mask = Image.new("1", (w * ss, h * ss), 0)
    for _, f0, f1, _, segs in c["paths"]:
        if not (f0 or f1):
            continue
        for loop in gfx_render._loops(segs):
            if len(loop) > 2:
                m = Image.new("1", mask.size, 0)
                ImageDraw.Draw(m).polygon([((x - x0) * k, (y - y0) * k) for x, y in loop], fill=1)
                mask = ImageChops.logical_xor(mask, m)
    alpha = mask.convert("L").resize((w, h), Image.LANCZOS)
    im = Image.new("RGBA", (w, h), (255, 255, 255, 0))
    im.putalpha(alpha)
    im.save(path)
    print("wrote", os.path.basename(path), "(%d x %d, shape %d: %g x %g)" % (w, h, sid, x1 - x0, y1 - y0))


def shade_image(movie, path):
    """The overlay's linear gradient (shape 3: its gradient runs down the shape),
    stop to stop in sRGB as Flash blends them."""
    fill = movie.chars[SHADE_SHAPE]["styles"][0][0][0]
    stops = fill["stops"]
    n = 256
    t = (np.arange(n) + 0.5) / n * 255.0
    rgb = np.zeros((n, 3))
    for ch in range(3):
        rgb[:, ch] = np.interp(t, [s[0] for s in stops], [s[1][ch] for s in stops])
    col = np.dstack([rgb.round().clip(0, 255).astype(np.uint8)[:, None, :].repeat(4, axis=1)])
    Image.fromarray(col.reshape(n, 4, 3), "RGB").save(path)
    print("wrote", os.path.basename(path), "(stops %s)" % stops)


def main():
    if len(sys.argv) != 3:
        print(__doc__)
        sys.exit(1)
    cooked, assets = sys.argv[1], sys.argv[2]
    out = os.path.join(assets, "pregame", "img")
    os.makedirs(out, exist_ok=True)
    pkg = Package(os.path.join(cooked, PACKAGE))
    tmp = os.path.join(out, "_PreGameCountdown.gfx")
    with open(tmp, "wb") as f:
        f.write(swf_movie(pkg, MOVIE))
    movie = gfx_io.Movie(tmp)
    os.remove(tmp)
    for name, sid in SHAPES:
        shape_image(movie, sid, os.path.join(out, name))
    shade_image(movie, os.path.join(out, "shade.png"))
    import texture
    for i, e in enumerate(pkg.exports):
        if pkg.full_name(i + 1) == GLOW_TEXTURE and pkg.class_name(e) == "Texture2D":
            rgba, _ = texture.read_texture(pkg, i + 1, cooked)
            Image.fromarray(rgba).save(os.path.join(out, "select_glow.png"))
            print("wrote select_glow.png (%d x %d)" % (rgba.shape[1], rgba.shape[0]))
            break
    else:
        raise SystemExit(GLOW_TEXTURE + " not found")
    # The scoreboard in a match: the classes' symbols.
    with open(tmp, "wb") as f:
        f.write(swf_movie(pkg, SHARED_MOVIE))
    movie = gfx_io.Movie(tmp)
    os.remove(tmp)
    for file, symbol in SPECIALTY_ICONS:
        sprite = movie.chars[movie.exports[symbol]]
        sid = next(pl["id"] for pl in sprite["frames"][0].values())
        shape_image(movie, sid, os.path.join(out, file), scale=ICON_SCALE)


if __name__ == "__main__":
    main()
