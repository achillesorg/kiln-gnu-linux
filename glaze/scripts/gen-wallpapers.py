#!/usr/bin/env python3
"""Generate Glaze's bundled wallpapers, v2 (original flat cut-paper artwork).

Deterministic and seeded, Python 3 stdlib only. Run from anywhere:

    python3 scripts/gen-wallpapers.py              # SVG sources + PNG renders + manifest + contact sheet
    python3 scripts/gen-wallpapers.py --svg-only   # skip rsvg-convert (no PNGs, no contact sheet)
    python3 scripts/gen-wallpapers.py --no-grain   # PNGs without the baked paper grain
    python3 scripts/gen-wallpapers.py --set kiln --seed 3   # one set, another composition

Writes, for each set in SETS:
    assets/wallpapers/<set>/src/<set>-<scheme>-<orientation>-<W>x<H>.svg   (source, lint-clean)
    assets/wallpapers/<set>/<set>-<scheme>-<orientation>-<W>x<H>.png       (render, grain baked in)
plus assets/wallpapers/manifest.toml (schema 1, resolver-compatible) and
assets/wallpapers/contact-sheet.png.

Style rules (docs/decisions.md): flat hard-edged paper layers with small offsets,
tone-on-tone offset shadows (flat fill at low opacity, never black slabs),
circles / squares / arches / blobs / odd cut-outs, sparse hand-drawn ink lines
(seeded wobble, round caps), lots of calm empty field where desktop icons sit
(top-left on landscape, the middle band on portrait). No filters, no blur, no
colour ramps in the SVG sources (assets/wallpapers/tools/lint-svg.py enforces it).
Paper grain is applied only in the PNG render step from a generated tile.

PNG renders need `rsvg-convert` (librsvg). The legacy v1 sets (paper-hills,
terracotta-waves) are not regenerated here; they stay as shipped (see git
history for the v1 generator) until the user approves the v2 replacements.
"""
import argparse
import base64
import math
import os
import random
import shutil
import struct
import subprocess
import sys
import zlib

ROOT = os.path.normpath(os.path.join(os.path.dirname(os.path.abspath(__file__)), ".."))
OUT = os.path.join(ROOT, "assets", "wallpapers")

# Output sizes. The manifest points at the 1920x1080 / 1080x2340 renders; the
# others are listed in `renders` for installers and future per-size picking.
LANDSCAPE = [(1280, 800), (1920, 1080), (2560, 1440)]
PORTRAIT = [(1080, 2340)]
MANIFEST_SIZE = {"landscape": (1920, 1080), "portrait": (1080, 2340)}

# ---------------------------------------------------------------- palette ---
TERRA = "#D97757"
TERRA_DEEP = "#B95F40"
CREAM = "#FAF9F5"
CREAM_2 = "#F1EBE0"
SAND = "#E6DAC6"
INK = "#2B2926"
CHAR = "#1F1E1C"


def hex2rgb(c):
    c = c.lstrip("#")
    return tuple(int(c[i:i + 2], 16) for i in (0, 2, 4))


def rgb2hex(r):
    return "#%02X%02X%02X" % tuple(max(0, min(255, round(v))) for v in r)


def mix(a, b, t):
    """Flat colour t of the way from a to b (used to derive tone-on-tone)."""
    ra, rb = hex2rgb(a), hex2rgb(b)
    return rgb2hex([ra[i] + (rb[i] - ra[i]) * t for i in range(3)])


def f(v):
    return ("%.1f" % v).rstrip("0").rstrip(".") if abs(v) >= 0.05 else "0"


# ------------------------------------------------------------- primitives ---
# Every shape is a path in local coordinates centred on (0, 0); a Canvas
# places it with translate + rotate and can drop a hard offset shadow under it.

def sh_rect(w, h, r=0):
    if r <= 0:
        return f"M{f(-w/2)} {f(-h/2)}H{f(w/2)}V{f(h/2)}H{f(-w/2)}Z"
    return (f"M{f(-w/2+r)} {f(-h/2)}H{f(w/2-r)}A{f(r)} {f(r)} 0 0 1 {f(w/2)} {f(-h/2+r)}"
            f"V{f(h/2-r)}A{f(r)} {f(r)} 0 0 1 {f(w/2-r)} {f(h/2)}H{f(-w/2+r)}"
            f"A{f(r)} {f(r)} 0 0 1 {f(-w/2)} {f(h/2-r)}V{f(-h/2+r)}A{f(r)} {f(r)} 0 0 1 {f(-w/2+r)} {f(-h/2)}Z")


def sh_circle(r):
    return f"M{f(r)} 0A{f(r)} {f(r)} 0 1 1 {f(-r)} 0A{f(r)} {f(r)} 0 1 1 {f(r)} 0Z"


def sh_ring(R, r):
    """Use with fill-rule evenodd."""
    return sh_circle(R) + sh_circle(r)


def sh_arch(w, h):
    """Doorway arch: flat bottom at +h/2, round top."""
    r = w / 2
    return f"M{f(-r)} {f(h/2)}V{f(-h/2+r)}A{f(r)} {f(r)} 0 0 1 {f(r)} {f(-h/2+r)}V{f(h/2)}Z"


def sh_arch_ring(w, h, band):
    """Arch with an arch-shaped hole sharing the bottom edge (the Kiln motif). evenodd."""
    r = (w - 2 * band) / 2
    hi = h - band
    dy = band / 2
    inner = f"M{f(-r)} {f(hi/2+dy)}V{f(-hi/2+r+dy)}A{f(r)} {f(r)} 0 0 1 {f(r)} {f(-hi/2+r+dy)}V{f(hi/2+dy)}Z"
    return sh_arch(w, h) + inner


def sh_pill(w, h):
    return sh_rect(w, h, min(w, h) / 2)


def sh_bitten(R, rb, ang_deg, d):
    """Circle of radius R with a round bite (radius rb, centre at distance d)."""
    a = math.radians(ang_deg)
    x = (d * d - rb * rb + R * R) / (2 * d)
    y = math.sqrt(max(R * R - x * x, 0.0))
    ca, sa = math.cos(a), math.sin(a)
    p1 = (x * ca - y * sa, x * sa + y * ca)
    p2 = (x * ca + y * sa, x * sa - y * ca)
    # the long way round the main circle from p1 to p2, then the bite arc back
    return (f"M{f(p1[0])} {f(p1[1])}A{f(R)} {f(R)} 0 1 1 {f(p2[0])} {f(p2[1])}"
            f"A{f(rb)} {f(rb)} 0 0 0 {f(p1[0])} {f(p1[1])}Z")


def sh_notched(s, n):
    """Square with one square notch cut from the top-right corner."""
    h = s / 2
    return f"M{f(-h)} {f(-h)}H{f(h-n)}V{f(-h+n)}H{f(h)}V{f(h)}H{f(-h)}Z"


def sh_steps(w, h, k):
    """Stepped cut-out: k steps rising to the right, flat bottom."""
    pts = [(-w / 2, h / 2)]
    sw, shh = w / k, h / k
    for i in range(k):
        x0 = -w / 2 + i * sw
        y = h / 2 - (i + 1) * shh
        pts += [(x0, y), (x0 + sw, y)]
    pts.append((w / 2, h / 2))
    return "M" + "L".join(f"{f(x)} {f(y)}" for x, y in pts) + "Z"


def sh_blob(r, rng, n=7, wob=0.16):
    """Soft closed curve (Catmull-Rom through seeded radii, as cubic Beziers)."""
    radii = [r * (1 + wob * (rng.random() * 2 - 1)) for _ in range(n)]
    pts = [(radii[i] * math.cos(2 * math.pi * i / n), radii[i] * math.sin(2 * math.pi * i / n)) for i in range(n)]
    d = f"M{f(pts[0][0])} {f(pts[0][1])}"
    for i in range(n):
        p0, p1, p2, p3 = pts[i - 1], pts[i], pts[(i + 1) % n], pts[(i + 2) % n]
        c1 = (p1[0] + (p2[0] - p0[0]) / 6, p1[1] + (p2[1] - p0[1]) / 6)
        c2 = (p2[0] - (p3[0] - p1[0]) / 6, p2[1] - (p3[1] - p1[1]) / 6)
        d += f"C{f(c1[0])} {f(c1[1])} {f(c2[0])} {f(c2[1])} {f(p2[0])} {f(p2[1])}"
    return d + "Z"


def smooth_path(pts):
    """Quadratic midpoint smoothing through points (hand-drawn line)."""
    if len(pts) < 3:
        return "M" + "L".join(f"{f(x)} {f(y)}" for x, y in pts)
    d = f"M{f(pts[0][0])} {f(pts[0][1])}"
    for i in range(1, len(pts) - 1):
        mx, my = (pts[i][0] + pts[i + 1][0]) / 2, (pts[i][1] + pts[i + 1][1]) / 2
        d += f"Q{f(pts[i][0])} {f(pts[i][1])} {f(mx)} {f(my)}"
    return d + f"L{f(pts[-1][0])} {f(pts[-1][1])}"


class Wobble:
    """Low-frequency seeded wobble: sum of three sines with random phases."""

    def __init__(self, rng, amp):
        self.terms = [(amp * a, rng.uniform(1.0, 2.2) * k, rng.uniform(0, 2 * math.pi))
                      for a, k in ((0.6, 1), (0.3, 2.3), (0.15, 4.7))]

    def __call__(self, t):
        return sum(a * math.sin(2 * math.pi * k * t + p) for a, k, p in self.terms)


# ----------------------------------------------------------------- canvas ---
class Canvas:
    def __init__(self, w, h, scheme, rng):
        self.w, self.h, self.scheme, self.rng = w, h, scheme, rng
        self.u = min(w, h) / 1080.0  # 1 design unit = 1 px at a 1080 px short side
        self.out = []
        self.shadow = ("#000000", 0.30) if scheme == "dark" else (INK, 0.13)
        self.off = (5, 6)  # hard offset, down-right, in design units

    def X(self, fx):
        return fx * self.w

    def Y(self, fy):
        return fy * self.h

    def jit(self, a):
        return self.rng.uniform(-a, a)

    def field(self, color):
        self.out.append(f'<rect width="{self.w}" height="{self.h}" fill="{color}"/>')

    def piece(self, d, x, y, fill, rot=0.0, shadow=True, evenodd=False, scale=1.0, off=None):
        """Place shape d (design units) at pixel (x, y), rotated, with a hard offset shadow."""
        s = self.u * scale
        fr = ' fill-rule="evenodd"' if evenodd else ""
        if shadow:
            ox, oy = off or self.off
            col, op = self.shadow
            self.out.append(
                f'<path d="{d}" fill="{col}" fill-opacity="{op}"{fr} '
                f'transform="translate({f(x + ox * self.u)} {f(y + oy * self.u)}) rotate({f(rot)}) scale({s:.4f})"/>')
        self.out.append(f'<path d="{d}" fill="{fill}"{fr} '
                        f'transform="translate({f(x)} {f(y)}) rotate({f(rot)}) scale({s:.4f})"/>')

    def ink(self, pts, color, width=2.25, opacity=None):
        """Hand-drawn ink stroke through pixel points; width in design units."""
        op = f' stroke-opacity="{opacity}"' if opacity is not None else ""
        self.out.append(
            f'<path d="{smooth_path(pts)}" fill="none" stroke="{color}"{op} '
            f'stroke-width="{f(width * self.u)}" stroke-linecap="round" stroke-linejoin="round"/>')

    def ink_loop(self, x0, y0, x1, y1, loops=1, r=60, color=INK, width=2.25, wob=6, opacity=None):
        """A line from (x0,y0) to (x1,y1) with `loops` small curls (prolate cycloid)."""
        n = 140
        wv = Wobble(self.rng, wob * self.u)
        dx, dy = x1 - x0, y1 - y0
        L = math.hypot(dx, dy) or 1
        ux, uy = dx / L, dy / L
        nx, ny = -uy, ux
        R = r * self.u
        pts = []
        for i in range(n + 1):
            t = i / n
            e = min(1.0, max(0.0, (t - 0.2) / 0.6))  # curl only in the middle 60 %
            env = math.sin(math.pi * e)
            ph = 2 * math.pi * loops * e
            along = -R * math.sin(ph) * env
            across = R * (1 - math.cos(ph)) * env
            w_ = wv(t)
            pts.append((x0 + dx * t + ux * along + nx * (across + w_),
                        y0 + dy * t + uy * along + ny * (across + w_)))
        self.ink(pts, color, width, opacity)

    def ink_curve(self, ctrl, color=INK, width=2.25, wob=4, n=90, opacity=None):
        """Wobbly line along a cubic Bezier given 4 pixel control points."""
        (ax, ay), (bx, by), (cx, cy), (dx, dy) = ctrl
        wv = Wobble(self.rng, wob * self.u)
        pts, prev = [], None
        for i in range(n + 1):
            t = i / n
            mt = 1 - t
            x = mt ** 3 * ax + 3 * mt * mt * t * bx + 3 * mt * t * t * cx + t ** 3 * dx
            y = mt ** 3 * ay + 3 * mt * mt * t * by + 3 * mt * t * t * cy + t ** 3 * dy
            if prev:
                tx, ty = x - prev[0], y - prev[1]
                L = math.hypot(tx, ty) or 1
                pts.append((x - ty / L * wv(t), y + tx / L * wv(t)))
            else:
                pts.append((x, y))
            prev = (x, y)
        self.ink(pts, color, width, opacity)

    def ink_circle(self, cx, cy, r, color=INK, width=2.0, wob=3, overshoot=0.08, opacity=None):
        """Hand-drawn circle that does not quite close (slight overshoot)."""
        wv = Wobble(self.rng, wob * self.u)
        a0 = self.rng.uniform(0, 2 * math.pi)
        n = 100
        pts = []
        for i in range(n + 1):
            t = i / n
            a = a0 + 2 * math.pi * (1 + overshoot) * t
            rr = r * self.u + wv(t)
            pts.append((cx + rr * math.cos(a), cy + rr * math.sin(a)))
        self.ink(pts, color, width, opacity)

    def svg(self, title):
        head = (f'<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {self.w} {self.h}" '
                f'width="{self.w}" height="{self.h}">\n<title>{title}</title>\n'
                f'<!-- Original Glaze artwork, generated by scripts/gen-wallpapers.py. MIT OR Apache-2.0. -->\n')
        return head + "\n".join(self.out) + "\n</svg>\n"


# ------------------------------------------------------------ compositions ---
# Each composer draws one set. `c.rng` is seeded from (set, orientation, seed)
# only, so the light and dark variants of a set share one composition.
# Landscape keeps the top-left ~45 % x 60 % empty for desktop icons; portrait
# keeps the middle band empty for the app grid.

def compose_kiln(c, portrait):
    dark = c.scheme == "dark"
    field = mix(CHAR, TERRA, 0.10) if dark else TERRA
    tone = mix(field, "#FFFFFF", 0.06) if dark else mix(field, INK, 0.07)  # tone-on-tone
    sheet_back = TERRA_DEEP if dark else CREAM_2
    sheet_front = TERRA if dark else CREAM
    sun = CREAM_2 if dark else INK
    accent = TERRA if dark else CREAM
    line = mix(field, CREAM, 0.45) if dark else INK
    c.field(field)
    if not portrait:
        c.piece(sh_arch_ring(560, 900, 84), c.X(0.70) + c.jit(10), c.Y(1.0) + 120 * c.u, tone,
                rot=c.jit(0.8), evenodd=True, shadow=False)
        bx, by = c.X(0.66), c.Y(0.50)
        c.piece(sh_rect(300, 384), bx, by, sheet_back, rot=-3.2 + c.jit(0.6))
        c.piece(sh_rect(300, 384), bx + 38 * c.u, by - 30 * c.u, sheet_front, rot=1.4 + c.jit(0.6))
        sx, sy = c.X(0.85), c.Y(0.25)
        c.piece(sh_circle(78), sx - 16 * c.u, sy + 14 * c.u, tone, shadow=False)
        c.piece(sh_circle(70), sx, sy, sun, shadow=False)
        c.piece(sh_ring(46, 18), c.X(0.92), c.Y(0.72), accent, evenodd=True)
        c.piece(sh_notched(64, 22), c.X(0.53), c.Y(0.80), tone, rot=6 + c.jit(2), shadow=False)
        c.piece(sh_bitten(30, 18, -40, 30), c.X(0.80), c.Y(0.57), accent, rot=c.jit(10))
        for i, (fx, fy) in enumerate(((0.56, 0.27), (0.585, 0.24), (0.97, 0.45))):
            c.piece(sh_circle(6 + 2 * (i % 2)), c.X(fx), c.Y(fy), accent, shadow=False)
        c.ink_loop(bx - 90 * c.u, by - 250 * c.u, sx - 110 * c.u, sy + 10 * c.u, loops=1, r=30,
                   color=line, width=2.25)
    else:
        c.piece(sh_arch_ring(700, 1100, 100), c.X(0.62), c.Y(1.0) + 200 * c.u, tone,
                rot=c.jit(0.8), evenodd=True, shadow=False)
        bx, by = c.X(0.58), c.Y(0.80)
        c.piece(sh_rect(330, 420), bx, by, sheet_back, rot=-3.0 + c.jit(0.6))
        c.piece(sh_rect(330, 420), bx + 40 * c.u, by - 34 * c.u, sheet_front, rot=1.6 + c.jit(0.6))
        sx, sy = c.X(0.76), c.Y(0.14)
        c.piece(sh_circle(92), sx - 18 * c.u, sy + 16 * c.u, tone, shadow=False)
        c.piece(sh_circle(82), sx, sy, sun, shadow=False)
        c.piece(sh_ring(44, 17), c.X(0.20), c.Y(0.86), accent, evenodd=True)
        c.piece(sh_notched(70, 24), c.X(0.17), c.Y(0.12), tone, rot=7, shadow=False)
        c.piece(sh_bitten(28, 17, -40, 28), c.X(0.88), c.Y(0.70), accent)
        c.ink_loop(c.X(0.30), c.Y(0.20), sx - 140 * c.u, sy + 30 * c.u, loops=1, r=38, color=line)


def ridge(c, x_from, x_to, base, amp, wv, step=8):
    pts = []
    x = x_from
    while x <= x_to + step:
        t = (x - x_from) / max(1, (x_to - x_from))
        pts.append((x, base(t) + amp * wv(t)))
        x += step * c.u
    return pts


def compose_draft(c, portrait):
    dark = c.scheme == "dark"
    field = CHAR if dark else "#F5F1E8"
    hills = (["#2A2724", "#3A302A", mix(CHAR, TERRA, 0.55)] if dark else [SAND, "#E3C3A6", TERRA])
    contour = CREAM if dark else INK
    c_op = 0.32 if dark else 0.55
    scrap = hills[1] if dark else hills[0]
    c.field(field)
    w, h = c.w, c.h
    if not portrait:
        bases = [(0.60, 0.50), (0.70, 0.60), (0.80, 0.72)]  # (left, right) ridge heights, fraction of h
        amp = 26
        sx, sy = c.X(0.80), c.Y(0.30)
    else:
        bases = [(0.70, 0.66), (0.77, 0.74), (0.85, 0.83)]
        amp = 30
        sx, sy = c.X(0.70), c.Y(0.16)
    c.piece(sh_circle(64), sx, sy, TERRA, shadow=False)
    c.ink_circle(sx + 6 * c.u, sy - 4 * c.u, 92, color=contour, width=1.75, opacity=c_op)
    c.piece(sh_notched(54, 18), c.X(0.62 if not portrait else 0.24), c.Y(0.20 if not portrait else 0.11),
            scrap, rot=-8 + c.jit(2))
    c.piece(sh_pill(90, 26), c.X(0.93 if not portrait else 0.86), c.Y(0.44 if not portrait else 0.32),
            scrap, rot=c.jit(2))
    for i, (col, (bl, br)) in enumerate(zip(hills, bases)):
        wv = Wobble(c.rng, 1.0)
        top = ridge(c, -20 * c.u, w + 20 * c.u, lambda t, bl=bl, br=br: h * (bl + (br - bl) * t),
                    amp * c.u, wv)
        d = ("M" + "L".join(f"{f(x)} {f(y)}" for x, y in top) + f"L{f(w + 40 * c.u)} {f(h + 40 * c.u)}"
             f"L{f(-40 * c.u)} {f(h + 40 * c.u)}Z")
        ox, oy = c.off
        col_s, op_s = c.shadow
        # hill casts its hard shadow up-left onto the layer behind it
        c.out.append(f'<path d="{d}" fill="{col_s}" fill-opacity="{op_s}" '
                     f'transform="translate({f(-ox * c.u)} {f(-oy * c.u)})"/>')
        c.out.append(f'<path d="{d}" fill="{col}"/>')
        for k in (1, 2):  # contour lines: the ridge offset downward
            if i == 0 and k == 2:
                continue
            dy = (26 + 4 * i) * k * c.u
            span0, span1 = (0.10, 0.95) if k == 1 else (0.30, 0.92)
            pts = [(x, y + dy + 3 * c.u * math.sin(x / (90 * c.u) + i)) for x, y in top
                   if span0 * w < x < span1 * w]
            c.ink(pts, contour, width=1.5, opacity=c_op)


def compose_margin(c, portrait):
    dark = c.scheme == "dark"
    field = CHAR if dark else CREAM
    tone = mix(field, "#FFFFFF", 0.05) if dark else mix(field, INK, 0.05)
    dot = CREAM_2 if dark else TERRA
    pen = mix(field, CREAM, 0.5) if dark else INK
    c.field(field)
    if not portrait:
        mx = c.X(0.80)
        for dx in (0, 7):  # double margin rule, hand-ruled (a hair off vertical)
            x = mx + dx * c.u
            c.ink([(x, -10), (x + 2 * c.u, c.h * 0.5), (x + 1 * c.u, c.h + 10)], TERRA, width=1.25)
        c.piece(sh_arch(220, 420), c.X(0.90), c.h - 150 * c.u, tone, rot=c.jit(0.6))
        c.piece(sh_circle(11), mx + 3.5 * c.u, c.Y(0.20), dot, shadow=False)
        c.ink_curve([(c.X(0.84), c.Y(0.30)), (c.X(0.87), c.Y(0.27)), (c.X(0.90), c.Y(0.34)),
                     (c.X(0.93), c.Y(0.30))], color=pen, width=1.75, wob=2)
    else:
        my = c.Y(0.86)
        for dy in (0, 7):
            y = my + dy * c.u
            c.ink([(-10, y), (c.w * 0.5, y + 2 * c.u), (c.w + 10, y + 1 * c.u)], TERRA, width=1.25)
        c.piece(sh_arch(240, 440), c.X(0.78), c.h - 40 * c.u, tone, rot=c.jit(0.6))
        c.piece(sh_circle(12), c.X(0.16), my + 3.5 * c.u, dot, shadow=False)
        c.ink_curve([(c.X(0.10), c.Y(0.93)), (c.X(0.18), c.Y(0.91)), (c.X(0.26), c.Y(0.95)),
                     (c.X(0.34), c.Y(0.92))], color=pen, width=1.75, wob=2)


def compose_night(c, portrait):
    field = "#1C1B19"
    sheet_a, sheet_b, sheet_c = "#E9E2D6", CREAM_2, CREAM
    line = mix(field, CREAM, 0.42)
    c.field(field)
    if not portrait:
        c.piece(sh_circle(210), c.X(0.80), c.Y(0.70), TERRA, shadow=False)
        bx, by = c.X(0.66), c.Y(0.55)
        c.piece(sh_rect(260, 340), bx - 30 * c.u, by + 20 * c.u, sheet_a, rot=-5 + c.jit(0.8))
        c.piece(sh_rect(260, 340), bx, by, sheet_b, rot=-1.5 + c.jit(0.6))
        c.piece(sh_rect(260, 340), bx + 34 * c.u, by - 28 * c.u, sheet_c, rot=2.2 + c.jit(0.6))
        c.piece(sh_bitten(42, 36, -30, 26), c.X(0.88), c.Y(0.20), CREAM_2)
        for fx, fy, r in ((0.55, 0.14, 4), (0.74, 0.10, 3), (0.95, 0.40, 3.5), (0.60, 0.30, 2.5)):
            c.piece(sh_circle(r), c.X(fx), c.Y(fy), CREAM_2 if r < 3.5 else TERRA, shadow=False)
        c.piece(sh_steps(120, 60, 3), c.X(0.50), c.Y(0.86), "#2C2A27", rot=c.jit(1))
        c.ink_loop(bx + 150 * c.u, by - 220 * c.u, c.X(0.86) - 80 * c.u, c.Y(0.26), loops=1, r=30,
                   color=line, width=2.0)
    else:
        c.piece(sh_circle(240), c.X(0.70), c.Y(0.88), TERRA, shadow=False)
        bx, by = c.X(0.52), c.Y(0.82)
        c.piece(sh_rect(290, 380), bx - 30 * c.u, by + 22 * c.u, sheet_a, rot=-5 + c.jit(0.8))
        c.piece(sh_rect(290, 380), bx, by, sheet_b, rot=-1.5 + c.jit(0.6))
        c.piece(sh_rect(290, 380), bx + 36 * c.u, by - 30 * c.u, sheet_c, rot=2.2 + c.jit(0.6))
        c.piece(sh_bitten(46, 40, -30, 28), c.X(0.78), c.Y(0.11), CREAM_2)
        for fx, fy, r in ((0.20, 0.08, 4), (0.42, 0.15, 3), (0.90, 0.24, 3.5), (0.12, 0.22, 2.5)):
            c.piece(sh_circle(r), c.X(fx), c.Y(fy), CREAM_2 if r < 3.5 else TERRA, shadow=False)
        c.ink_loop(c.X(0.25), c.Y(0.14), c.X(0.70), c.Y(0.12), loops=1, r=30, color=line, width=2.0)


def compose_teal(c, portrait):
    dark = c.scheme == "dark"
    field = "#1E4644" if dark else "#2F6E6A"
    tone = mix(field, "#FFFFFF", 0.05)
    tone2 = mix(field, "#000000", 0.08)
    c.field(field)
    if not portrait:
        c.piece(sh_arch_ring(420, 700, 64), c.X(0.84), c.h + 60 * c.u, tone, evenodd=True,
                rot=c.jit(0.5), shadow=False)
        c.piece(sh_rect(150, 150), c.X(0.70), c.Y(0.82), tone2, rot=4 + c.jit(1), shadow=False)
        c.piece(sh_circle(9), c.X(0.84), c.Y(0.55), TERRA, shadow=False)
    else:
        c.piece(sh_arch_ring(460, 760, 70), c.X(0.68), c.h + 60 * c.u, tone, evenodd=True,
                rot=c.jit(0.5), shadow=False)
        c.piece(sh_rect(150, 150), c.X(0.24), c.Y(0.90), tone2, rot=4 + c.jit(1), shadow=False)
        c.piece(sh_circle(9), c.X(0.68), c.Y(0.80), TERRA, shadow=False)


# (id, title, description, composer, schemes, curated seed, grain strength)
SETS = [
    ("hills-hard-edge", "Hills (hard edge)", "Paper Hills with crisp hard-offset paper shadows instead of soft ones.",
     "legacy-hills", ("light", "dark"), 1, 1.0),
    ("kiln", "Kiln", "Stacked cream sheets and an ink sun on a terracotta field.",
     compose_kiln, ("light", "dark"), 1, 1.0),
    ("draft", "Draft", "Cream drafting paper with contour-lined paper hills and a clay sun.",
     compose_draft, ("light", "dark"), 1, 1.0),
    ("margin", "Margin", "Almost nothing: a hand-ruled terracotta margin and one paper arch.",
     compose_margin, ("light", "dark"), 1, 0.8),
    ("night-paper", "Night Paper", "Cream sheets, a paper moon and a terracotta disc on charcoal.",
     compose_night, ("dark",), 1, 1.0),
    ("classic-teal", "Classic Teal", "Flat heritage teal with a faint paper arch and grain.",
     compose_teal, ("light", "dark"), 1, 0.7),
]
# The original v1 sets (the user's favourite 'Hills'; paper-hills is the
# default set in crates/glaze-common/src/config.rs). Their SVGs are NOT
# regenerated here: they stay on disk unchanged and are reproducible
# byte-for-byte with scripts/gen-wallpapers-legacy.py. Their manifest
# entries are written first and exactly as v1 wrote them.
LEGACY = [
    ("paper-hills", "Paper Hills", "Layered paper-cut hills under a low clay sun."),
    ("terracotta-waves", "Terracotta Waves", "Calm warm-grey paper waves with a single clay disc."),
]


def compose_hills_hard(scheme, orientation, w, h):
    """'Hills (hard edge)': the v1 Paper Hills drawing, with each blurred layer
    shadow replaced by a hard, flat, tone-on-tone offset. Reuses the legacy
    generator's code so the hills, sun and clouds stay exactly the same shapes."""
    import importlib.util
    import re
    spec = importlib.util.spec_from_file_location(
        "legacy_wp", os.path.join(ROOT, "scripts", "gen-wallpapers-legacy.py"))
    legacy = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(legacy)
    form = "mobile" if orientation == "portrait" else "desktop"
    legacy.SIZES = {form: (w, h)}
    text = legacy.build("hills", scheme, form)
    u = min(w, h) / 1080.0
    text = re.sub(r"<defs><filter.*?</filter></defs>\n?", "", text, flags=re.S)
    text = text.replace("<title>hills ", "<title>hills-hard-edge ")

    def hard(m):
        op = float(m.group(1)) * 0.75
        return f'fill-opacity="{op:.2f}" transform="translate({f(4 * u)} {f(-6 * u)})"'

    text = re.sub(r'opacity="([0-9.]+)" filter="url\(#s\)" transform="translate\(0 -6\)"', hard, text)
    return text


def build(set_id, composer, scheme, orientation, w, h, seed):
    if composer == "legacy-hills":
        return compose_hills_hard(scheme, orientation, w, h)
    rng = random.Random(f"{set_id}/{orientation}/{seed}")
    c = Canvas(w, h, scheme, rng)
    composer(c, orientation == "portrait")
    return c.svg(f"{set_id} {scheme} {orientation} {w}x{h}")


# ------------------------------------------------------------ paper grain ---
def png_bytes(w, h, rows):
    """Minimal RGBA PNG encoder (stdlib)."""
    raw = b"".join(b"\x00" + bytes(r) for r in rows)

    def chunk(t, data):
        return struct.pack(">I", len(data)) + t + data + struct.pack(">I", zlib.crc32(t + data) & 0xFFFFFFFF)

    return (b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", struct.pack(">IIBBBBB", w, h, 8, 6, 0, 0, 0))
            + chunk(b"IDAT", zlib.compress(raw, 9)) + chunk(b"IEND", b""))


def grain_tile(strength=1.0, size=256, seed=7):
    """Tileable paper grain: fine speckle + soft blotches + a few fibres.

    Pixels are warm white or ink at alpha <= ~11/255 (about 2-4 % luminance
    variation once composited). Deterministic for a given seed.
    """
    rng = random.Random(seed)
    cells = 16
    lat = [[rng.uniform(-1, 1) for _ in range(cells)] for _ in range(cells)]

    def vnoise(x, y):  # value noise on a wrapping lattice, so the tile tiles
        gx, gy = x / size * cells, y / size * cells
        x0, y0 = int(gx) % cells, int(gy) % cells
        x1, y1 = (x0 + 1) % cells, (y0 + 1) % cells
        tx, ty = gx - int(gx), gy - int(gy)
        tx, ty = tx * tx * (3 - 2 * tx), ty * ty * (3 - 2 * ty)
        a = lat[y0][x0] + (lat[y0][x1] - lat[y0][x0]) * tx
        b = lat[y1][x0] + (lat[y1][x1] - lat[y1][x0]) * tx
        return a + (b - a) * ty

    val = [[0.55 * rng.uniform(-1, 1) + 0.45 * vnoise(x, y) for x in range(size)] for y in range(size)]
    for _ in range(70):  # fibres, wrapped at the tile edges
        x, y = rng.uniform(0, size), rng.uniform(0, size)
        a = rng.uniform(0, math.pi)
        L = rng.uniform(6, 22)
        sgn = rng.choice((-1, 1))
        for i in range(int(L)):
            px, py = int(x + i * math.cos(a)) % size, int(y + i * math.sin(a)) % size
            val[py][px] += 0.5 * sgn
    amax = 11 * strength
    rows = []
    for y in range(size):
        r = []
        for x in range(size):
            v = max(-1.6, min(1.6, val[y][x])) / 1.6
            r += [255, 252, 244, int(round(v * amax))] if v >= 0 else [43, 41, 38, int(round(-v * amax))]
        rows.append(r)
    return png_bytes(size, size, rows)


def render_png(svg_text, w, h, dst, grain_png, rsvg):
    """Render via rsvg-convert. Grain is composited by wrapping the source in a
    throwaway SVG that tiles the grain PNG over it; the sources stay clean."""
    body = svg_text
    if grain_png:
        b64 = base64.b64encode(grain_png).decode()
        inner = svg_text[svg_text.index(">", svg_text.index("<svg")) + 1: svg_text.rindex("</svg>")]
        body = (f'<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {w} {h}" width="{w}" height="{h}">'
                f'<defs><pattern id="grain" width="256" height="256" patternUnits="userSpaceOnUse">'
                f'<image width="256" height="256" href="data:image/png;base64,{b64}"/></pattern></defs>'
                f'{inner}<rect width="{w}" height="{h}" fill="url(#grain)"/></svg>')
    subprocess.run([rsvg, "-w", str(w), "-h", str(h), "-o", dst], input=body.encode(), check=True)


def contact_sheet(entries, dst, rsvg):
    """entries: list of (label, png_path, w, h). Landscape row(s) first, then portrait."""
    pad, W = 20, 20 + 6 * (300 + 20)
    y, parts = 60, []
    for want_portrait, tw, cols in ((False, 300, 6), (True, 120, 11)):
        group = [e for e in entries if (e[3] > e[2]) == want_portrait]
        for r0 in range(0, len(group), cols):
            rh = 0
            for i, (label, path, w, h) in enumerate(group[r0:r0 + cols]):
                th = round(tw * h / w)
                rh = max(rh, th)
                x = pad + i * ((W - pad) // cols)
                with open(path, "rb") as fh:
                    b64 = base64.b64encode(fh.read()).decode()
                parts.append(f'<rect x="{x - 1}" y="{y - 1}" width="{tw + 2}" height="{th + 2}" fill="#D8D0C2"/>'
                             f'<image x="{x}" y="{y}" width="{tw}" height="{th}" preserveAspectRatio="none" '
                             f'href="data:image/png;base64,{b64}"/>'
                             f'<text x="{x}" y="{y + th + 15}" font-family="sans-serif" font-size="11" '
                             f'fill="#2B2926">{label}</text>')
            y += rh + 36
    svg = (f'<svg xmlns="http://www.w3.org/2000/svg" width="{W}" height="{y}" viewBox="0 0 {W} {y}">'
           f'<rect width="{W}" height="{y}" fill="#F5F1E8"/>'
           f'<text x="{pad}" y="36" font-family="sans-serif" font-size="20" fill="#2B2926">'
           f'Glaze wallpapers v2 (PNG renders, grain baked in)</text>' + "".join(parts) + "</svg>")
    subprocess.run([rsvg, "-o", dst], input=svg.encode(), check=True)


def legacy_stanzas():
    """The v1 manifest entries, verbatim (same keys, order and values)."""
    out = ["# ---- Hills: the original v1 sets, shipped unchanged (paper-hills is the default) ----", ""]
    for sid, title, desc in LEGACY:
        out += ["[[set]]", 'id = "%s"' % sid, 'title = "%s"' % title, 'description = "%s"' % desc, ""]
        for scheme in ("light", "dark"):
            for form, orient in (("desktop", "landscape"), ("mobile", "portrait")):
                w, h = {"desktop": (1920, 1080), "mobile": (1080, 2340)}[form]
                out += ["[[set.variant]]", 'scheme = "%s"' % scheme, 'orientation = "%s"' % orient,
                        'file = "%s-%s-%s.svg"' % (sid, scheme, form), "width = %d" % w, "height = %d" % h, ""]
    out += ["# ---- v2 sets (scripts/gen-wallpapers.py) ----", ""]
    return out


def toml_str(s):
    return '"' + s.replace("\\", "\\\\").replace('"', '\\"') + '"'


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--set", help="only this set id (manifest and contact sheet are not rewritten)")
    ap.add_argument("--seed", type=int, help="override the curated seed")
    ap.add_argument("--svg-only", action="store_true")
    ap.add_argument("--no-grain", action="store_true")
    ap.add_argument("--no-contact", action="store_true")
    args = ap.parse_args()

    rsvg = None if args.svg_only else shutil.which("rsvg-convert")
    if not args.svg_only and not rsvg:
        sys.exit("rsvg-convert not found (install librsvg) or pass --svg-only")

    man = ["# Glaze bundled wallpapers. Original flat cut-paper artwork generated by",
           "# scripts/gen-wallpapers.py (MIT OR Apache-2.0, see LICENSE.md).",
           "# `file` is what the resolver (crates/glaze-common/src/wallpaper.rs) loads:",
           "# a grain-baked PNG at 1920x1080 (landscape) or 1080x2340 (portrait).",
           "# Extra keys (seed, source, renders) are ignored by the resolver; they",
           "# record how to reproduce each file and which other sizes exist.",
           "schema = 1", ""]
    contact = []
    grain_cache = {}
    for sid, title, desc, composer, schemes, seed0, gstr in SETS:
        if args.set and sid != args.set:
            continue
        seed = args.seed if args.seed is not None else seed0
        d_src = os.path.join(OUT, sid, "src")
        os.makedirs(d_src, exist_ok=True)
        man += ["[[set]]", f"id = {toml_str(sid)}", f"title = {toml_str(title)}",
                f"description = {toml_str(desc)}", ""]
        for scheme in schemes:
            for orientation, sizes in (("landscape", LANDSCAPE), ("portrait", PORTRAIT)):
                renders = []
                for (w, h) in sizes:
                    base = f"{sid}-{scheme}-{orientation}-{w}x{h}"
                    text = build(sid, composer, scheme, orientation, w, h, seed)
                    with open(os.path.join(d_src, base + ".svg"), "w") as fh:
                        fh.write(text)
                    if rsvg:
                        g = None if args.no_grain else grain_cache.setdefault(gstr, grain_tile(gstr))
                        dst = os.path.join(OUT, sid, base + ".png")
                        render_png(text, w, h, dst, g, rsvg)
                        renders.append(f"{sid}/{base}.png")
                        if (w, h) in ((1920, 1080), (1080, 2340)):
                            contact.append((f"{sid} {scheme}", dst, w, h))
                mw, mh = MANIFEST_SIZE[orientation]
                mbase = f"{sid}-{scheme}-{orientation}-{mw}x{mh}"
                file_ = f"{sid}/{mbase}.png" if rsvg else f"{sid}/src/{mbase}.svg"
                man += ["[[set.variant]]", f"scheme = {toml_str(scheme)}",
                        f"orientation = {toml_str(orientation)}", f"file = {toml_str(file_)}",
                        f"width = {mw}", f"height = {mh}", f"seed = {seed}",
                        f"source = {toml_str(f'{sid}/src/{mbase}.svg')}",
                        "renders = [" + ", ".join(toml_str(r) for r in renders) + "]", ""]
    if not args.set:
        man = man[:man.index("schema = 1") + 2] + legacy_stanzas() + man[man.index("schema = 1") + 2:]
        with open(os.path.join(OUT, "manifest.toml"), "w") as fh:
            fh.write("\n".join(man))
        if rsvg and contact and not args.no_contact:
            contact_sheet(contact, os.path.join(OUT, "contact-sheet.png"), rsvg)
    print("wrote", OUT)


if __name__ == "__main__":
    main()
