#!/usr/bin/env bash
#
# Glaze M4: scripted checks of the Glaze compositor session on tty2 (real DRM
# display, real QEMU input through QMP), screenshots into docs/screens/M4/.
# ALWAYS switches back to tty1 at the end (trap), even on failure.
#
# Prerequisites: scripts/vm/install-tty2.sh, scripts/vm/m4-deploy.sh.
#
# Usage: scripts/vm/m4-check.sh [STEP...]
#   start      (re)start the tty2 session (kills only the tty2 session's processes)
#   status     control-socket window list, layout, log tail
#   shot NAME  screenshot
#   ctl JSON   one control-socket request
#   ptr ARGS   scripts/vm/qmp_pointer.py ARGS (quote as one argument)
#   wait S     sleep S seconds
#   stay       do not switch back to tty1 at the end
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
VM="${GLAZE_VM:-$ROOT/../vm}"
R=/run/user/1000/glaze-tty2
STAY=0
back() { [[ "$STAY" == 1 ]] || "$ROOT/scripts/vm/chvt.sh" 1 >/dev/null; }
trap back EXIT

ctl() {
    "$VM" "python3 -c 'import socket,sys; s=socket.socket(socket.AF_UNIX); s.settimeout(15); s.connect(\"$R/glaze/compositor.sock\"); s.sendall(sys.argv[1].encode()+b\"\\n\"); print(s.makefile().readline().strip())' '$1'"
}

"$ROOT/scripts/vm/chvt.sh" 2 >/dev/null
while [[ $# -gt 0 ]]; do
    case "$1" in
        start)
            # Only the tty2 session: processes whose XDG_RUNTIME_DIR is ours.
            # glaze-session first (SIGTERM = clean stop, no crash marker).
            "$VM" "pkill -u \$(id -u) -f '^glaze-session run --backend glaze' || true; sleep 1
                   for p in \$(pgrep -u \$(id -u)); do grep -qz '^XDG_RUNTIME_DIR=$R\$' /proc/\$p/environ 2>/dev/null && echo \$p; done | xargs -r kill 2>/dev/null
                   rm -f ~/.local/state/glaze/compositor-failed $R/wayland-* $R/glaze/compositor.sock; true"
            sleep 8
            "$VM" "grep -E 'glaze-session|FALLING|ready|ERROR|panicked' ~/session-tty2.log | tail -8" || true
            ;;
        status)
            ctl '{"kind":"list-windows"}' | python3 -c 'import json,sys; d=json.load(sys.stdin); [print(" ", w["id"], repr(w["title"]), "ws", w["workspace"], "focused" if w["focused"] else "", "MIN" if w["minimized"] else "", "float" if w["floating"] else "tiled") for w in d.get("windows", [])]'
            ctl '{"kind":"get-layout"}'
            "$VM" "grep -vE 'control request' ~/session-tty2.log | tail -4 | cut -c1-220"
            ;;
        shot) GLAZE_SHOT_DELAY=1 "$ROOT/scripts/vm/shot.sh" "$2" M4; shift ;;
        ctl) ctl "$2"; shift ;;
        ptr) # shellcheck disable=SC2086
             python3 "$ROOT/scripts/vm/qmp_pointer.py" $2; shift ;;
        wait) sleep "$2"; shift ;;
        stay) STAY=1 ;;
        *) echo "unknown step $1" >&2; exit 2 ;;
    esac
    shift
done
