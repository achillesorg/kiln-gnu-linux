#!/usr/bin/env bash
# Glaze M4: open/close frame times. Restarts the tty2 session (optional env
# for the session via ~/.config/glaze/tty2-cmd), opens Thunar, then opens and
# closes foot 6 times (Super+Shift+Q), and prints the frame-time lines of
# that period. Always ends on tty1.  Usage: m4-openclose.sh LABEL [ENV=VAL ...]
set -uo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"; S="$ROOT/scripts/vm"; VM="$ROOT/../vm"
LABEL=$1; shift
if [ $# -gt 0 ]; then "$VM" "echo '$* glaze-session run --backend glaze' > ~/.config/glaze/tty2-cmd"; else "$VM" "rm -f ~/.config/glaze/tty2-cmd"; fi
trap '"$S/chvt.sh" 1 >/dev/null; "$VM" "rm -f ~/.config/glaze/tty2-cmd"' EXIT
"$S/m4-check.sh" stay start >/dev/null 2>&1
sleep 15
"$S/m4-run.sh" thunar >/dev/null; sleep 8
t0=$("$VM" 'date -u +%H:%M:%S')
for i in 1 2 3 4 5 6; do
    "$S/m4-run.sh" "foot --title oc$i" >/dev/null; sleep 2.5
    python3 "$S/qmp_pointer.py" key meta_l-shift-q >/dev/null; sleep 1.5
done
sleep 5
echo "== $LABEL (from $t0)"
"$VM" "grep -E 'frame-time' ~/session-tty2.log | sed 's/.*T\([0-9:]*\)\..*frames=/\1 frames=/' | awk '\$1 >= \"$t0\"' | cut -c1-140"
