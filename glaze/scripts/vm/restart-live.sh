#!/usr/bin/env bash
#
# Glaze — restart glaze-service and glaze-shell INSIDE the running tty1 session.
# Runs in the VM (redeploy.sh / sync-build.sh --install call it over ssh).
#
# The session env is copied from the running glaze-shell (or, if none, rebuilt
# from the live sway process: XDG_RUNTIME_DIR, WAYLAND_DISPLAY, SWAYSOCK,
# XDG_CURRENT_DESKTOP=Glaze). glaze-session does not respawn these two
# components and keeps running while sway lives, so killing them is safe.
# Logs of the relaunched processes: ~/.cache/glaze/live-{service,shell}.log
#
# Usage (in the VM): bash scripts/vm/restart-live.sh [--only-shell]

set -uo pipefail

ONLY_SHELL=0
[[ "${1:-}" == "--only-shell" ]] && ONLY_SHELL=1

uid=$(id -u)
BIN="${GLAZE_BIN:-$HOME/.local/bin}"
LOGDIR="$HOME/.cache/glaze"
mkdir -p "$LOGDIR"

sway_pid=$(pgrep -u "$uid" -x sway | head -n1 || true)
if [[ -z "$sway_pid" ]]; then
    echo "[live] sway is NOT running in the tty1 session: nothing restarted."
    echo "[live] (log in on tty1 or reboot the VM to start the Glaze session)"
    exit 3
fi

old_shell=$(pgrep -u "$uid" -x glaze-shell | tr '\n' ' ' || true)
old_service=$(pgrep -u "$uid" -x glaze-service | tr '\n' ' ' || true)

# Session environment.
declare -a ENVV=()
src_pid=$(pgrep -u "$uid" -x glaze-shell | head -n1 || true)
if [[ -n "$src_pid" && -r /proc/$src_pid/environ ]]; then
    while IFS= read -r kv; do
        case "$kv" in
            WAYLAND_DISPLAY=*|SWAYSOCK=*|XDG_RUNTIME_DIR=*|XDG_CURRENT_DESKTOP=*|XDG_SESSION_TYPE=*|\
            XDG_SESSION_DESKTOP=*|DBUS_SESSION_BUS_ADDRESS=*|XDG_DATA_DIRS=*|LANG=*|PATH=*|HOME=*|USER=*)
                ENVV+=("$kv") ;;
        esac
    done < <(tr '\0' '\n' </proc/$src_pid/environ)
    echo "[live] session env copied from glaze-shell pid $src_pid"
else
    rt="/run/user/$uid"
    wl=$(cd "$rt" && ls wayland-[0-9]* 2>/dev/null | grep -v '\.lock$' | head -n1)
    sock=$(ls "$rt"/sway-ipc."$uid"."$sway_pid".sock 2>/dev/null | head -n1)
    ENVV=("XDG_RUNTIME_DIR=$rt" "WAYLAND_DISPLAY=$wl" "SWAYSOCK=$sock"
          "XDG_CURRENT_DESKTOP=Glaze" "XDG_SESSION_TYPE=wayland" "XDG_SESSION_DESKTOP=glaze"
          "DBUS_SESSION_BUS_ADDRESS=unix:path=$rt/bus" "HOME=$HOME" "USER=$USER"
          "PATH=$BIN:/usr/local/bin:/usr/bin:/bin")
    echo "[live] no glaze-shell running; env rebuilt from sway pid $sway_pid ($wl, $sock)"
fi

# Renderer for the GTK shell (llvmpipe): GLAZE_GSK_RENDERER=cairo|gl|ngl|vulkan.
EXTRA=()
[[ -n "${GLAZE_GSK_RENDERER:-}" ]] && EXTRA+=("GSK_RENDERER=$GLAZE_GSK_RENDERER")
launch() {
    local name="$1"; shift
    env -i "${ENVV[@]}" "${EXTRA[@]}" GLAZE_LIVE_RESTART=1 setsid "$BIN/$name" "$@" \
        >>"$LOGDIR/live-${name#glaze-}.log" 2>&1 </dev/null &
    disown || true
}

if [[ "$ONLY_SHELL" -eq 0 && -n "$old_service" ]]; then
    pkill -u "$uid" -x glaze-service || true
fi
[[ -n "$old_shell" ]] && pkill -u "$uid" -x glaze-shell || true
for _ in $(seq 1 30); do
    pgrep -u "$uid" -x glaze-shell >/dev/null || break
    sleep 0.1
done

if [[ "$ONLY_SHELL" -eq 0 ]]; then
    launch glaze-service --serve
    sleep 0.5
fi
launch glaze-shell
sleep 1.5

new_shell=$(pgrep -u "$uid" -x glaze-shell | tr '\n' ' ' || true)
new_service=$(pgrep -u "$uid" -x glaze-service | tr '\n' ' ' || true)
echo "[live] glaze-service: old pid(s) [${old_service% }] -> new [${new_service% }]"
echo "[live] glaze-shell:   old pid(s) [${old_shell% }] -> new [${new_shell% }]"
if [[ -z "$new_shell" ]]; then
    echo "[live] ERROR: glaze-shell did not stay up; last log lines:"
    tail -n 20 "$LOGDIR/live-shell.log"
    exit 1
fi
