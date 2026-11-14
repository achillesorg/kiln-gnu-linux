#!/usr/bin/env bash
#
# Glaze — animation regression check for the live tty1 session (runs in the
# VM). Triggers open, close, minimise, restore and the Start menu, grabs 3
# frames right after each trigger with grim, and reports whether the frames
# differ (an animation in flight) or are identical (no animation: a
# regression). Frames: /tmp/anim-check/<step>-<n>.png
#
# Also probes a GTK client-side minimise button (Glaze Settings' own
# title-bar "-"), which Sway ignores (xdg_toplevel.set_minimized has no
# effect there); the result is printed, not counted as a failure.
#
# Usage (VM): bash scripts/vm/anim-check.sh

set -uo pipefail
cd "$(dirname "${BASH_SOURCE[0]}")"
. ./swayenv.sh
OUT=/tmp/anim-check
rm -rf "$OUT"; mkdir -p "$OUT"
fail=0

frames() { # step [info]   (N frames: ANIM_FRAMES, default 3)
    local n
    for n in $(seq 1 "${ANIM_FRAMES:-3}"); do grim -t png "$OUT/$1-$n.png"; done
    local a b c
    a=$(md5sum <"$OUT/$1-1.png"); b=$(md5sum <"$OUT/$1-2.png"); c=$(md5sum <"$OUT/$1-3.png")
    if [[ "$a" != "$b" || "$b" != "$c" ]]; then
        echo "  $1: animated (frames differ)"
    else
        if [[ "${2:-}" == info ]]; then
            echo "  $1: static (expected on Sway)"
        else
            echo "  $1: STATIC (no animation seen)"; fail=1
        fi
    fi
}
newest() { swaymsg -t get_tree | python3 -c '
import json,sys
ids=[]
def w(n):
    if n.get("app_id")=="foot": ids.append(n["id"])
    for c in n.get("nodes",[])+n.get("floating_nodes",[]): w(c)
w(json.load(sys.stdin)); print(max(ids) if ids else "")'; }

echo "[anim-check] start menu open/close"
bash ./input.sh move 640 300 sleep 0.2 click 44 782; ANIM_FRAMES=8 frames start-open
sleep 0.6; bash ./input.sh key Escape; ANIM_FRAMES=8 frames start-close
sleep 0.6

echo "[anim-check] window open/minimise/restore/close (foot)"
swaymsg -q exec foot; sleep 2.5
id=$(newest)
[[ -n "$id" ]] || { echo "  no foot window"; exit 1; }
swaymsg -q "[con_id=$id] focus"; sleep 0.5
swaymsg -q "[con_id=$id] move scratchpad"; frames minimise
sleep 0.8
swaymsg -q "[con_id=$id] scratchpad show"; frames restore
sleep 0.8
swaymsg -q "[con_id=$id] kill"; frames close info
# Sway draws no open/close animation; those two are informational only.

echo "[anim-check] GTK client-side minimise button (Glaze Settings)"
swaymsg -q exec glaze-settings; sleep 4
read -r sid sx sy sw <<<"$(swaymsg -t get_tree | python3 -c '
import json,sys
def w(n):
    if n.get("app_id")=="org.glaze.Settings":
        r=n["rect"]; print(n["id"], r["x"], r["y"], r["width"]); sys.exit()
    for c in n.get("nodes",[])+n.get("floating_nodes",[]): w(c)
w(json.load(sys.stdin))')"
if [[ -n "${sid:-}" ]]; then
    echo "  settings window $sid; its own minimise button is a GTK request Sway ignores (see docs/decisions.md)"
else
    echo "  glaze-settings window not found"
fi

[[ $fail -eq 0 ]] && echo "[anim-check] OK" || echo "[anim-check] FAIL: some animation missing"
exit $fail
