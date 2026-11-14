#!/usr/bin/env python3
"""Kiln GNU/Linux favicon, app-icon and web-icon set (two-tone, grid-exact).

Runs after gen_kiln_logo.py and the logo-design skill's export_variants.py
(which writes the one-colour SVG variants into kiln/web/). This script
replaces its mono app icon and rescaled favicon with two-tone versions:

  web/favicon.svg            small cut, ink inner arch turns cream in dark mode
  web/favicon-{16,32,48}.png small cut at 16/32 (pixel grid), master at 48
  web/favicon.ico            16/32/48
  web/kiln-app-icon.svg      ink rounded tile, terracotta + cream arches
  web/kiln-app-icon-light.svg cream rounded tile, terracotta + ink arches
  web/kiln-app-icon-maskable.svg full-bleed tile, mark inside the 80% safe zone
  web/apple-touch-icon.png (180), icon-192.png, icon-512.png, maskable-512.png
  app-icon/kiln-<size>.png   16..512 for a hicolor 'apps' icon (16 uses small cut)

Needs rsvg-convert and ImageMagick (`magick`) for the .ico. Stdlib only.
"""
import os
import subprocess
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from gen_kiln_logo import TERRA, INK, CREAM, symbol, svg  # noqa: E402

KILN = os.path.expanduser("~/fedeos/brand/kiln")
WEB = os.path.join(KILN, "web")
APP = os.path.join(KILN, "app-icon")
os.makedirs(WEB, exist_ok=True)
os.makedirs(APP, exist_ok=True)

# The master symbol's ink sits in x 28..228, y 20..228: centre (128, 124).
CX, CY = 128, 124


def placed(scale, outer, inner, small=False, cy=128):
    tx, ty = 128 - CX * scale, cy - CY * scale
    if small:  # small cut is already centred (16..240 both ways)
        tx = ty = 128 - 128 * scale
    return (f'<g transform="translate({tx:.2f} {ty:.2f}) scale({scale})">'
            f'{symbol(outer, inner, small)}</g>')


def write(path, text):
    with open(path, "w") as f:
        f.write(text)


def render(src, dst, size):
    subprocess.run(["rsvg-convert", "-w", str(size), "-h", str(size), src, "-o", dst], check=True)


# favicon.svg: the grid-exact small cut, inner arch flips for dark browser chrome.
fav = ('<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 256 256" width="256" height="256">'
       '<title>Kiln GNU/Linux</title>'
       '<style>.i{fill:%s}@media (prefers-color-scheme:dark){.i{fill:%s}}</style>'
       '<path fill="%s" fill-rule="evenodd" d="M16 240V128a112 112 0 0 1 224 0v112zM48 240V128a80 80 0 0 1 160 0v112z"/>'
       '<path class="i" fill-rule="evenodd" d="M80 240V128a48 48 0 0 1 96 0v112zM112 240V128a16 16 0 0 1 32 0v112z"/>'
       '</svg>\n') % (INK, CREAM, TERRA)
write(os.path.join(WEB, "favicon.svg"), fav)

tile = '<rect x="8" y="8" width="240" height="240" rx="56" fill="%s"/>'
write(os.path.join(WEB, "kiln-app-icon.svg"),
      svg(256, 256, tile % INK + placed(0.66, TERRA, CREAM), "Kiln GNU/Linux app icon"))
write(os.path.join(WEB, "kiln-app-icon-light.svg"),
      svg(256, 256, tile % CREAM + '<rect x="8.5" y="8.5" width="239" height="239" rx="55.5" fill="none" stroke="#E4DCCB"/>'
          + placed(0.66, TERRA, INK), "Kiln GNU/Linux app icon (light)"))
write(os.path.join(WEB, "kiln-app-icon-maskable.svg"),
      svg(256, 256, f'<rect width="256" height="256" fill="{INK}"/>' + placed(0.56, TERRA, CREAM),
          "Kiln GNU/Linux maskable icon"))
write(os.path.join(WEB, "kiln-apple-touch.svg"),
      svg(256, 256, f'<rect width="256" height="256" fill="{INK}"/>' + placed(0.64, TERRA, CREAM),
          "Kiln GNU/Linux apple touch icon"))
# small-cut tile for 16/24 px app icons (no tile: the tile eats 2 px at 16 px)
write(os.path.join(WEB, "kiln-app-icon-small.svg"),
      svg(256, 256, symbol(TERRA, INK, True), "Kiln GNU/Linux app icon, 16 px cut"))

small = os.path.join(KILN, "kiln-symbol-small.svg")
master = os.path.join(KILN, "kiln-symbol.svg")
render(small, os.path.join(WEB, "favicon-16.png"), 16)
render(small, os.path.join(WEB, "favicon-32.png"), 32)
render(master, os.path.join(WEB, "favicon-48.png"), 48)
subprocess.run(["magick", os.path.join(WEB, "favicon-16.png"), os.path.join(WEB, "favicon-32.png"),
                os.path.join(WEB, "favicon-48.png"), os.path.join(WEB, "favicon.ico")], check=True)
render(os.path.join(WEB, "kiln-apple-touch.svg"), os.path.join(WEB, "apple-touch-icon.png"), 180)
render(os.path.join(WEB, "kiln-app-icon.svg"), os.path.join(WEB, "icon-192.png"), 192)
render(os.path.join(WEB, "kiln-app-icon.svg"), os.path.join(WEB, "icon-512.png"), 512)
render(os.path.join(WEB, "kiln-app-icon-maskable.svg"), os.path.join(WEB, "maskable-512.png"), 512)
for n in (16, 24, 32, 48, 64, 128, 256, 512):
    src = os.path.join(WEB, "kiln-app-icon-small.svg" if n <= 16 else "kiln-app-icon.svg")
    render(src, os.path.join(APP, f"kiln-{n}.png"), n)
print("ok")
