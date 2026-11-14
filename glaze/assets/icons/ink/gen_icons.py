#!/usr/bin/env python3
"""Generate Glaze's 'ink' shell glyphs (original artwork, MIT OR Apache-2.0).

    python3 assets/icons/ink/gen_icons.py

Writes three variants of every glyph next to this script:
    stroke-1.5/<name>.svg    default: 1.5 px outline
    stroke-1.25/<name>.svg   hairline variant (matches the 1-1.25 px UI strokes)
    duo-1.5/<name>.svg       1.5 px outline + flat tone-on-tone fill (16 % currentColor)

Rules (checked by check_icons.py): 24x24 viewBox, 3 px padding for the main
silhouette (nothing outside 1..23), every coordinate on a 0.25 px grid,
absolute path commands only, round caps and joins, only `currentColor`
(or `none`) as paint, no filters / ramps / text / CSS.

Optional fill token: shapes with class="glaze-fill" are the flat 'paper' area
of a glyph. They are present but transparent (fill-opacity="0") in the stroke
variants, so a toolkit may tint them by CSS (`.glaze-fill { fill-opacity: .16 }`
or a fill colour) without new files; the duo variant bakes 0.16 in.
Shapes with class="glaze-badge" (the bell dot) may be tinted with the accent.
Stdlib only, deterministic.
"""
import math
import os

HERE = os.path.dirname(os.path.abspath(__file__))


def q(v):
    """Snap to the 0.25 px grid and format."""
    v = round(v * 4) / 4
    s = ("%.2f" % v).rstrip("0").rstrip(".")
    return "0" if s in ("-0", "") else s


def pt(cx, cy, r, deg):
    a = math.radians(deg)
    return q(cx + r * math.cos(a)), q(cy + r * math.sin(a))


def arc(cx, cy, r, a0, a1, move=True):
    """Arc from angle a0 to a1 (degrees, screen coords, clockwise when a1 > a0)."""
    x0, y0 = pt(cx, cy, r, a0)
    x1, y1 = pt(cx, cy, r, a1)
    span = a1 - a0
    large = 1 if abs(span) > 180 else 0
    sweep = 1 if span > 0 else 0
    head = f"M{x0} {y0}" if move else ""
    return f"{head}A{q(r)} {q(r)} 0 {large} {sweep} {x1} {y1}"


# element helpers: (tag, attrs, role) with role in stroke | solid | fill | badge
def P(d, role="stroke"):
    return ("path", {"d": d}, role)


def C(cx, cy, r, role="stroke"):
    return ("circle", {"cx": q(cx), "cy": q(cy), "r": q(r)}, role)


def R(x, y, w, h, rx=0, role="stroke"):
    a = {"x": q(x), "y": q(y), "width": q(w), "height": q(h)}
    if rx:
        a["rx"] = q(rx)
    return ("rect", a, role)


def E(cx, cy, rx, ry, role="stroke"):
    return ("ellipse", {"cx": q(cx), "cy": q(cy), "rx": q(rx), "ry": q(ry)}, role)


SPEAKER = "M4.5 9.5H7.5L11.5 5.5V18.5L7.5 14.5H4.5Z"


def speaker_waves(n):
    out = [P(SPEAKER), P(SPEAKER, "fill")]
    for r in (3.5, 6.5, 9.5)[:n]:
        out.append(P(arc(11.5, 12, r, -40, 40)))
    return out


def wifi(arcs=3, dot=True):
    out = []
    for r in (3.5, 7.5, 11.5)[:arcs]:
        out.append(P(arc(12, 18.5, r, -135, -45)))
    if dot:
        out.append(C(12, 18.5, 1.25, "solid"))
    return out


BATT = [R(3.5, 7.5, 15, 9, 1.5), R(3.5, 7.5, 15, 9, 1.5, "fill"), P("M20.5 10.5V13.5")]


def battery(level):
    out = list(BATT)
    if level:
        out.append(R(5.5, 9.5, level, 5, 0.5, "solid"))
    return out


def gear():
    pts = []
    for k in range(8):
        a = 45 * k
        pts += [pt(12, 12, 6.75, a - 16), pt(12, 12, 9, a - 9.5), pt(12, 12, 9, a + 9.5), pt(12, 12, 6.75, a + 16)]
    d = "M" + "L".join(f"{x} {y}" for x, y in pts) + "Z"
    return [P(d), P(d, "fill"), C(12, 12, 2.75)]


BELL = "M6 16.5V11A6 6 0 0 1 18 11V16.5L19.5 18H4.5Z"
WIN_BACK = "M8.5 8.5V6.5A1 1 0 0 1 9.5 5.5H17.5A1 1 0 0 1 18.5 6.5V14.5A1 1 0 0 1 17.5 15.5H15.5"
PIN = "M9 4.5H15L14 10L17.5 13.5H6.5L10 10Z"

ICONS = {
    # shell
    "start": [P("M5 19.5V11A7 7 0 0 1 19 11V19.5"), P("M9 19.5V12.5A3 3 0 0 1 15 12.5V19.5")],
    "search": [C(10.5, 10.5, 6), C(10.5, 10.5, 6, "fill"), P("M15 15L20 20")],
    "apps": [C(x, y, 1.75, "solid") for y in (6, 12, 18) for x in (6, 12, 18)],
    "folder": [P("M3.5 18.5V6.5A1 1 0 0 1 4.5 5.5H9.5L11.5 7.5H19.5A1 1 0 0 1 20.5 8.5V18.5"
                 "A1 1 0 0 1 19.5 19.5H4.5A1 1 0 0 1 3.5 18.5Z"),
               P("M3.5 18.5V6.5A1 1 0 0 1 4.5 5.5H9.5L11.5 7.5H19.5A1 1 0 0 1 20.5 8.5V18.5"
                 "A1 1 0 0 1 19.5 19.5H4.5A1 1 0 0 1 3.5 18.5Z", "fill"),
               P("M3.5 10.5H20.5")],
    "terminal": [R(3.5, 4.5, 17, 15, 1.5), R(3.5, 4.5, 17, 15, 1.5, "fill"),
                 P("M7.5 9.5L10.5 12L7.5 14.5"), P("M12.5 15H16.5")],
    "browser": [C(12, 12, 8.5), C(12, 12, 8.5, "fill"), E(12, 12, 3.5, 8.5), P("M3.5 12H20.5")],
    "settings": gear(),
    "power": [P(arc(12, 13, 7.5, -50, 230)), P("M12 3.5V11")],
    "restart": [P(arc(12, 12, 7.5, -60, 240)), P("M4 5L8.25 5.5L6.5 9.5")],
    "logout": [P("M10 4.5H5.5A1 1 0 0 0 4.5 5.5V18.5A1 1 0 0 0 5.5 19.5H10"),
               P("M10 12H20"), P("M16.5 8.5L20 12L16.5 15.5")],
    "lock": [R(5.5, 10.5, 13, 9.5, 1.5), R(5.5, 10.5, 13, 9.5, 1.5, "fill"),
             P("M8.5 10.5V7.5A3.5 3.5 0 0 1 15.5 7.5V10.5"), P("M12 14.25V16")],
    "sleep": [P("M19.5 14.5A8 8 0 1 1 9.5 4.5A6.5 6.5 0 0 0 19.5 14.5Z"),
              P("M19.5 14.5A8 8 0 1 1 9.5 4.5A6.5 6.5 0 0 0 19.5 14.5Z", "fill")],
    # network
    "network-wifi": wifi(),
    "network-wifi-weak": wifi(1) + [P(arc(12, 18.5, r, -135, -45), "faint") for r in (7.5, 11.5)],
    "network-wired": [R(9.5, 3.5, 5, 4.5, 1), R(3.5, 16, 5, 4.5, 1), R(15.5, 16, 5, 4.5, 1),
                      P("M12 8V12"), P("M6 12H18"), P("M6 12V16"), P("M18 12V16")],
    "network-off": wifi() + [P("M4 4L20 20")],
    # volume
    "volume-mute": [P(SPEAKER), P(SPEAKER, "fill"), P("M15.5 9.5L20 14"), P("M20 9.5L15.5 14")],
    "volume-low": speaker_waves(1),
    "volume-medium": speaker_waves(2),
    "volume-high": speaker_waves(3),
    # battery
    "battery-empty": battery(0),
    "battery-low": battery(3),
    "battery-medium": battery(6),
    "battery-full": battery(11),
    "battery-charging": list(BATT) + [P("M11.75 9L9.25 12.25H12.75L10.25 15")],
    # notifications
    "bell": [P(BELL), P(BELL, "fill"), P("M10 20.5H14")],
    "bell-dot": [P(BELL), P(BELL, "fill"), P("M10 20.5H14"), C(19, 5, 2.25, "badge")],
    # window controls
    "window-close": [P("M6.5 6.5L17.5 17.5"), P("M17.5 6.5L6.5 17.5")],
    "window-minimise": [P("M6 12H18")],
    "window-maximise": [R(5.5, 5.5, 13, 13, 1), R(5.5, 5.5, 13, 13, 1, "fill")],
    "window-restore": [R(5.5, 8.5, 10, 10, 1), R(5.5, 8.5, 10, 10, 1, "fill"), P(WIN_BACK)],
    "layout-float": [R(3.5, 8.5, 11, 9, 1), R(3.5, 8.5, 11, 9, 1, "fill"),
                     P("M8.5 8.5V6.5A1 1 0 0 1 9.5 5.5H19.5A1 1 0 0 1 20.5 6.5V13.5A1 1 0 0 1 19.5 14.5H14.5")],
    "layout-tile": [R(3.5, 5.5, 17, 13, 1), R(3.5, 5.5, 8.5, 13, 1, "fill"), P("M12 5.5V18.5"), P("M12 12H20.5")],
    "pin": [P(PIN), P(PIN, "fill"), P("M12 13.5V20")],
    "unpin": [P(PIN), P("M12 13.5V20"), P("M4 4L20 20")],
    # navigation
    "chevron-up": [P("M7 14.5L12 9.5L17 14.5")],
    "chevron-down": [P("M7 9.5L12 14.5L17 9.5")],
    "chevron-left": [P("M14.5 7L9.5 12L14.5 17")],
    "chevron-right": [P("M9.5 7L14.5 12L9.5 17")],
    # time
    "clock": [C(12, 12, 8.5), C(12, 12, 8.5, "fill"), P("M12 7.5V12L15 14")],
    "calendar": [R(3.5, 5.5, 17, 15, 1.5), R(3.5, 5.5, 17, 4.5, 1.5, "fill"), P("M3.5 10H20.5"),
                 P("M8 3.5V7"), P("M16 3.5V7"), R(14, 13.5, 3, 3, 0.5, "solid")],
}

TITLES = {k: k.replace("-", " ").capitalize() for k in ICONS}


def render(name, elems, sw, duo):
    out = [f'<svg xmlns="http://www.w3.org/2000/svg" width="24" height="24" viewBox="0 0 24 24">',
           f'<title>{TITLES[name]}</title>',
           f'<g fill="none" stroke="currentColor" stroke-width="{sw}" stroke-linecap="round" stroke-linejoin="round">']
    # fills first so outlines sit on top
    order = [e for e in elems if e[2] == "fill"] + [e for e in elems if e[2] != "fill"]
    for tag, attrs, role in order:
        a = " ".join(f'{k}="{v}"' for k, v in attrs.items())
        if role == "stroke":
            out.append(f"<{tag} {a}/>")
        elif role == "fill":
            out.append(f'<{tag} class="glaze-fill" {a} fill="currentColor" fill-opacity="{"0.16" if duo else "0"}" stroke="none"/>')
        elif role == "solid":
            out.append(f'<{tag} {a} fill="currentColor" stroke="none"/>')
        elif role == "faint":  # inactive part of a level glyph
            out.append(f'<{tag} {a} stroke-opacity="0.35"/>')
        elif role == "badge":
            out.append(f'<{tag} class="glaze-badge" {a} fill="currentColor" stroke="none"/>')
    out.append("</g></svg>")
    return "\n".join(out) + "\n"


def main():
    for d, sw, duo in (("stroke-1.5", "1.5", False), ("stroke-1.25", "1.25", False), ("duo-1.5", "1.5", True)):
        os.makedirs(os.path.join(HERE, d), exist_ok=True)
        for name, elems in ICONS.items():
            with open(os.path.join(HERE, d, name + ".svg"), "w") as fh:
                fh.write(render(name, elems, sw, duo))
    print(f"wrote {len(ICONS)} glyphs x 3 variants")


if __name__ == "__main__":
    main()
