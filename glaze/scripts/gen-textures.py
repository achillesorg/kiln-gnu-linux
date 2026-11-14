#!/usr/bin/env python3
"""Generate Glaze's tileable paper textures (deterministic, stdlib only).

Writes assets/textures/<set>-<tone>.png for set in grain, fibre, laid and
tone in light (warm-brown specks for light paper) and dark (cream specks for
dark paper). Tiles are 128x128 RGBA, seamless (all noise wraps around the
tile edges), and their alpha never exceeds MAX_ALPHA so text contrast on a
textured surface stays within the WCAG gate (checked by
crates/glaze-design/src/texture.rs, which carries the same constant).

The shell scales the alpha by `appearance.texture_intensity` at runtime and
caches the result; set `none` (or reduced effects) draws plain fills.

Usage: scripts/gen-textures.py [OUT_DIR]   (default: assets/textures)
Re-running produces byte-identical files.
"""
import math
import os
import struct
import sys
import zlib

SIZE = 128
MAX_ALPHA = 0.12  # keep in sync with glaze_design::texture::MAX_TEXEL_ALPHA
TONES = {
    # rgb of the texel colour per scheme
    "light": (0x5A, 0x44, 0x2C),
    "dark": (0xF3, 0xEC, 0xDD),
}


def rng(seed):
    """Small deterministic xorshift32 generator returning floats in [0,1)."""
    state = [seed & 0xFFFFFFFF or 1]

    def nxt():
        x = state[0]
        x ^= (x << 13) & 0xFFFFFFFF
        x ^= x >> 17
        x ^= (x << 5) & 0xFFFFFFFF
        state[0] = x
        return x / 4294967296.0

    return nxt


def value_noise(seed, cells):
    """Periodic value noise (wraps at SIZE) with `cells` lattice cells."""
    r = rng(seed)
    lattice = [[r() for _ in range(cells)] for _ in range(cells)]

    def smooth(t):
        return t * t * (3 - 2 * t)

    out = [[0.0] * SIZE for _ in range(SIZE)]
    step = SIZE / cells
    for y in range(SIZE):
        gy = y / step
        y0 = int(gy) % cells
        y1 = (y0 + 1) % cells
        ty = smooth(gy - int(gy))
        for x in range(SIZE):
            gx = x / step
            x0 = int(gx) % cells
            x1 = (x0 + 1) % cells
            tx = smooth(gx - int(gx))
            a = lattice[y0][x0] + (lattice[y0][x1] - lattice[y0][x0]) * tx
            b = lattice[y1][x0] + (lattice[y1][x1] - lattice[y1][x0]) * tx
            out[y][x] = a + (b - a) * ty
    return out


def grain():
    r = rng(0xGLAZE01)
    low = value_noise(7, 8)
    a = [[0.0] * SIZE for _ in range(SIZE)]
    for y in range(SIZE):
        for x in range(SIZE):
            speck = r()
            v = 0.35 * low[y][x] + (0.65 * speck if speck > 0.55 else 0.0)
            a[y][x] = v
    # A few slightly larger flecks (2x2), wrapped.
    for _ in range(90):
        cx, cy = int(r() * SIZE), int(r() * SIZE)
        s = 0.6 + 0.4 * r()
        for dy in (0, 1):
            for dx in (0, 1):
                a[(cy + dy) % SIZE][(cx + dx) % SIZE] = max(a[(cy + dy) % SIZE][(cx + dx) % SIZE], s)
    return a


def fibre():
    r = rng(0xGLAZE02)
    base = value_noise(11, 4)
    a = [[0.25 * base[y][x] for x in range(SIZE)] for y in range(SIZE)]
    for _ in range(140):
        x, y = r() * SIZE, r() * SIZE
        ang = r() * math.pi
        length = 6 + r() * 18
        bend = (r() - 0.5) * 0.15
        strength = 0.45 + 0.55 * r()
        steps = int(length * 2)
        for i in range(steps):
            ang += bend / steps * 4
            x += math.cos(ang) * 0.5
            y += math.sin(ang) * 0.5
            px, py = int(x) % SIZE, int(y) % SIZE
            a[py][px] = max(a[py][px], strength)
    return a


def laid():
    r = rng(0xGLAZE03)
    wob = value_noise(5, 8)
    a = [[0.0] * SIZE for _ in range(SIZE)]
    for y in range(SIZE):
        line = 0.55 if y % 4 == 0 else 0.0
        for x in range(SIZE):
            chain = 0.5 if x % 32 == 0 else 0.0
            a[y][x] = max(line * (0.6 + 0.4 * wob[y][x]), chain * (0.5 + 0.5 * wob[x][y]))
            a[y][x] = max(a[y][x], 0.2 * r() if r() > 0.7 else 0.0)
    return a


def png_rgba(path, alpha, rgb):
    raw = bytearray()
    for row in alpha:
        raw.append(0)
        for v in row:
            al = int(round(max(0.0, min(1.0, v)) * MAX_ALPHA * 255))
            raw.extend((rgb[0], rgb[1], rgb[2], al))

    def chunk(kind, body):
        c = struct.pack(">I", len(body)) + kind + body
        return c + struct.pack(">I", zlib.crc32(kind + body) & 0xFFFFFFFF)

    data = b"\x89PNG\r\n\x1a\n"
    data += chunk(b"IHDR", struct.pack(">IIBBBBB", SIZE, SIZE, 8, 6, 0, 0, 0))
    data += chunk(b"IDAT", zlib.compress(bytes(raw), 9))
    data += chunk(b"IEND", b"")
    with open(path, "wb") as f:
        f.write(data)
    peak = max(max(int(round(max(0.0, min(1.0, v)) * MAX_ALPHA * 255)) for v in row) for row in alpha)
    assert peak <= int(MAX_ALPHA * 255) + 1, peak
    return peak


def main():
    root = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
    out = sys.argv[1] if len(sys.argv) > 1 else os.path.join(root, "assets", "textures")
    os.makedirs(out, exist_ok=True)
    for name, fn in (("grain", grain), ("fibre", fibre), ("laid", laid)):
        alpha = fn()
        for tone, rgb in TONES.items():
            path = os.path.join(out, f"{name}-{tone}.png")
            peak = png_rgba(path, alpha, rgb)
            print(f"{path} ({SIZE}x{SIZE}, peak alpha {peak}/255)")


if __name__ == "__main__":
    main()
