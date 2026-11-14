#!/usr/bin/env bash
#
# Screenshot every glaze-settings page in a throwaway headless Sway (INSIDE the
# Kiln VM; never touches the live session). Uses its own XDG_CONFIG_HOME, so
# the user's settings are not read or written.
#
# Usage: apps/glaze-settings/scripts/vm-shots.sh OUTDIR [VARIANT...]
#   VARIANT: light (default) | dark | classic | classic-dark
# Env: BIN (default: the release build), PAGES (space-separated slugs),
#      EXTRA_ARGS (passed to every launch, e.g. "--search clock").
set -uo pipefail
[[ -z "${FS_IN_DBUS:-}" ]] && exec env FS_IN_DBUS=1 dbus-run-session -- "$0" "$@"
OUT="${1:?usage: vm-shots.sh OUTDIR [VARIANT...]}"
shift
VARIANTS=("${@:-light}")
BIN="${BIN:-$HOME/glaze-settings/apps/glaze-settings/target/release/glaze-settings}"
PAGES="${PAGES:-appearance wallpaper animation taskbar start-menu windows displays sound network power default-apps about}"
RT=/tmp/xdg-settings-shots
mkdir -p "$OUT" "$RT" && chmod 700 "$RT"
export XDG_RUNTIME_DIR="$RT"
rm -f "$RT"/sway-ipc.*.sock "$RT"/wayland-*
cat >"$RT/sway.conf" <<'EOF'
output HEADLESS-1 resolution 1280x800 bg #e9e3d6 solid_color
default_border none
for_window [app_id="org.glaze.Settings"] floating enable, resize set 1180 760, move position center
EOF
WLR_RENDERER=pixman WLR_BACKENDS=headless WLR_LIBINPUT_NO_DEVICES=1 sway -c "$RT/sway.conf" >"$RT/sway.log" 2>&1 &
SWAY_PID=$!
for _ in $(seq 50); do
    SOCK=$(ls "$RT"/sway-ipc.*.sock 2>/dev/null | head -1)
    WL=$(ls "$RT" | grep -E '^wayland-[0-9]+$' | head -1)
    [[ -n "$SOCK" && -n "$WL" ]] && break
    sleep 0.1
done
export SWAYSOCK="$SOCK" WAYLAND_DISPLAY="$WL"
trap 'kill $SWAY_PID 2>/dev/null' EXIT

for v in "${VARIANTS[@]}"; do
    CFG="$RT/config-$v"
    rm -rf "$CFG" && mkdir -p "$CFG/glaze"
    case "$v" in
        light) theme=clay scheme=light ;;
        dark) theme=clay scheme=dark ;;
        classic) theme=classic scheme=light ;;
        classic-dark) theme=classic scheme=dark ;;
        *) echo "unknown variant $v" >&2; exit 2 ;;
    esac
    printf '[appearance]\ntheme = "%s"\nscheme = "%s"\n' "$theme" "$scheme" >"$CFG/glaze/config.toml"
    for p in $PAGES; do
        # shellcheck disable=SC2086
        XDG_CONFIG_HOME="$CFG" XDG_STATE_HOME="$RT/state" XDG_CACHE_HOME="$RT/cache" \
            "$BIN" --page "$p" ${EXTRA_ARGS:-} >"$RT/app.log" 2>&1 &
        APP=$!
        for _ in $(seq 80); do
            swaymsg -t get_tree | grep -q '"app_id": "org.glaze.Settings"' && break
            sleep 0.1
        done
        sleep "${SETTLE:-1.5}"
        grim "$OUT/$v-$p.png" && echo "[shots] $OUT/$v-$p.png"
        kill "$APP" 2>/dev/null
        wait "$APP" 2>/dev/null
        for _ in $(seq 30); do
            swaymsg -t get_tree | grep -q '"app_id": "org.glaze.Settings"' || break
            sleep 0.1
        done
    done
done
