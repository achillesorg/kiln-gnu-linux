#!/usr/bin/env python3
"""Generate the Kiln ASCII logo (nested paper arches) for fastfetch.

Samples the real logo geometry (brand/gen_kiln_logo.py: OUTER/INNER arch paths) on a
character grid. Output: logo.txt using fastfetch colour placeholders:
  $1 = terracotta (outer arch), $2 = cream (inner arch).
Deterministic, stdlib only.
"""
import math
import os

COLS, ROWS = 36, 19
X0, X1, Y0, Y1 = 28, 228, 20, 228  # logo bounds in the 256-unit symbol space


def arch(x, y, left, right, cx, cy, r):
    """True if (x, y) is inside an arch: semicircle on top, straight sides below."""
    if y >= cy:
        return left <= x <= right and y <= 228
    return (x - cx) ** 2 + (y - cy) ** 2 <= r * r


def outer_ring(x, y):
    return arch(x, y, 28, 228, 128, 120, 100) and not arch(x, y, 60, 196, 128, 120, 68)


def inner_ring(x, y):
    return arch(x, y, 82, 174, 128, 124, 46) and not arch(x, y, 110, 146, 128, 128, 18)


def build():
    lines = []
    for r in range(ROWS):
        y = Y0 + (r + 0.5) * (Y1 - Y0) / ROWS
        out, cur = "", None
        for c in range(COLS):
            x = X0 + (c + 0.5) * (X1 - X0) / COLS
            if outer_ring(x, y):
                tag, ch = "$1", "█"
            elif inner_ring(x, y):
                tag, ch = "$2", "█"
            else:
                tag, ch = None, " "
            if tag and tag != cur:
                out += tag
                cur = tag
            out += ch
        lines.append(out.rstrip())
    return "\n".join(lines) + "\n"


if __name__ == "__main__":
    here = os.path.dirname(os.path.abspath(__file__))
    with open(os.path.join(here, "logo.txt"), "w", encoding="utf-8") as f:
        f.write(build())
    print("wrote logo.txt")
