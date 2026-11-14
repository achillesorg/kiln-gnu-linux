#!/usr/bin/env bash
# Glaze M4: scripted real-input interactions on the tty2 Glaze session with
# screenshots mid-gesture. Always ends on tty1.
# Usage: scripts/vm/m4-interact.sh WxH STEP...   steps:
#   bg "QMP ARGS"   run qmp_pointer.py ARGS in the background
#   fg "QMP ARGS"   run it in the foreground
#   shot NAME | sleep S | log PATTERN
set -uo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
S="$ROOT/scripts/vm"
SIZE="$1"; shift
trap '"$S/chvt.sh" 1 >/dev/null' EXIT
"$S/chvt.sh" 2 >/dev/null
sleep 3
while [[ $# -gt 0 ]]; do
    case "$1" in
        bg) # shellcheck disable=SC2086
            python3 "$S/qmp_pointer.py" --size "$SIZE" $2 & shift ;;
        fg) # shellcheck disable=SC2086
            python3 "$S/qmp_pointer.py" --size "$SIZE" $2; shift ;;
        shot) GLAZE_SHOT_DELAY=0 GLAZE_SHOT_NOGRIM=1 "$S/shot.sh" "$2" M4 | tail -1; shift ;;
        sleep) sleep "$2"; shift ;;
        log) "$ROOT/../vm" "grep -E '$2' ~/session-tty2.log | tail -6 | cut -c12-230"; shift ;;
    esac
    shift
done
wait
