#!/usr/bin/env bash
# Glaze M4: animation frame-time bench on the tty2 compositor that needs no
# QMP input (control socket only). Restarts the tty2 session (optionally
# with another compositor binary and/or env), opens two foot windows, idles
# 10 s, then runs CYCLES rounds of maximise toggle, minimise + restore and
# tiling toggle (one action every 1.2 s) and prints a summary of the frames
# rendered while animating (from the compositor's frame-time lines) and the
# idle cost. Always ends on tty1 with the normal binary back in place.
#
# Usage: m4-animbench.sh LABEL [CYCLES] [BINARY_IN_VM] [ENV=VAL ...]
#   BINARY_IN_VM: e.g. ~/glaze-compositor-head ("-" = the deployed one)
set -uo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"; S="$ROOT/scripts/vm"; VM="${GLAZE_VM:-$ROOT/../vm}"
R=/run/user/1000/glaze-tty2
LABEL=$1; CYCLES=${2:-6}; BIN=${3:--}; shift; shift || true; shift || true
BIN_DIR='~/.local/glaze-m4/bin'
ctl() { "$VM" "python3 -c 'import socket,sys; s=socket.socket(socket.AF_UNIX); s.settimeout(15); s.connect(\"$R/glaze/compositor.sock\"); s.sendall(sys.argv[1].encode()+b\"\\n\"); print(s.makefile().readline().strip())' '$1'"; }
cleanup() {
    "$VM" "rm -f ~/.config/glaze/tty2-cmd; [ -f $BIN_DIR/glaze-compositor.bench-keep ] && mv -f $BIN_DIR/glaze-compositor.bench-keep $BIN_DIR/glaze-compositor; true"
    "$S/chvt.sh" 1 >/dev/null
}
trap cleanup EXIT
if [ "$BIN" != "-" ]; then
    "$VM" "cp -f $BIN_DIR/glaze-compositor $BIN_DIR/glaze-compositor.bench-keep && install -m 755 $BIN $BIN_DIR/glaze-compositor"
fi
if [ $# -gt 0 ]; then "$VM" "echo '$* glaze-session run --backend glaze' > ~/.config/glaze/tty2-cmd"; else "$VM" "rm -f ~/.config/glaze/tty2-cmd"; fi
"$S/m4-check.sh" start >/dev/null 2>&1
"$S/chvt.sh" 2 >/dev/null
"$S/m4-run.sh" 'foot --title bench-a' >/dev/null
"$S/m4-run.sh" 'foot --title bench-b' >/dev/null
sleep 8
ids=$(ctl '{"kind":"list-windows"}' | python3 -c 'import json,sys; print(" ".join(w["id"] for w in json.load(sys.stdin).get("windows", []) if w["title"].startswith("bench")))')
[ -n "$ids" ] || { echo "no bench windows"; exit 1; }
# Marks are wall-clock stamps (the session log is not opened for append).
t_idle=$("$VM" "date -u +%H:%M:%S"); sleep 11
t_anim=$("$VM" "date -u +%H:%M:%S")
for _ in $(seq "$CYCLES"); do
    for id in $ids; do
        ctl "{\"kind\":\"set-maximized\",\"id\":\"$id\"}" >/dev/null; sleep 1.2
        ctl "{\"kind\":\"set-maximized\",\"id\":\"$id\"}" >/dev/null; sleep 1.2
        ctl "{\"kind\":\"minimize-window\",\"id\":\"$id\"}" >/dev/null; sleep 1.2
        ctl "{\"kind\":\"restore-window\",\"id\":\"$id\"}" >/dev/null; sleep 1.2
    done
    ctl '{"kind":"set-window-layout","mode":"toggle","workspace":0}' >/dev/null; sleep 1.2
done
sleep 6
t_end=$("$VM" "date -u +%H:%M:%S")
echo "== $LABEL ($("$VM" "grep -o 'low_power=[a-z]*' ~/session-tty2.log | tail -1"))"
"$VM" "awk -v ta=$t_idle -v tb=$t_anim -v tc=$t_end '/frame-time/ { t=substr(\$1, 12, 8); s=(t>ta && t<=tb) ? 1 : (t>tb && t<=tc) ? 2 : 0 } s && /frame-time/ {
    n=\$0; sub(/.*frames=/, \"\", n); n+=0; a=\$0; sub(/.*avg_ms=\"/, \"\", a); a+=0; p=\$0; sub(/.*p95_ms=\"/, \"\", p); p+=0
    m=\$0; sub(/.*max_ms=\"/, \"\", m); m+=0; v=\$0; sub(/.*vblank_avg_ms=\"/, \"\", v); v+=0
    if (s==1) { inf+=n; ims+=n*a } else { f+=n; ms+=n*a; if (p>pm) pm=p; if (m>mx) mx=m; if (v>0) { vb+=v; vn++ } }
  } END {
    printf \"idle: %d frames, %.1f ms CPU render in ~10 s\n\", inf, ims;
    if (f) printf \"anim: %d frames, avg %.1f ms, worst 5 s p95 %.1f ms, max %.1f ms, flip-to-flip avg %.1f ms\n\", f, ms/f, pm, mx, (vn?vb/vn:0)
  }' ~/session-tty2.log"
