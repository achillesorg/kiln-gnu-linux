#!/usr/bin/env bash
#
# Glaze — idle cost of the running shell and service (in the VM): CPU over
# a window (from /proc utime+stime), RSS, threads, wake-ups (voluntary
# context switches) and the shell's start-up time from its log.
#
# Usage (VM): bash scripts/vm/perf-shell.sh [SECONDS] [shell-name] [service-name]

set -uo pipefail
SECS="${1:-30}"
SHELL_BIN="${2:-glaze-shell}"
SVC_BIN="${3:-glaze-service}"
HZ=$(getconf CLK_TCK)
declare -A T0 W0
for n in "$SHELL_BIN" "$SVC_BIN"; do
    p=$(pgrep -x "$n" | head -n1) || continue
    T0[$n]=$(awk '{print $14+$15}' "/proc/$p/stat")
    W0[$n]=$(awk '/^voluntary_ctxt_switches/{print $2}' "/proc/$p/status")
done
sleep "$SECS"
for n in "$SHELL_BIN" "$SVC_BIN"; do
    p=$(pgrep -x "$n" | head -n1) || { echo "$n: not running"; continue; }
    t1=$(awk '{print $14+$15}' "/proc/$p/stat")
    w1=$(awk '/^voluntary_ctxt_switches/{print $2}' "/proc/$p/status")
    rss=$(awk '/^VmRSS/{print $2}' "/proc/$p/status")
    thr=$(awk '/^Threads/{print $2}' "/proc/$p/status")
    cpu=$(echo "scale=2; 100 * (${t1} - ${T0[$n]}) / ($HZ * $SECS)" | bc)
    wak=$(echo "scale=1; (${w1} - ${W0[$n]}) / $SECS" | bc)
    echo "$n (pid $p): idle CPU ${cpu}% of one core over ${SECS}s, ${wak} wake-ups/s, RSS $((rss / 1024)) MiB, $thr threads"
done
log="$HOME/.cache/glaze/live-shell.log"
[ -f "$log" ] && grep -E "started in|presented" "$log" | tail -n 2
