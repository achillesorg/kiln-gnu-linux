#!/usr/bin/env python3
"""Print the sway windows as one line each: id app workspace floating focused visible.
Usage (VM): swaymsg -t get_tree | python3 tree.py"""
import json, sys

def walk(n, ws=None):
    if n.get("type") == "workspace":
        ws = n.get("name")
    if n.get("app_id") or n.get("window_properties"):
        print(f'{n["id"]} app={n.get("app_id")} ws={ws} type={n["type"]} '
              f'focused={n.get("focused")} visible={n.get("visible")} title={n.get("name")!r}')
    for c in n.get("nodes", []) + n.get("floating_nodes", []):
        walk(c, ws)

walk(json.load(sys.stdin))
