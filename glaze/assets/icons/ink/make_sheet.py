#!/usr/bin/env python3
"""Contact sheet of the ink glyphs at 16/24/32/48 px, light and dark, per variant.

    python3 assets/icons/ink/make_sheet.py   ->  assets/icons/ink/contact-sheet.png
Needs rsvg-convert. Stdlib only.
"""
import os
import re
import subprocess

HERE = os.path.dirname(os.path.abspath(__file__))
SIZES = (16, 24, 32, 48)
THEMES = (("light", "#FAF9F5", "#2B2926"), ("dark", "#1F1E1C", "#F1EBE0"))
VARS = ("stroke-1.5", "stroke-1.25", "duo-1.5")


def inner(path):
    t = open(path).read()
    t = re.sub(r"<title>.*?</title>", "", t)
    return t[t.index(">", t.index("<svg")) + 1: t.rindex("</svg>")]


def main():
    names = sorted(f[:-4] for f in os.listdir(os.path.join(HERE, VARS[0])) if f.endswith(".svg"))
    label_w, cell = 130, 16 + 24 + 32 + 48 + 4 * 12
    col_w = label_w + cell
    cols = 2  # light | dark for each variant block
    rows = len(names)
    row_h = 56
    block_w = cols * col_w + 30
    W = 20 + len(VARS) * block_w
    H = 70 + rows * row_h + 20
    out = [f'<svg xmlns="http://www.w3.org/2000/svg" width="{W}" height="{H}" viewBox="0 0 {W} {H}">',
           f'<rect width="{W}" height="{H}" fill="#ECE6DA"/>',
           f'<text x="20" y="28" font-family="sans-serif" font-size="16" fill="#2B2926">'
           f'Glaze ink glyphs ({len(names)}) at 16 / 24 / 32 / 48 px, rendered 1:1</text>']
    for vi, var in enumerate(VARS):
        bx = 20 + vi * block_w
        out.append(f'<text x="{bx}" y="54" font-family="sans-serif" font-size="13" fill="#2B2926">{var}</text>')
        for ti, (tname, bg, fg) in enumerate(THEMES):
            x0 = bx + ti * col_w
            out.append(f'<rect x="{x0}" y="62" width="{col_w - 6}" height="{rows * row_h + 8}" fill="{bg}"/>')
            for ri, n in enumerate(names):
                y = 66 + ri * row_h
                out.append(f'<text x="{x0 + 8}" y="{y + 30}" font-family="sans-serif" font-size="11" '
                           f'fill="{fg}">{n}</text>')
                x = x0 + label_w
                body = inner(os.path.join(HERE, var, n + ".svg"))
                for s in SIZES:
                    out.append(f'<svg x="{x}" y="{y + (48 - s) // 2 + 2}" width="{s}" height="{s}" '
                               f'viewBox="0 0 24 24" style="color:{fg}">{body}</svg>')
                    x += s + 12
    out.append("</svg>")
    subprocess.run(["rsvg-convert", "-o", os.path.join(HERE, "contact-sheet.png")],
                   input="\n".join(out).encode(), check=True)
    print("wrote contact-sheet.png")


if __name__ == "__main__":
    main()
