#!/usr/bin/env bash
#
# Glaze — drag every edge and corner of one floating window with the real VM
# pointer (QMP USB tablet, scripts/vm/qmp_pointer.py) and print the geometry
# before/after each drag. Host side. Usage: scripts/vm/test-resize.sh CON_ID
# Assumes a 1280x800 output at scale 1.

set -uo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
ID="${1:?con id}"
VM="$ROOT/../vm"
geo() {
    "$VM" ". ~/glaze/scripts/vm/swayenv.sh; swaymsg -t get_tree" | python3 -c "
import json,sys
def w(n):
  if n.get('id')==$ID:
    r=n['rect']; d=n['deco_rect']['height']; print(r['x'],r['y']-d,r['width'],r['height']+d)
  for c in n.get('nodes',[])+n.get('floating_nodes',[]): w(c)
w(json.load(sys.stdin))"
}
"$VM" ". ~/glaze/scripts/vm/swayenv.sh; swaymsg -q '[con_id=$ID] focus'"
for name in left right bottom top bottomright bottomleft topleft topright; do
    read -r X Y W H <<<"$(geo)"
    case $name in
        left) a=($((X + 2)) $((Y + H / 2)) $((X - 38)) $((Y + H / 2))) ;;
        right) a=($((X + W - 3)) $((Y + H / 2)) $((X + W + 37)) $((Y + H / 2))) ;;
        bottom) a=($((X + W / 2)) $((Y + H - 3)) $((X + W / 2)) $((Y + H + 27))) ;;
        top) a=($((X + W / 2)) $((Y)) $((X + W / 2)) $((Y - 30))) ;;
        bottomright) a=($((X + W - 3)) $((Y + H - 3)) $((X + W + 27)) $((Y + H + 27))) ;;
        bottomleft) a=($((X + 2)) $((Y + H - 3)) $((X - 28)) $((Y + H + 27))) ;;
        topleft) a=($((X + 2)) $((Y)) $((X - 28)) $((Y - 30))) ;;
        topright) a=($((X + W - 3)) $((Y)) $((X + W + 27)) $((Y - 30))) ;;
    esac
    python3 "$ROOT/scripts/vm/qmp_pointer.py" drag "${a[@]}"
    sleep 0.5
    read -r X2 Y2 W2 H2 <<<"$(geo)"
    echo "$name: ($X,$Y ${W}x$H) -> ($X2,$Y2 ${W2}x$H2)"
done
