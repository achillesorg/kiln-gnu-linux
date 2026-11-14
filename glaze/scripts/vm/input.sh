#!/usr/bin/env bash
#
# Glaze — drive real pointer/keyboard input in the live tty1 Sway session
# (runs in the VM). Pointer input goes through Sway's seat cursor commands,
# keyboard input through wtype (virtual-keyboard protocol), so the shell sees
# genuine Wayland events.
#
# Usage (VM):  bash scripts/vm/input.sh CMD [ARGS] [CMD [ARGS]] ...
#   move X Y            pointer to output-layout coordinates
#   click X Y           left click at X Y       (rclick / mclick: right / middle)
#   key NAME            one key (wtype -k), e.g. Escape, Return, Down, super
#   type TEXT           type text
#   combo MOD KEY       e.g. combo logo Return   (wtype -M MOD -k KEY -m MOD)
#   sleep SECONDS
# From the host:  ~/fedeos/vm 'bash ~/glaze/scripts/vm/input.sh click 40 780'

set -euo pipefail
. "$(dirname "${BASH_SOURCE[0]}")/swayenv.sh"

press() { # x y button
    # Approach from 1 px away so the client under the pointer always gets
    # an enter/motion event first (a warp onto the same spot sends none).
    swaymsg -q "seat - cursor set $(($1 + 1)) $(($2 + 1))"
    sleep 0.03
    swaymsg -q "seat - cursor set $1 $2"
    sleep 0.05
    swaymsg -q "seat - cursor press $3"
    sleep 0.06
    swaymsg -q "seat - cursor release $3"
}

while [[ $# -gt 0 ]]; do
    cmd="$1"; shift
    case "$cmd" in
        move) swaymsg -q "seat - cursor set $1 $2"; shift 2 ;;
        click) press "$1" "$2" button1; shift 2 ;;
        rclick) press "$1" "$2" button3; shift 2 ;;
        mclick) press "$1" "$2" button2; shift 2 ;;
        key) wtype -k "$1"; shift ;;
        type) wtype "$1"; shift ;;
        combo) wtype -M "$1" -k "$2" -m "$1"; shift 2 ;;
        sleep) sleep "$1"; shift ;;
        *) echo "input.sh: unknown command $cmd" >&2; exit 2 ;;
    esac
done
