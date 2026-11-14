#!/usr/bin/env bash
#
# Glaze — capture the taskbar states used for review sheets, inside the VM
# (1280x800 scale 1 assumed; coordinates are for the default bottom bar).
# Writes /tmp/glaze-cap-{full,startmenu,group-chooser,task-menu}.png with grim
# (exact frames). Drives real input via scripts/vm/input.sh.
#
# Usage (VM): bash scripts/vm/capture-taskbar.sh FOOT_X THUNAR_X

set -euo pipefail
cd "$(dirname "${BASH_SOURCE[0]}")"
. ./swayenv.sh
FOOT_X="${1:-285}"
OTHER_X="${2:-560}"
inp() { bash ./input.sh "$@"; }

swaymsg -q "[app_id=foot title=foot] focus" || true
inp move 640 300 sleep 1
grim -t png /tmp/glaze-cap-full.png
inp click 44 782 sleep 1.3
grim -t png /tmp/glaze-cap-startmenu.png
inp key Escape sleep 0.6
inp move 640 300 sleep 0.3 click "$FOOT_X" 782 sleep 1.3
grim -t png /tmp/glaze-cap-group-chooser.png
inp click "$FOOT_X" 782 sleep 0.6 rclick "$OTHER_X" 782 sleep 0.8 move 450 690 move 452 692 sleep 1.3
grim -t png /tmp/glaze-cap-task-menu.png
inp key Escape sleep 0.3 key Escape sleep 0.3 move 640 300
