#!/usr/bin/env bash
# Glaze M4: idle cost of the tty2 compositor. Makes tty2 active, waits, then
# samples the compositor's main-thread context switches (event-loop
# wake-ups) and CPU ticks (all threads, incl. llvmpipe) over N seconds
# (default 20) with no input. Always ends on tty1.  Usage: m4-idle.sh [SECS]
set -uo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"; S="$ROOT/scripts/vm"; VM="${GLAZE_VM:-$ROOT/../vm}"
N="${1:-20}"
trap '"$S/chvt.sh" 1 >/dev/null' EXIT
"$S/chvt.sh" 2 >/dev/null; sleep 5
"$VM" "P=\$(pgrep -f '^glaze-compositor' | head -1); [ -n \"\$P\" ] || { echo 'no compositor'; exit 1; }
  cs() { grep -E '^(non)?voluntary_ctxt_switches' /proc/\$P/task/\$P/status | awk '{s+=\$2} END {print s}'; }
  ticks() { awk '{print \$14+\$15}' /proc/\$P/stat; }
  a=\$(cs); t0=\$(ticks); sleep $N; b=\$(cs); t1=\$(ticks)
  echo \"pid=\$P idle ${N}s: wake-ups/s=\$(( (b-a)/$N )) cpu=\$(( (t1-t0)*100/$N/\$(getconf CLK_TCK) ))% of one core\""
