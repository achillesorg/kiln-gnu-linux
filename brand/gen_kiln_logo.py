#!/usr/bin/env python3
"""Generate Kiln GNU/Linux logo files (concept C: nested paper arches).

Output: ~/fedeos/brand/kiln/   (deterministic, stdlib only, no live text)

v2 (2026-11-11, agent C):
  * lockup gets a right margin equal to the left one (audit: R margin was 0);
  * 'k' arm snapped to a true 45 degrees (audit: near-miss angles 136/136.5 deg);
  * kiln-symbol-small.svg: a 16 px cut drawn on a 16-unit pixel grid
    (the master's 18-unit counter and 22-unit gap smear below 24 px);
  * kiln-lockup-full*.svg: symbol + 'kiln' + smaller 'GNU/Linux', all paths.
"""
import math
import os

OUT = os.path.expanduser("~/fedeos/brand/kiln")
os.makedirs(OUT, exist_ok=True)

TERRA, INK, CREAM, DARK = "#D97757", "#2B2926", "#FAF9F5", "#1F1E1C"

OUTER = "M28 228V120a100 100 0 0 1 200 0v108zM60 228V120a68 68 0 0 1 136 0v108z"
INNER = "M82 228V124a46 46 0 0 1 92 0v104zM110 228V128a18 18 0 0 1 36 0v100z"

# 16 px cut: every edge on a whole pixel at 16x16 (1 px = 16 units here).
# columns: 1 | ring 2 | gap 2 | leg 2 | counter 2 | leg 2 | gap 2 | ring 2 | 1
OUTER_SMALL = "M16 240V128a112 112 0 0 1 224 0v112zM48 240V128a80 80 0 0 1 160 0v112z"
INNER_SMALL = "M80 240V128a48 48 0 0 1 96 0v112zM112 240V128a16 16 0 0 1 32 0v112z"


def symbol(outer, inner, small=False):
    o, i = (OUTER_SMALL, INNER_SMALL) if small else (OUTER, INNER)
    return (f'<path fill="{outer}" fill-rule="evenodd" d="{o}"/>'
            f'<path fill="{inner}" fill-rule="evenodd" d="{i}"/>')


def rect(x, y, w, h):  # clockwise subpath
    return f'M{x} {y}h{w}v{h}h-{w}z'


def wordmark(x0, fill):
    """'kiln' built from rectangles/paths (no live text). Baseline 228, x-height top 118, ascender 52."""
    k = x0
    i = k + 130 + 24
    l = i + 28 + 24
    n = l + 28 + 24
    # each letter is ONE path (subpaths all clockwise, nonzero fill) so no seams.
    # k arm: upper edge (k+28,178)->(k+88,118) and lower edge (k+126,118)->(k+68,176)
    # are both exactly 45 degrees.
    parts = [
        '<path d="' + rect(k, 52, 28, 176)
        + f'M{k+28} 178L{k+88} 118H{k+126}L{k+68} 176L{k+130} 228H{k+90}L{k+44} 188L{k+28} 204Z"/>',
        f'<path d="{rect(i, 118, 28, 110)}"/>',
        f'<circle cx="{i+14}" cy="80" r="15"/>',
        f'<path d="{rect(l, 52, 28, 176)}"/>',
        '<path d="' + rect(n, 118, 28, 110) + rect(n + 72, 168, 28, 60)
        + f'M{n} 168A50 50 0 0 1 {n+100} 168H{n+72}A22 22 0 0 0 {n+28} 168Z"/>',
    ]
    return f'<g fill="{fill}">' + "".join(parts) + "</g>"


# ---- 'GNU/Linux' as geometric paths -------------------------------------
# Cap height 72 (top 156, baseline 228), stroke 12, x-height 50.
B, CAP, S, XH = 228, 72, 12, 50
T = B - CAP


def _f(v):
    return ("%.2f" % v).rstrip("0").rstrip(".")


def _pt(cx, cy, r, deg):
    a = math.radians(deg)
    return cx + r * math.cos(a), cy + r * math.sin(a)


def annular_sector(cx, cy, R, r, a0, a1):
    """Filled ring sector from angle a0 to a1 (degrees, screen coords, clockwise)."""
    span = (a1 - a0) % 360
    large = 1 if span > 180 else 0
    x0, y0 = _pt(cx, cy, R, a0)
    x1, y1 = _pt(cx, cy, R, a1)
    x2, y2 = _pt(cx, cy, r, a1)
    x3, y3 = _pt(cx, cy, r, a0)
    return (f"M{_f(x0)} {_f(y0)}A{_f(R)} {_f(R)} 0 {large} 1 {_f(x1)} {_f(y1)}"
            f"L{_f(x2)} {_f(y2)}A{_f(r)} {_f(r)} 0 {large} 0 {_f(x3)} {_f(y3)}Z")


def gnu_linux(x0, fill):
    out, x = [], x0
    # G: ring sector open at the upper right, plus a bar into the bowl.
    R = CAP / 2
    cx, cy = x + R, T + R
    out.append(annular_sector(cx, cy, R, R - S, 0, 318))
    out.append(rect(_f(cx + 4), _f(cy), _f(R - 4), S))
    x += CAP + 10
    # N
    W = 58
    out.append(rect(x, T, S, CAP) + rect(x + W - S, T, S, CAP))
    a = W - CAP / math.tan(math.radians(60))  # diagonal at exactly 60 degrees
    out.append(f"M{x} {T}H{_f(x+a)}L{x+W} {B}H{_f(x+W-a)}Z")
    x += W + 10
    # U
    W = 56
    R2 = W / 2
    out.append(f"M{x} {T}H{x+S}V{_f(B-R2)}A{_f(R2-S)} {_f(R2-S)} 0 0 0 {x+W-S} {_f(B-R2)}"
               f"V{T}H{x+W}V{_f(B-R2)}A{_f(R2)} {_f(R2)} 0 0 1 {x} {_f(B-R2)}Z")
    x += W + 6
    # slash
    run = CAP / math.tan(math.radians(75))  # slash at exactly 75 degrees
    out.append(f"M{x} {B}H{x+S+2}L{_f(x+S+2+run)} {T}H{_f(x+run)}Z")
    x = round(x + S + 2 + run + 8)
    # L
    out.append(rect(x, T, S, CAP) + rect(x, B - S, 42, S))
    x += 42 + 8
    # i (stem + round dot)
    out.append(rect(x, B - XH, S, XH))
    dx = x + S / 2
    out.append(f"M{_f(dx+7)} {T+7}A7 7 0 1 1 {_f(dx-7)} {T+7}A7 7 0 1 1 {_f(dx+7)} {T+7}Z")
    x += S + 10
    # n
    W = 46
    R3 = W / 2
    top = B - XH
    out.append(rect(x, top, S, XH) + rect(x + W - S, _f(top + R3), S, _f(XH - R3)))
    out.append(f"M{x} {_f(top+R3)}A{_f(R3)} {_f(R3)} 0 0 1 {x+W} {_f(top+R3)}"
               f"H{x+W-S}A{_f(R3-S)} {_f(R3-S)} 0 0 0 {x+S} {_f(top+R3)}Z")
    x += W + 10
    # u (n turned 180 degrees)
    out.append(rect(x + W - S, top, S, XH) + rect(x, top, S, _f(XH - R3)))
    out.append(f"M{x} {_f(B-R3)}H{x+S}A{_f(R3-S)} {_f(R3-S)} 0 0 0 {x+W-S} {_f(B-R3)}"
               f"H{x+W}A{_f(R3)} {_f(R3)} 0 0 1 {x} {_f(B-R3)}Z")
    x += W + 10
    # x
    W = 48
    out.append(f"M{x} {top}H{x+S+2}L{x+W} {B}H{x+W-S-2}Z")
    out.append(f"M{x+W} {top}H{x+W-S-2}L{x} {B}H{x+S+2}Z")
    x += W
    body = "".join(f'<path d="{d}"/>' for d in out)
    return f'<g fill="{fill}">{body}</g>', x


def svg(w, h, body, title, bg=None):
    r = f'<rect width="{w}" height="{h}" fill="{bg}"/>' if bg else ""
    return (f'<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {w} {h}" width="{w}" height="{h}">'
            f'<title>{title}</title>{r}{body}</svg>\n')


def write(name, content):
    with open(os.path.join(OUT, name), "w") as f:
        f.write(content)


W = 686  # lockup width: 28 left margin (inside the symbol box) + 28 right margin
GL_X = 694  # 'GNU/Linux' starts 36 units after 'kiln'
_, GL_END = gnu_linux(GL_X, INK)
WF = int(GL_END + 28)  # full-name lockup width

if __name__ == "__main__":
    # symbols
    write("kiln-symbol.svg", svg(256, 256, symbol(TERRA, INK), "Kiln GNU/Linux symbol"))
    write("kiln-symbol-on-dark.svg", svg(256, 256, symbol(TERRA, CREAM), "Kiln GNU/Linux symbol (for dark backgrounds)"))
    write("kiln-symbol-mono-black.svg", svg(256, 256, symbol("#000", "#000"), "Kiln symbol, one colour black"))
    write("kiln-symbol-mono-white.svg", svg(256, 256, symbol("#fff", "#fff"), "Kiln symbol, one colour white"))
    # small cut (use at 24 px and below)
    write("kiln-symbol-small.svg", svg(256, 256, symbol(TERRA, INK, True), "Kiln GNU/Linux symbol, small-size cut"))
    write("kiln-symbol-small-on-dark.svg", svg(256, 256, symbol(TERRA, CREAM, True), "Kiln symbol, small cut, dark backgrounds"))
    write("kiln-symbol-small-mono-black.svg", svg(256, 256, symbol("#000", "#000", True), "Kiln symbol, small cut, one colour black"))
    write("kiln-symbol-small-mono-white.svg", svg(256, 256, symbol("#fff", "#fff", True), "Kiln symbol, small cut, one colour white"))
    # lockups
    write("kiln-lockup.svg", svg(W, 256, symbol(TERRA, INK) + wordmark(300, INK), "Kiln GNU/Linux lockup"))
    write("kiln-lockup-on-dark.svg", svg(W, 256, symbol(TERRA, CREAM) + wordmark(300, CREAM), "Kiln GNU/Linux lockup (dark backgrounds)"))
    write("kiln-lockup-mono-black.svg", svg(W, 256, symbol("#000", "#000") + wordmark(300, "#000"), "Kiln lockup, one colour black"))
    write("kiln-lockup-mono-white.svg", svg(W, 256, symbol("#fff", "#fff") + wordmark(300, "#fff"), "Kiln lockup, one colour white"))
    # full-name lockups
    for name, outer, inner, word, title in [
        ("kiln-lockup-full.svg", TERRA, INK, INK, "Kiln GNU/Linux full-name lockup"),
        ("kiln-lockup-full-on-dark.svg", TERRA, CREAM, CREAM, "Kiln GNU/Linux full-name lockup (dark backgrounds)"),
        ("kiln-lockup-full-mono-black.svg", "#000", "#000", "#000", "Kiln GNU/Linux full-name lockup, one colour black"),
        ("kiln-lockup-full-mono-white.svg", "#fff", "#fff", "#fff", "Kiln GNU/Linux full-name lockup, one colour white"),
    ]:
        gl, _ = gnu_linux(GL_X, word)
        write(name, svg(WF, 256, symbol(outer, inner) + wordmark(300, word) + gl, title))

    # stacked full-name lockup: symbol, then 'kiln', then 'GNU/Linux', centred
    GLW = GL_END - GL_X
    WS = int(GLW + 56)
    for name, outer, inner, word, title in [
        ("kiln-lockup-stacked.svg", TERRA, INK, INK, "Kiln GNU/Linux stacked lockup"),
        ("kiln-lockup-stacked-on-dark.svg", TERRA, CREAM, CREAM, "Kiln GNU/Linux stacked lockup (dark backgrounds)"),
        ("kiln-lockup-stacked-mono-black.svg", "#000", "#000", "#000", "Kiln GNU/Linux stacked lockup, one colour black"),
        ("kiln-lockup-stacked-mono-white.svg", "#fff", "#fff", "#fff", "Kiln GNU/Linux stacked lockup, one colour white"),
    ]:
        gl, _ = gnu_linux(GL_X, word)
        body = (f'<g transform="translate({_f((WS-256)/2)} 0)">{symbol(outer, inner)}</g>'
                f'<g transform="translate({_f((WS-358)/2-300)} 216)">{wordmark(300, word)}</g>'
                f'<g transform="translate({_f(28-GL_X)} 320)">{gl}</g>')
        write(name, svg(WS, 576, body, title))

    # boot splash 1920x1080: lockup centred, progress track below
    def boot(bg, inner, word, track, fill):
        x = (1920 - W) // 2
        body = (f'<g transform="translate({x} 372)">{symbol(TERRA, inner)}{wordmark(300, word)}</g>'
                f'<rect x="780" y="720" width="360" height="4" rx="2" fill="{track}"/>'
                f'<rect x="780" y="720" width="108" height="4" rx="2" fill="{fill}"/>')
        return svg(1920, 1080, body, "Kiln GNU/Linux boot splash", bg)

    write("kiln-boot-dark.svg", boot(DARK, CREAM, CREAM, "#3A3733", TERRA))
    write("kiln-boot-light.svg", boot(CREAM, INK, INK, "#E4DCCB", TERRA))
    print("ok", WF, sorted(os.listdir(OUT)))
