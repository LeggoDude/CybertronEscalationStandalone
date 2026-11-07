"""KTX2 textures for the demo's maps, block-compressed as the game's own are, with
full mip chains (numpy only).

The game keeps its textures DXT-compressed (BC1/BC3; the texture sets' normals are
two DXT5 alpha channels) with mip maps. umodel exports the top mip as a PNG; this
module turns such pictures back into GPU textures Bevy loads as they are (its
"ktx2" feature, on by default for 3D):

  * mip maps made with a 2 x 2 box filter (in linear light for colour);
  * BC1 for colour and other RGB data, BC3 when there is alpha, BC5 (two BC4
    channels) for normal maps, which Bevy reads as two-component normals;
  * the encoders put each 4 x 4 block's end points on the block's extreme colours
    along its principal axis (BC1) or on its lowest and highest values (BC4), so
    pictures decoded from DXT data come back nearly bit for bit.

Bevy takes sRGB or linear from how the glTF uses a texture, so the format's
variant here only matters to other readers.

    encode(pixels, kind) -> KTX2 bytes   (kind: "color", "data" or "normal")
    decode(ktx2_bytes) -> RGBA uint8 array of the top mip (for previews)
"""
import struct
import numpy as np

KTX2_ID = b"\xabKTX 20\xbb\r\n\x1a\n"

# VkFormat numbers, with block bytes and the Khronos Data Format colour model.
FORMATS = {
    # name: (vkFormat linear, vkFormat sRGB, bytes per 4x4 block, colour model, samples)
    # samples: (bit offset, bit length, channel id)
    "bc1": (131, 132, 8, 128, [(0, 64, 0)]),
    "bc3": (137, 138, 16, 130, [(0, 64, 15), (64, 64, 0)]),
    "bc4": (139, 139, 8, 131, [(0, 64, 0)]),
    "bc5": (141, 141, 16, 132, [(0, 64, 0), (64, 64, 1)]),
}
VK_TO_FORMAT = {vk: (name, vk == spec[1] and spec[0] != spec[1]) for name, spec in FORMATS.items() for vk in spec[:2]}


# ---------------------------------------------------------------------------
# Mip maps
# ---------------------------------------------------------------------------

def srgb_to_linear(x):
    x = np.asarray(x, np.float32) / 255.0
    return np.where(x <= 0.04045, x / 12.92, ((x + 0.055) / 1.055) ** 2.4)


def linear_to_srgb(x):
    x = np.clip(x, 0.0, 1.0)
    return np.where(x <= 0.0031308, x * 12.92, 1.055 * np.power(x, 1 / 2.4) - 0.055) * 255.0


def _half(a):
    """One mip down: each pixel the average of the 2 x 2 (or 2 x 1) it covers."""
    h, w = a.shape[:2]
    if h > 1:
        a = (a[0:h - 1:2] + a[1:h:2]) * 0.5
    if w > 1:
        a = (a[:, 0:w - 1:2] + a[:, 1:w:2]) * 0.5
    return a


def mip_chain(img, kind):
    """The picture and every smaller mip down to 1 x 1, as uint8 arrays. `kind`:
    "color" (sRGB RGB(A): RGB averaged in linear light, alpha as it is), "data"
    (averaged as stored) or "normal" (two channels holding x and y of unit vectors:
    averaged as vectors, then made unit length again)."""
    img = np.asarray(img)
    if img.ndim == 2:
        img = img[..., None]
    if kind == "color":
        work = img.astype(np.float32)
        work[..., :3] = srgb_to_linear(img[..., :3])
    elif kind == "normal":
        xy = img[..., :2].astype(np.float32) / 127.5 - 1.0
        z = np.sqrt(np.clip(1.0 - (xy ** 2).sum(-1, keepdims=True), 0.0, 1.0))
        work = np.concatenate([xy, z], -1)
    else:
        work = img.astype(np.float32)

    def to_u8(a):
        if kind == "color":
            out = a.copy()
            out[..., :3] = linear_to_srgb(a[..., :3])
            return np.clip(np.round(out), 0, 255).astype(np.uint8)
        if kind == "normal":
            n = a / np.maximum(np.linalg.norm(a, axis=-1, keepdims=True), 1e-6)
            return np.clip(np.round((n[..., :2] + 1.0) * 127.5), 0, 255).astype(np.uint8)
        return np.clip(np.round(a), 0, 255).astype(np.uint8)

    levels = [img if kind != "normal" else img[..., :2].astype(np.uint8)]
    while work.shape[0] > 1 or work.shape[1] > 1:
        work = _half(work)
        levels.append(to_u8(work))
    return levels


def _pad4(img):
    """Edge pixels repeated out to whole 4 x 4 blocks."""
    h, w = img.shape[:2]
    ph, pw = (-h) % 4, (-w) % 4
    if ph or pw:
        pad = [(0, ph), (0, pw)] + [(0, 0)] * (img.ndim - 2)
        img = np.pad(img, pad, mode="edge")
    return img


def _blocks(img):
    """(blocks, 16, channels): 4 x 4 blocks in row order, their pixels in row order."""
    img = _pad4(img)
    if img.ndim == 2:
        img = img[..., None]
    h, w, c = img.shape
    return img.reshape(h // 4, 4, w // 4, 4, c).transpose(0, 2, 1, 3, 4).reshape(-1, 16, c)


# ---------------------------------------------------------------------------
# Block encoders
# ---------------------------------------------------------------------------

def _pack565(c):
    c = np.asarray(c, np.float32)
    r = np.clip(np.round(c[..., 0] * 31 / 255), 0, 31).astype(np.uint32)
    g = np.clip(np.round(c[..., 1] * 63 / 255), 0, 63).astype(np.uint32)
    b = np.clip(np.round(c[..., 2] * 31 / 255), 0, 31).astype(np.uint32)
    return (r << 11) | (g << 5) | b


def _unpack565(v):
    v = np.asarray(v, np.uint32)
    r, g, b = (v >> 11) & 31, (v >> 5) & 63, v & 31
    return np.stack([(r << 3) | (r >> 2), (g << 2) | (g >> 4), (b << 3) | (b >> 2)], -1).astype(np.float32)


def _bc1_blocks(px, four_color=False):
    """BC1 colour blocks (N x 8 bytes) for (N, 16, 3) pixels. `four_color`: for BC3,
    whose colour block always has four colours."""
    px = px.astype(np.float32)
    n = len(px)
    mean = px.mean(1, keepdims=True)
    d = px - mean
    cov = np.einsum("nki,nkj->nij", d, d)
    # Principal axis by power iteration, from the widest channel.
    v = np.zeros((n, 3), np.float32)
    v[np.arange(n), np.argmax(np.einsum("nii->ni", cov), 1)] = 1.0
    for _ in range(8):
        v = np.einsum("nij,nj->ni", cov, v)
        v /= np.maximum(np.linalg.norm(v, axis=1, keepdims=True), 1e-12)
    t = np.einsum("nki,ni->nk", d, v)
    e0 = px[np.arange(n), t.argmax(1)]
    e1 = px[np.arange(n), t.argmin(1)]
    c0, c1 = _pack565(e0), _pack565(e1)
    # c0 > c1 picks four colours; swap where needed (equal: one colour, index 0).
    swap = c0 < c1
    c0, c1 = np.where(swap, c1, c0), np.where(swap, c0, c1)
    p0, p1 = _unpack565(c0), _unpack565(c1)
    pal = np.stack([p0, p1, (2 * p0 + p1) / 3, (p0 + 2 * p1) / 3], 1)  # (N, 4, 3)
    dist = ((px[:, :, None, :] - pal[:, None, :, :]) ** 2).sum(-1)  # (N, 16, 4)
    idx = dist.argmin(2).astype(np.uint32)
    idx[c0 == c1] = 0
    bits = np.zeros(n, np.uint32)
    for k in range(16):
        bits |= idx[:, k] << np.uint32(2 * k)
    out = np.zeros((n, 8), np.uint8)
    out[:, 0], out[:, 1] = c0 & 0xFF, c0 >> 8
    out[:, 2], out[:, 3] = c1 & 0xFF, c1 >> 8
    for k in range(4):
        out[:, 4 + k] = (bits >> np.uint32(8 * k)) & 0xFF
    return out


def _bc4_blocks(px):
    """BC4 blocks (N x 8 bytes) for (N, 16) values: end points on the block's highest
    and lowest value (eight-value mode), each pixel on the nearest step."""
    px = px.astype(np.int32)
    n = len(px)
    hi, lo = px.max(1), px.min(1)
    pal = np.stack([hi, lo] + [((7 - i) * hi + i * lo) / 7.0 for i in range(1, 7)], 1)
    idx = np.abs(px[:, :, None] - pal[:, None, :]).argmin(2).astype(np.uint64)
    bits = np.zeros(n, np.uint64)
    for k in range(16):
        bits |= idx[:, k] << np.uint64(3 * k)
    out = np.zeros((n, 8), np.uint8)
    out[:, 0], out[:, 1] = hi, lo
    for k in range(6):
        out[:, 2 + k] = ((bits >> np.uint64(8 * k)) & np.uint64(0xFF)).astype(np.uint8)
    return out


def compress(img, fmt):
    """One mip level as BC blocks (bytes). img: uint8 HxWxC (C as the format needs)."""
    b = _blocks(img)
    if fmt == "bc1":
        out = _bc1_blocks(b[..., :3])
    elif fmt == "bc3":
        out = np.concatenate([_bc4_blocks(b[..., 3]), _bc1_blocks(b[..., :3], True)], 1)
    elif fmt == "bc4":
        out = _bc4_blocks(b[..., 0])
    elif fmt == "bc5":
        out = np.concatenate([_bc4_blocks(b[..., 0]), _bc4_blocks(b[..., 1])], 1)
    else:
        raise ValueError(fmt)
    return out.tobytes()


# ---------------------------------------------------------------------------
# The KTX2 container
# ---------------------------------------------------------------------------

def _dfd(fmt, srgb):
    """The basic data format descriptor of a block-compressed format."""
    _, _, block, model, samples = FORMATS[fmt]
    body = struct.pack("<4B4B8B", model, 1, 2 if srgb else 1, 0, 3, 3, 0, 0, block, 0, 0, 0, 0, 0, 0, 0)
    for offset, length, channel in samples:
        qualifier = 0x10 if (srgb and channel == 15) else 0  # alpha stays linear
        body += struct.pack("<HBB4BII", offset, length - 1, channel | qualifier, 0, 0, 0, 0, 0, 0xFFFFFFFF)
    block_size = 8 + len(body)
    blk = struct.pack("<IHH", 0, 2, block_size) + body
    return struct.pack("<I", 4 + len(blk)) + blk


def write(fmt, srgb, width, height, levels, faces=1):
    """A KTX2 file: `levels` are the mips' block data, largest first (a cube map's,
    `faces` 6, each its six faces' one after another: +X, -X, +Y, -Y, +Z, -Z)."""
    vk = FORMATS[fmt][1 if srgb else 0]
    block = FORMATS[fmt][2]
    n = len(levels)
    dfd = _dfd(fmt, srgb)
    dfd_offset = 80 + 24 * n
    pos = dfd_offset + len(dfd)
    # Mip data goes smallest first, each level aligned to the block size.
    offsets = [0] * n
    data = bytearray()
    for i in reversed(range(n)):
        while (pos + len(data)) % block:
            data.append(0)
        offsets[i] = pos + len(data)
        data += levels[i]
    head = KTX2_ID + struct.pack("<9I", vk, 1, width, height, 0, 0, faces, n, 0)
    head += struct.pack("<4I2Q", dfd_offset, len(dfd), 0, 0, 0, 0)
    for i in range(n):
        head += struct.pack("<3Q", offsets[i], len(levels[i]), len(levels[i]))
    return bytes(head + dfd + data)


def encode(img, kind):
    """A KTX2 texture with all its mips. `kind`: "color" (sRGB; BC1, or BC3 with
    alpha), "data" (linear values: roughness and the like; BC1/BC3) or "normal"
    (x and y of tangent-space normals in the first two channels, 0-255 for -1..1:
    BC5)."""
    img = np.asarray(img)
    if img.ndim == 2:
        img = np.repeat(img[..., None], 3, -1)
    h, w = img.shape[:2]
    if kind == "normal":
        fmt, srgb = "bc5", False
    else:
        fmt = "bc3" if img.shape[2] == 4 else "bc1"
        srgb = kind == "color"
    levels = [compress(m, fmt) for m in mip_chain(img, kind)]
    return write(fmt, srgb, w, h, levels)


def is_ktx2(data):
    return data[:12] == KTX2_ID


# ---------------------------------------------------------------------------
# Reading (previews, checks)
# ---------------------------------------------------------------------------

def read(data):
    """(format name, sRGB, width, height, [level bytes, largest first])."""
    assert is_ktx2(data), "not a KTX2 file"
    vk, _ts, w, h, _d, _l, _f, n, sc = struct.unpack_from("<9I", data, 12)
    assert sc == 0, "supercompressed KTX2 isn't read here"
    fmt, srgb = VK_TO_FORMAT[vk]
    levels = []
    for i in range(max(n, 1)):
        off, length, _ = struct.unpack_from("<3Q", data, 80 + 24 * i)
        levels.append(data[off:off + length])
    return fmt, srgb, w, h, levels


def _decode_bc1(b, four_color):
    c0 = b[:, 0].astype(np.uint32) | (b[:, 1].astype(np.uint32) << 8)
    c1 = b[:, 2].astype(np.uint32) | (b[:, 3].astype(np.uint32) << 8)
    p0, p1 = _unpack565(c0), _unpack565(c1)
    four = (c0 > c1) | four_color
    p2 = np.where(four[:, None], (2 * p0 + p1) / 3, (p0 + p1) / 2)
    p3 = np.where(four[:, None], (p0 + 2 * p1) / 3, 0)
    a3 = np.where(four, 255, 0)
    pal = np.stack([p0, p1, p2, p3], 1)
    alpha = np.stack([np.full_like(a3, 255)] * 3 + [a3], 1)
    bits = (b[:, 4].astype(np.uint32) | (b[:, 5].astype(np.uint32) << 8) | (b[:, 6].astype(np.uint32) << 16)
            | (b[:, 7].astype(np.uint32) << 24))
    idx = np.stack([(bits >> np.uint32(2 * k)) & 3 for k in range(16)], 1).astype(int)
    rgb = np.take_along_axis(pal, idx[..., None], 1)
    a = np.take_along_axis(alpha, idx, 1)
    return np.concatenate([rgb, a[..., None]], -1)


def _decode_bc4(b):
    r0, r1 = b[:, 0].astype(np.float32), b[:, 1].astype(np.float32)
    eight = r0 > r1
    pal = [r0, r1]
    for i in range(1, 7):
        six = ((5 - i) * r0 + i * r1) / 5 if i < 5 else np.full_like(r0, 0.0 if i == 5 else 255.0)
        pal.append(np.where(eight, ((7 - i) * r0 + i * r1) / 7, six))
    pal = np.stack(pal, 1)
    bits = np.zeros(len(b), np.uint64)
    for k in range(6):
        bits |= b[:, 2 + k].astype(np.uint64) << np.uint64(8 * k)
    idx = np.stack([((bits >> np.uint64(3 * k)) & np.uint64(7)).astype(int) for k in range(16)], 1)
    return np.take_along_axis(pal, idx, 1)


def decode_level(fmt, w, h, data):
    """One mip level's blocks as an RGBA uint8 array."""
    bw, bh = (w + 3) // 4, (h + 3) // 4
    size = FORMATS[fmt][2]
    b = np.frombuffer(data, np.uint8)[: bw * bh * size].reshape(-1, size)
    if fmt == "bc1":
        px = _decode_bc1(b, False)
    elif fmt == "bc3":
        px = _decode_bc1(b[:, 8:], True)
        px[..., 3] = _decode_bc4(b[:, :8])
    elif fmt == "bc4":
        v = _decode_bc4(b)
        px = np.stack([v, v, v, np.full_like(v, 255)], -1)
    else:  # bc5
        px = np.stack([_decode_bc4(b[:, :8]), _decode_bc4(b[:, 8:]), np.zeros((len(b), 16)), np.full((len(b), 16), 255.0)], -1)
    px = np.clip(np.round(px), 0, 255).astype(np.uint8)
    img = px.reshape(bh, bw, 4, 4, 4).transpose(0, 2, 1, 3, 4).reshape(bh * 4, bw * 4, 4)
    return img[:h, :w]


def decode(data, level=0):
    """A KTX2 texture's mip `level` as an RGBA uint8 array."""
    fmt, _srgb, w, h, levels = read(data)
    return decode_level(fmt, max(w >> level, 1), max(h >> level, 1), levels[level])
