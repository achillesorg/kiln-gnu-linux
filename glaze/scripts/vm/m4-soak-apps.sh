#!/usr/bin/env bash
# Glaze M4: restart the tty2 session, open five apps (Thunar, 2x foot,
# Settings, Firefox), then run m4-soak.sh for MIN minutes at 1280x800 and
# print the frame-time lines and slow-frame count of the soak period.
# Usage: m4-soak-apps.sh [MIN] [LABEL]
set -uo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"; S="$ROOT/scripts/vm"; VM="${GLAZE_VM:-$ROOT/../vm}"
MIN="${1:-3}"; LABEL="${2:-soak}"; OUT="${TMPDIR:-/tmp}/glaze-soak-$LABEL.out"
"$S/m4-check.sh" stay start >/dev/null 2>&1
sleep 12
for a in thunar "foot --title soak1" "foot --title soak2" glaze-settings firefox-esr; do
    "$S/m4-run.sh" "$a" >/dev/null 2>&1; sleep 4
done
sleep 20
t0=$("$VM" 'date -u +%H:%M:%S')
"$S/m4-soak.sh" "$MIN" 1280x800 > "$OUT" 2>&1
echo "== $LABEL (from $t0; full output $OUT)"
grep -E "actions|pid at" "$OUT"
"$VM" "uptime; grep -E 'frame-time' ~/session-tty2.log | sed 's/.*T\([0-9:]*\)\..*frames=/\1 frames=/' | awk '\$1 >= \"$t0\"' | cut -c1-200
  echo -n 'slow frames (>50 ms): '; grep 'slow frame' ~/session-tty2.log | sed 's/.*T\([0-9:]*\)\..*slow/\1 slow/' | awk '\$1 >= \"$t0\"' | wc -l"
