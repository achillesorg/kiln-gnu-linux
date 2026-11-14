#!/usr/bin/env bash
#
# glaze-settings smoke test, run INSIDE the Kiln VM (never touches the live
# tty1 session): start a throwaway headless Sway, launch glaze-settings in it,
# wait, assert the process is still alive and that Sway reports a window with
# app_id org.glaze.Settings, optionally take screenshots, then tear down.
#
# Usage (in the VM):
#   apps/glaze-settings/scripts/vm-smoke.sh [BINARY] [-- extra glaze-settings args]
# Env:
#   SHOT=/path/name.png   take a grim screenshot after the window appears
#   KEEP=1                leave the Sway running (prints its env for reuse)
#   WAIT=3                seconds to wait before checking
#
# Exit status: 0 on success, 1 when the app died or presented no window.

set -uo pipefail

BIN="${1:-$HOME/glaze-settings/apps/glaze-settings/target/release/glaze-settings}"
shift || true
[[ "${1:-}" == "--" ]] && shift
ARGS=("$@")
WAIT="${WAIT:-3}"
RT=/tmp/xdg-settings
export XDG_RUNTIME_DIR="$RT"
mkdir -p "$RT" && chmod 700 "$RT"

# Our own config dir so the smoke test never edits the user's settings,
# unless GLAZE_SMOKE_REAL_CONFIG=1.
if [[ "${GLAZE_SMOKE_REAL_CONFIG:-0}" != 1 ]]; then
    export XDG_CONFIG_HOME="${XDG_CONFIG_HOME_SMOKE:-/tmp/glaze-settings-smoke/config}"
    export XDG_STATE_HOME="/tmp/glaze-settings-smoke/state"
    export XDG_CACHE_HOME="/tmp/glaze-settings-smoke/cache"
    mkdir -p "$XDG_CONFIG_HOME" "$XDG_STATE_HOME" "$XDG_CACHE_HOME"
fi

SWAYCONF="$RT/sway-smoke.conf"
cat >"$SWAYCONF" <<'EOF'
output HEADLESS-1 resolution 1280x800 bg #e9e3d6 solid_color
default_border none
for_window [app_id="org.glaze.Settings"] floating enable, resize set 1180 760, move position center
EOF

start_sway() {
    # pixman: the default GLES renderer on llvmpipe cannot be read back by
    # grim on the headless backend ("failed to copy output").
    WLR_RENDERER=pixman WLR_BACKENDS=headless WLR_LIBINPUT_NO_DEVICES=1 sway -c "$SWAYCONF" >"$RT/sway.log" 2>&1 &
    SWAY_PID=$!
    for _ in $(seq 50); do
        SOCK=$(ls "$RT"/sway-ipc.*.sock 2>/dev/null | head -1)
        WL=$(ls "$RT" | grep -E '^wayland-[0-9]+$' | head -1)
        [[ -n "$SOCK" && -n "$WL" ]] && break
        sleep 0.1
    done
    export SWAYSOCK="$SOCK" WAYLAND_DISPLAY="$WL"
}

if [[ -n "${SWAYSOCK:-}" && -S "${SWAYSOCK}" && "${SWAYSOCK}" == "$RT"/* ]]; then
    SWAY_PID=""
else
    rm -f "$RT"/sway-ipc.*.sock "$RT"/wayland-*
    start_sway
fi
echo "[smoke] SWAYSOCK=$SWAYSOCK WAYLAND_DISPLAY=$WAYLAND_DISPLAY"

dbus-run-session -- "$BIN" "${ARGS[@]}" >"$RT/settings.log" 2>&1 &
APP_PID=$!
# Wait (up to TIMEOUT s) for the window to be mapped, then require the app to
# stay alive WAIT more seconds.
TIMEOUT="${TIMEOUT:-20}"
for _ in $(seq $((TIMEOUT * 4))); do
    kill -0 "$APP_PID" 2>/dev/null || break
    swaymsg -t get_tree 2>/dev/null | grep -q '"app_id": "org.glaze.Settings"' && break
    sleep 0.25
done
sleep "$WAIT"

status=0
if ! kill -0 "$APP_PID" 2>/dev/null; then
    echo "[smoke] FAIL: glaze-settings exited early; log:" >&2
    cat "$RT/settings.log" >&2
    status=1
elif ! swaymsg -t get_tree | grep -q '"app_id": "org.glaze.Settings"'; then
    echo "[smoke] FAIL: no org.glaze.Settings window in the Sway tree" >&2
    cat "$RT/settings.log" >&2
    status=1
else
    echo "[smoke] OK: glaze-settings is running and presented its window"
    if [[ -n "${SHOT:-}" ]]; then
        grim "$SHOT" && echo "[smoke] screenshot $SHOT"
    fi
fi

if [[ "${KEEP:-0}" == 1 ]]; then
    echo "export XDG_RUNTIME_DIR=$RT SWAYSOCK=$SWAYSOCK WAYLAND_DISPLAY=$WAYLAND_DISPLAY APP_PID=$APP_PID"
else
    kill "$APP_PID" 2>/dev/null
    [[ -n "${SWAY_PID:-}" ]] && kill "$SWAY_PID" 2>/dev/null
fi
exit $status
