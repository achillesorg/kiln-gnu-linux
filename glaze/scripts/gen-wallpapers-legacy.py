#!/usr/bin/env python3
"""Generate Glaze's bundled wallpapers (original flat paper-cut SVG artwork).

LEGACY v1 generator, kept so the shipped paper-hills / terracotta-waves SVGs
(the 'Hills' wallpapers the user likes, and the current default set) can be
reproduced byte-for-byte. The drawing code below is unchanged from v1; only
the output handling changed: it no longer rewrites manifest.toml (that is now
owned by scripts/gen-wallpapers.py, which lists these sets unchanged), and
--out lets you regenerate into another directory to compare checksums:

    python3 scripts/gen-wallpapers-legacy.py --out /tmp/wp && \
      cmp /tmp/wp/paper-hills-light-desktop.svg assets/wallpapers/paper-hills-light-desktop.svg

Deterministic: no randomness, no external assets.
"""
import math, os, sys

OUT = os.path.join(os.path.dirname(__file__), "..", "assets", "wallpapers")
SIZES = {"desktop": (1920, 1080), "mobile": (1080, 2340)}

PAL = {
    ("hills", "light"): dict(sky="#F6F1E7", sun="#E9A687", sun2="#F1C7AF",
        layers=["#EBDCC6", "#E3C2A3", "#D99A76", "#C2704A", "#8F4E37", "#5A3B30"], shadow="#4A3620", sa=0.20),
    ("hills", "dark"): dict(sky="#1F1E1C", sun="#D97757", sun2="#3A2A22",
        layers=["#2A2724", "#33291F", "#4A3226", "#6B3E2E", "#8E4C33", "#B5603F"], shadow="#000000", sa=0.45),
    ("waves", "light"): dict(sky="#F3EFE6", sun="#D97757", sun2="#EBC9B6",
        layers=["#E9E2D3", "#DDD3C0", "#CFC2AB", "#B9AA92", "#8C7F6C", "#5E5547"], shadow="#4A3620", sa=0.18),
    ("waves", "dark"): dict(sky="#1C1B19", sun="#D97757", sun2="#2F2622",
        layers=["#25231F", "#2E2B26", "#3A362F", "#4B453C", "#5F574B", "#7A6F5F"], shadow="#000000", sa=0.45),
}

SETS = [
    ("paper-hills", "hills", "Paper Hills",
     "Layered paper-cut hills under a low clay sun."),
    ("terracotta-waves", "waves", "Terracotta Waves",
     "Calm warm-grey paper waves with a single clay disc."),
]


def ridge(w, h, base, amp, freq, phase, seed2=0.0, step=12):
    pts = []
    x = 0
    while x <= w + step:
        t = x / w
        y = base + amp * (math.sin(t * freq * 2 * math.pi + phase)
                          + 0.45 * math.sin(t * freq * 4.3 * math.pi + phase * 1.7 + seed2))
        pts.append((x, y))
        x += step
    d = "M0 %d " % h + "L%.1f %.1f " % pts[0]
    d += " ".join("L%.1f %.1f" % p for p in pts[1:])
    d += " L%d %d Z" % (w + step, h)
    return d


def band(w, h, base, amp, freq, phase):
    return ridge(w, h, base, amp, freq, phase, seed2=1.3, step=10)


def build(kind, scheme, form):
    w, h = SIZES[form]
    p = PAL[(kind, scheme)]
    portrait = form == "mobile"
    n = len(p["layers"])
    top = 0.40 if portrait else 0.36  # where layers start (fraction of height)
    span = (1 - top - 0.04)
    out = []
    out.append('<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 %d %d" width="%d" height="%d">' % (w, h, w, h))
    out.append('<title>%s %s %s</title>' % (kind, scheme, form))
    out.append('<defs><filter id="s" x="-5%%" y="-10%%" width="110%%" height="130%%">'
               '<feGaussianBlur stdDeviation="%d"/></filter></defs>' % (14 if not portrait else 18))
    out.append('<rect width="%d" height="%d" fill="%s"/>' % (w, h, p["sky"]))
    # sun / disc with a flat halo ring (no gradients)
    cx = w * (0.72 if not portrait else 0.64)
    cy = h * (0.30 if not portrait else 0.24)
    r = h * (0.10 if not portrait else 0.075)
    out.append('<circle cx="%.0f" cy="%.0f" r="%.0f" fill="%s"/>' % (cx, cy, r * 1.9, p["sun2"]))
    out.append('<circle cx="%.0f" cy="%.0f" r="%.0f" fill="%s"/>' % (cx, cy, r, p["sun"]))
    # flat paper clouds (hills set only)
    if kind == "hills":
        for (x, y, s) in ([(0.18, 0.20, 1.0), (0.42, 0.13, 0.7)] if not portrait
                          else [(0.25, 0.14, 1.0), (0.70, 0.36, 0.7)]):
            ox, oy, sc = w * x, h * y, (h * 0.045) * s
            out.append('<g fill="%s" opacity="0.85"><rect x="%.0f" y="%.0f" width="%.0f" height="%.0f" rx="%.0f"/>'
                       '<circle cx="%.0f" cy="%.0f" r="%.0f"/><circle cx="%.0f" cy="%.0f" r="%.0f"/></g>'
                       % (p["layers"][0], ox, oy, sc * 5, sc * 1.4, sc * 0.7,
                          ox + sc * 1.6, oy, sc * 1.1, ox + sc * 3.0, oy - sc * 0.2, sc * 1.4))
    for i, color in enumerate(p["layers"]):
        t = i / (n - 1)
        base = h * (top + span * (0.10 + 0.80 * t) * 0.92)
        if kind == "hills":
            amp = h * (0.030 - 0.012 * t)
            d = ridge(w, h, base, amp, 0.9 + 0.35 * i, i * 1.9)
        else:
            amp = h * (0.018 - 0.006 * t)
            d = band(w, h, base, amp, 1.4 + 0.3 * i, i * 1.1)
        if i > 0:  # soft low shadow cast by this paper layer on the one below
            out.append('<path d="%s" fill="%s" opacity="%.2f" filter="url(#s)" transform="translate(0 -6)"/>'
                       % (d, p["shadow"], p["sa"]))
        out.append('<path d="%s" fill="%s"/>' % (d, color))
    out.append('</svg>')
    return "\n".join(out) + "\n"


def main():
    out = OUT
    if "--out" in sys.argv:
        out = sys.argv[sys.argv.index("--out") + 1]
    os.makedirs(out, exist_ok=True)
    for sid, kind, title, desc in SETS:
        for scheme in ("light", "dark"):
            for form in ("desktop", "mobile"):
                name = "%s-%s-%s.svg" % (sid, scheme, form)
                with open(os.path.join(out, name), "w") as f:
                    f.write(build(kind, scheme, form))
    print("wrote legacy SVGs to", out)


if __name__ == "__main__":
    main()
