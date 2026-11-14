#!/usr/bin/env python3
"""Validate the Glaze ink glyphs. Exit 1 on any violation.

    python3 assets/icons/ink/check_icons.py

Checks every SVG under stroke-1.5/, stroke-1.25/ and duo-1.5/:
  * root is <svg> with width=height=24 and viewBox "0 0 24 24", has a <title>;
  * paint is only currentColor or none (no hex colours, no url() paint);
  * the group sets round caps and joins and the variant's stroke width;
  * no filter / blur / ramp / text / style / image / script elements;
  * path data uses absolute commands only and every number sits on the
    0.25 px grid; all coordinates stay inside 1..23 (arc radii included);
  * circle / rect / ellipse attributes are on the grid and inside 1..23.
Stdlib only.
"""
import os
import re
import sys
import xml.etree.ElementTree as ET

HERE = os.path.dirname(os.path.abspath(__file__))
NS = "{http://www.w3.org/2000/svg}"
VARIANTS = {"stroke-1.5": "1.5", "stroke-1.25": "1.25", "duo-1.5": "1.5"}
ALLOWED = {"svg", "title", "g", "path", "circle", "rect", "ellipse", "line", "polyline", "polygon"}
NUM = re.compile(r"-?\d*\.?\d+")


def on_grid(v):
    return abs(v * 4 - round(v * 4)) < 1e-6


def check_path(d, err):
    if re.search(r"[a-z]", d):
        err("relative (lowercase) path command")
    # walk commands to tell arc flags/radii from coordinates
    toks = re.findall(r"[A-Za-z]|-?\d*\.?\d+", d)
    cmd, i, args = None, 0, []
    arity = {"M": 2, "L": 2, "H": 1, "V": 1, "C": 6, "S": 4, "Q": 4, "T": 2, "A": 7, "Z": 0}
    for t in toks:
        if t.isalpha():
            cmd = t.upper()
            args = []
            continue
        v = float(t)
        args.append(v)
        n = arity.get(cmd, 2)
        idx = (len(args) - 1) % n if n else 0
        if cmd == "A" and idx in (2, 3, 4):  # rotation, large-arc, sweep flags
            continue
        if not on_grid(v):
            err(f"off-grid number {t} in {cmd}")
        if not (1 <= v <= 23) and not (cmd == "A" and idx in (0, 1) and v <= 22):
            err(f"coordinate {t} outside 1..23 in {cmd}")


def main():
    bad = n = 0
    names = None
    for var, sw in VARIANTS.items():
        d = os.path.join(HERE, var)
        files = sorted(f for f in os.listdir(d) if f.endswith(".svg")) if os.path.isdir(d) else []
        if names is None:
            names = files
        elif files != names:
            print(f"FAIL {var}: glyph list differs from the first variant")
            bad += 1
        for f in files:
            n += 1
            path = os.path.join(d, f)
            problems = []
            err = problems.append
            text = open(path, encoding="utf-8").read()
            if re.search(r"filter|blur|gradient|<style|<text|<image|<script", text, re.I):
                err("forbidden element or keyword")
            for m in re.finditer(r'(?:fill|stroke)="([^"]+)"', text):
                if m.group(1) not in ("currentColor", "none"):
                    err(f"paint {m.group(1)!r} (only currentColor / none)")
            root = ET.fromstring(text)
            if root.tag != NS + "svg" or root.get("viewBox") != "0 0 24 24" \
                    or root.get("width") != "24" or root.get("height") != "24":
                err("root must be a 24x24 svg with viewBox 0 0 24 24")
            if root.find(NS + "title") is None:
                err("missing <title>")
            g = root.find(NS + "g")
            if g is None or g.get("stroke-linecap") != "round" or g.get("stroke-linejoin") != "round" \
                    or g.get("stroke-width") != sw or g.get("stroke") != "currentColor":
                err(f"group must set stroke=currentColor, width {sw}, round caps and joins")
            for el in root.iter():
                tag = el.tag.replace(NS, "")
                if tag not in ALLOWED:
                    err(f"element <{tag}> not allowed")
                if tag == "path":
                    check_path(el.get("d", ""), err)
                for k in ("cx", "cy", "x", "y", "r", "rx", "ry", "width", "height"):
                    if el.get(k) is not None and tag != "svg":
                        v = float(el.get(k))
                        if not on_grid(v):
                            err(f"{tag} {k}={v} off the 0.25 grid")
                        if k in ("cx", "cy", "x", "y") and not 1 <= v <= 23:
                            err(f"{tag} {k}={v} outside 1..23")
                if tag == "rect":
                    x, y = float(el.get("x")), float(el.get("y"))
                    if x + float(el.get("width")) > 23 or y + float(el.get("height")) > 23:
                        err("rect extends outside 1..23")
                if tag in ("circle", "ellipse"):
                    cx, cy = float(el.get("cx")), float(el.get("cy"))
                    rx = float(el.get("r") or el.get("rx"))
                    ry = float(el.get("r") or el.get("ry"))
                    if cx - rx < 1 or cx + rx > 23 or cy - ry < 1 or cy + ry > 23:
                        err(f"{tag} extends outside 1..23")
            for p in sorted(set(problems)):
                print(f"FAIL {var}/{f}: {p}")
            bad += bool(problems)
    print(f"check_icons: {n} file(s), {len(names or [])} glyphs, {bad} failing")
    sys.exit(1 if bad or not n else 0)


if __name__ == "__main__":
    main()
