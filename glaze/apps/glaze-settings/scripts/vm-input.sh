#!/usr/bin/env bash
#
# Drive real pointer/keyboard input into glaze-settings running in the
# throwaway headless Sway started by `KEEP=1 vm-smoke.sh` (INSIDE the VM; never
# the live session). Same verbs as scripts/vm/input.sh:
#
#   vm-input.sh click X Y | rclick X Y | move X Y | key NAME | type TEXT |
#               combo MOD KEY | sleep S | shot FILE | cfg
#
# `cfg` prints the smoke config.toml and settings.toml.
set -euo pipefail
RT=/tmp/xdg-settings
export XDG_RUNTIME_DIR="$RT"
SWAYSOCK=$(ls "$RT"/sway-ipc.*.sock 2>/dev/null | head -1)
WAYLAND_DISPLAY=$(ls "$RT" | grep -E '^wayland-[0-9]+$' | head -1)
export SWAYSOCK WAYLAND_DISPLAY
[[ -S "$SWAYSOCK" ]] || { echo "vm-input: no headless sway in $RT (run KEEP=1 vm-smoke.sh)" >&2; exit 1; }
CFG="${XDG_CONFIG_HOME_SMOKE:-/tmp/glaze-settings-smoke/config}"

# A throwaway headless Sway has no input devices, so `seat - cursor` clicks
# never reach clients. Pointer input goes through tools/vptr (a
# wlr-virtual-pointer daemon fed through a FIFO); keys through wtype.
VPTR="$(dirname "$0")/../tools/vptr/target/release/glaze-vptr"
FIFO="$RT/vptr.in"
ensure_vptr() {
    # Restart the helper when it is gone or belongs to an earlier Sway.
    if [[ ! -p "$FIFO" ]] || ! pgrep -f "glaze-vptr 1280 800" >/dev/null \
        || [[ "$(cat "$RT/vptr.sway" 2>/dev/null)" != "$SWAYSOCK" ]]; then
        [[ -x "$VPTR" ]] || { echo "vm-input: build tools/vptr first (cargo build --release)" >&2; exit 1; }
        pkill -f "glaze-vptr 1280 800" 2>/dev/null || true
        echo "$SWAYSOCK" >"$RT/vptr.sway"
        rm -f "$FIFO"; mkfifo "$FIFO"
        (setsid sleep infinity >"$FIFO" </dev/null 2>/dev/null &)
        (setsid "$VPTR" 1280 800 <"$FIFO" >"$RT/vptr.log" 2>&1 &) </dev/null
        sleep 0.5
    fi
}
ptr() { ensure_vptr; echo "$*" >"$FIFO"; sleep 0.25; }

while [[ $# -gt 0 ]]; do
    cmd="$1"; shift
    case "$cmd" in
        move) ptr move "$1" "$2"; shift 2 ;;
        click) ptr click "$1" "$2"; shift 2 ;;
        dclick) ptr dclick "$1" "$2"; shift 2 ;;
        rclick) ptr rclick "$1" "$2"; shift 2 ;;
        scroll) ptr scroll "$1"; shift ;;
        key) wtype -s 700 -k "$1"; shift ;;
        type) wtype -s 700 "$1"; shift ;;
        combo) wtype -s 700 -M "$1" -k "$2" -m "$1"; shift 2 ;;
        sleep) sleep "$1"; shift ;;
        shot) grim "$1"; echo "[input] shot $1"; shift ;;
        cfg)
            echo "--- $CFG/glaze/config.toml"; cat "$CFG/glaze/config.toml" 2>/dev/null || echo "(none)"
            echo "--- $CFG/glaze/settings.toml"; cat "$CFG/glaze/settings.toml" 2>/dev/null || echo "(none)" ;;
        *) echo "vm-input: unknown command $cmd" >&2; exit 2 ;;
    esac
done
