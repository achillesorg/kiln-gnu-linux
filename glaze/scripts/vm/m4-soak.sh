#!/usr/bin/env bash
# Glaze M4: soak the tty2 Glaze session for N minutes (default 10) with real
# input every few seconds (drags, snaps, Alt+Tab, minimise/restore, tiling
# toggle), then report compositor uptime, errors and frame-time lines.
# Always ends on tty1.
set -uo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
S="$ROOT/scripts/vm"; VM="$ROOT/../vm"
MIN="${1:-10}"; SIZE="${2:-1792x1344}"
R=/run/user/1000/glaze-tty2
Q() { python3 "$S/qmp_pointer.py" --size "$SIZE" "$@" >/dev/null 2>&1 || echo "  (qmp step failed)"; }
ctl() { "$VM" "python3 -c 'import socket,sys; s=socket.socket(socket.AF_UNIX); s.settimeout(15); s.connect(\"$R/glaze/compositor.sock\"); s.sendall(sys.argv[1].encode()+b\"\\n\"); print(s.makefile().readline().strip())' '$1'" >/dev/null 2>&1; }
trap '"$S/chvt.sh" 1 >/dev/null' EXIT
pid0=$("$VM" "pgrep -u \$(id -u) -f "^glaze-compositor" | head -1")
echo "compositor pid at start: $pid0"
"$S/chvt.sh" 2 >/dev/null; sleep 3
end=$(( $(date +%s) + MIN * 60 )); n=0
while [ "$(date +%s)" -lt "$end" ]; do
    case $(( n % 8 )) in
        0) Q key meta_l-left ;;
        1) Q key meta_l-right ;;
        2) Q keydown alt key tab sleep 0.6 keyup alt ;;
        3) Q key meta_l-m ;;
        4) Q key meta_l-shift-m ;;
        5) ctl '{"kind":"set-window-layout","mode":"toggle","workspace":0}' ;;
        6) ctl '{"kind":"set-window-layout","mode":"stacking","workspace":0}' ;;
        7) Q key meta_l-down ;;
    esac
    n=$((n + 1)); sleep 6
done
echo "actions: $n"
GLAZE_SHOT_DELAY=1 GLAZE_SHOT_NOGRIM=1 "$S/shot.sh" soak-end M4 | tail -1
pid1=$("$VM" "pgrep -u \$(id -u) -f "^glaze-compositor" | head -1")
echo "compositor pid at end: $pid1 ($( [ "$pid0" = "$pid1" ] && echo same || echo CHANGED))"
"$VM" "grep -cE 'panicked|ERROR' ~/session-tty2.log; grep -E 'frame-time' ~/session-tty2.log | tail -40 | sed 's/.*frames=/frames=/' | cut -c1-150"
