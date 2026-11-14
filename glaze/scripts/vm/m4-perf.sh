#!/usr/bin/env bash
# Glaze M4 frame-time run: restart the tty2 session (optional env for the
# compositor via ~/.config/glaze/tty2-cmd), open Thunar, idle, drag its title
# for 8 s, drag-snap it left, then print the compositor's frame-time lines.
# Usage: m4-perf.sh LABEL [ENV=VAL ...]
set -uo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"; S="$ROOT/scripts/vm"; VM="$ROOT/../vm"
LABEL=$1; shift
if [ $# -gt 0 ]; then "$VM" "echo '$* glaze-session run --backend glaze' > ~/.config/glaze/tty2-cmd"; else "$VM" "rm -f ~/.config/glaze/tty2-cmd"; fi
"$S/m4-check.sh" start >/dev/null 2>&1
"$S/m4-run.sh" thunar >/dev/null
trap '"$S/chvt.sh" 1 >/dev/null' EXIT
"$S/chvt.sh" 2 >/dev/null; sleep 10
"$VM" "echo MARK-idle >> ~/session-tty2.log"; sleep 6
"$VM" "echo MARK-drag >> ~/session-tty2.log"; "$S/m4-drag.sh" 500 14 8; sleep 1
"$VM" "echo MARK-snap >> ~/session-tty2.log"
python3 "$S/qmp_pointer.py" hold-drag 500 14 2 400 0.8 >/dev/null; sleep 6
echo "== $LABEL"
"$VM" "grep -E 'frame-time' ~/session-tty2.log | sed 's/T\([0-9:]*\)\..*frames=/ \1 frames=/' | cut -c1-150"
"$VM" "rm -f ~/.config/glaze/tty2-cmd"
