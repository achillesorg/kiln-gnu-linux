#!/usr/bin/env python3
"""Fail if any Glaze art SVG source uses a filter, blur or colour ramp.

Usage (from the repo root):
    python3 assets/wallpapers/tools/lint-svg.py            # v2 wallpapers, ink icons, plymouth sources
    python3 assets/wallpapers/tools/lint-svg.py PATH ...   # explicit files or directories
    python3 assets/wallpapers/tools/lint-svg.py --legacy   # also the v1 wallpapers (they fail: blurred shadows)

Rules: no <filter>, no fe* primitives, no 'blur' anywhere, no linear/radial
ramps (<linearGradient>, <radialGradient>, url(#...gradient), CSS gradient()).
Stdlib only. Exit 1 on any violation.
"""
import os
import re
import sys

ROOT = os.path.normpath(os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "..", ".."))
DEFAULT = ["assets/wallpapers", "assets/icons/ink", "packaging/kilnlinux/plymouth"]
LEGACY = {"paper-hills-light-desktop.svg", "paper-hills-light-mobile.svg", "paper-hills-dark-desktop.svg",
          "paper-hills-dark-mobile.svg", "terracotta-waves-light-desktop.svg", "terracotta-waves-light-mobile.svg",
          "terracotta-waves-dark-desktop.svg", "terracotta-waves-dark-mobile.svg"}
RULES = [
    (re.compile(r"<\s*filter\b", re.I), "<filter> element"),
    (re.compile(r"<\s*fe[A-Z]\w*", re.I), "filter primitive"),
    (re.compile(r"\bfilter\s*[:=]", re.I), "filter property"),
    (re.compile(r"blur", re.I), "blur"),
    (re.compile(r"gradient", re.I), "gradient"),
]


def files(args):
    for a in args:
        p = a if os.path.isabs(a) else os.path.join(ROOT, a)
        if os.path.isdir(p):
            for d, _, fs in os.walk(p):
                for f_ in sorted(fs):
                    if f_.endswith(".svg"):
                        yield os.path.join(d, f_)
        elif os.path.isfile(p):
            yield p


def main():
    argv = [a for a in sys.argv[1:] if a != "--legacy"]
    legacy = "--legacy" in sys.argv
    bad, n = 0, 0
    for path in files(argv or DEFAULT):
        if not legacy and not argv and os.path.basename(path) in LEGACY:
            continue
        n += 1
        text = open(path, encoding="utf-8").read()
        for rx, what in RULES:
            m = rx.search(text)
            if m:
                line = text.count("\n", 0, m.start()) + 1
                print(f"FAIL {os.path.relpath(path, ROOT)}:{line}: {what}")
                bad += 1
    print(f"lint-svg: {n} file(s) checked, {bad} violation(s)")
    sys.exit(1 if bad else 0)


if __name__ == "__main__":
    main()
