"""The demo's Windows icon (icon/foc_esc.ico) from a picture: a PNG, ideally square
with a transparent background (a non-square one is centred on a transparent square).
build.rs puts the .ico into foc_esc.exe, which Windows then shows for the program
in File Explorer, on the taskbar and in the window's corner (src/appicon.rs).

usage: python make_icon.py <picture.png> [<out.ico>]     (default: ..\\icon\\foc_esc.ico)

The .ico holds the picture at each size Windows asks for (16 to 256 pixels across,
for its various views and display scalings), each scaled down from the full
picture: as 32-bit bitmaps up to 128, and as a PNG at 256, as Windows' own icons
are kept.
"""
import io
import os
import struct
import sys

from PIL import Image

SIZES = (16, 20, 24, 32, 40, 48, 64, 96, 128, 256)
# Sizes from this one up are kept as PNG (smaller ones as bitmaps).
PNG_FROM = 256


def square(im):
    """The picture centred on a transparent square, if it is not one already."""
    im = im.convert("RGBA")
    side = max(im.size)
    if im.width == im.height:
        return im
    out = Image.new("RGBA", (side, side), (0, 0, 0, 0))
    out.paste(im, ((side - im.width) // 2, (side - im.height) // 2))
    return out


def scaled(im, size):
    """The picture at size x size, its colours scaled with their opacity (so the
    see-through edges do not darken or fringe)."""
    return im.convert("RGBa").resize((size, size), Image.LANCZOS).convert("RGBA")


def bitmap(im):
    """An icon image as Windows keeps a bitmap one: a BITMAPINFOHEADER (its height
    doubled, for the mask), the colours bottom row first as BGRA, then a 1-bit
    mask (set where the picture is fully see-through), rows padded to 4 bytes."""
    w, h = im.size
    px = im.load()
    colour = bytearray()
    mask = bytearray()
    row_bytes = ((w + 31) // 32) * 4
    for y in range(h - 1, -1, -1):
        bits = bytearray(row_bytes)
        for x in range(w):
            r, g, b, a = px[x, y]
            colour += bytes((b, g, r, a))
            if a == 0:
                bits[x // 8] |= 0x80 >> (x % 8)
        mask += bits
    header = struct.pack("<IiiHHIIiiII", 40, w, h * 2, 1, 32, 0, len(colour) + len(mask), 0, 0, 0, 0)
    return header + bytes(colour) + bytes(mask)


def png(im):
    buf = io.BytesIO()
    im.save(buf, "PNG", optimize=True)
    return buf.getvalue()


def make_ico(picture):
    im = square(Image.open(picture))
    images = [(size, png(scaled(im, size)) if size >= PNG_FROM else bitmap(scaled(im, size))) for size in SIZES]
    out = struct.pack("<HHH", 0, 1, len(images))
    offset = 6 + 16 * len(images)
    for size, data in images:
        # (A width or height of 256 is written as 0.)
        out += struct.pack("<BBBBHHII", size % 256, size % 256, 0, 0, 1, 32, len(data), offset)
        offset += len(data)
    return out + b"".join(data for _size, data in images)


def main():
    if len(sys.argv) not in (2, 3):
        sys.exit(__doc__)
    here = os.path.dirname(os.path.abspath(__file__))
    out = sys.argv[2] if len(sys.argv) == 3 else os.path.join(here, "..", "icon", "foc_esc.ico")
    data = make_ico(sys.argv[1])
    os.makedirs(os.path.dirname(os.path.abspath(out)), exist_ok=True)
    with open(out, "wb") as f:
        f.write(data)
    print(f"{out}: {len(SIZES)} sizes, {len(data)} bytes")


if __name__ == "__main__":
    main()
