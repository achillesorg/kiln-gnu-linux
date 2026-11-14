#!/usr/bin/env bash
# Drag a window title in circles for ~N seconds with the QMP tablet (perf load).
# Usage: m4-drag.sh X Y [SECONDS] [WxH]
set -uo pipefail
S="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
X=$1; Y=$2; T=${3:-6}; SIZE=${4:-1280x800}
python3 - "$S" "$X" "$Y" "$T" "$SIZE" <<'PY'
import math, sys, time
sys.path.insert(0, sys.argv[1])
import qmp_pointer as q
x, y, t = float(sys.argv[2]), float(sys.argv[3]), float(sys.argv[4])
w, h = (int(v) for v in sys.argv[5].split("x"))
c = q.Qmp(q.SOCK)
def move(px, py):
    c.cmd("input-send-event", {"events": [
        {"type": "abs", "data": {"axis": "x", "value": int(px * 32767 / (w - 1))}},
        {"type": "abs", "data": {"axis": "y", "value": int(py * 32767 / (h - 1))}}]})
def btn(d):
    c.cmd("input-send-event", {"events": [{"type": "btn", "data": {"down": d, "button": "left"}}]})
move(x, y); time.sleep(0.2); btn(True); time.sleep(0.1)
t0 = time.time()
while time.time() - t0 < t:
    a = (time.time() - t0) * 2.5
    move(x + 120 * math.sin(a), y + 60 + 60 * math.sin(2 * a)); time.sleep(0.016)
move(x, y); time.sleep(0.1); btn(False)
PY
