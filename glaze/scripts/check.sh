#!/usr/bin/env bash
#
# Glaze — environment / readiness report.
#
# Prints, without changing anything:
#   * OS, architecture, Rust toolchain
#   * Wayland/X11 display availability and session type
#   * detected compositor backend (Sway / Hyprland / wlroots hints)
#   * whether GTK4 and gtk4-layer-shell development files are visible
#   * how many real `.desktop` entries the registry would see
#
# Exits non-zero only under --strict (when a required runtime piece is missing).
#
# Usage:
#   scripts/check.sh [--strict] [--help]

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
STRICT=0

usage() {
    cat <<'EOF'
check.sh — report whether this host is ready to build and run Glaze

USAGE:
    scripts/check.sh [--strict]

OPTIONS:
    --strict   Exit non-zero if a runtime prerequisite (Wayland display,
               gtk4-layer-shell) is missing.
    -h, --help Show this help.
EOF
}

ok()   { printf '  [ ok ] %s\n' "$*"; }
missing() { printf '  [ -- ] %s\n' "$*"; }
line() { printf '\n\033[1;34m%s\033[0m\n' "$*"; }

while [[ $# -gt 0 ]]; do
    case "$1" in
        --strict) STRICT=1 ;;
        -h|--help) usage; exit 0 ;;
        *) printf 'unknown argument: %s (try --help)\n' "$1" >&2; exit 2 ;;
    esac
    shift
done

PROBLEMS=0

line "Host"
printf '  os:   %s\n' "$(uname -s)"
printf '  arch: %s\n' "$(uname -m)"
if [[ -r /etc/os-release ]]; then
    # shellcheck disable=SC1091
    . /etc/os-release
    printf '  distro: %s\n' "${PRETTY_NAME:-unknown}"
fi

line "Rust toolchain"
if command -v rustc >/dev/null 2>&1; then
    ok "rustc $(rustc --version | awk '{print $2}')"
else
    missing "rustc not found (scripts/bootstrap-fedora.sh)"
    PROBLEMS=$((PROBLEMS + 1))
fi
if command -v cargo >/dev/null 2>&1; then
    ok "cargo $(cargo --version | awk '{print $2}')"
else
    missing "cargo not found (scripts/bootstrap-fedora.sh)"
    PROBLEMS=$((PROBLEMS + 1))
fi

line "Display / session"
if [[ -n "${WAYLAND_DISPLAY:-}" ]]; then
    ok "WAYLAND_DISPLAY=$WAYLAND_DISPLAY"
else
    missing "WAYLAND_DISPLAY not set (glaze-shell needs a Wayland session)"
    PROBLEMS=$((PROBLEMS + 1))
fi
[[ -n "${DISPLAY:-}" ]] && ok "DISPLAY=$DISPLAY (X11/XWayland present)" || true
printf '  XDG_SESSION_TYPE=%s\n' "${XDG_SESSION_TYPE:-<unset>}"
printf '  XDG_CURRENT_DESKTOP=%s\n' "${XDG_CURRENT_DESKTOP:-<unset>}"
printf '  XDG_RUNTIME_DIR=%s\n' "${XDG_RUNTIME_DIR:-<unset>}"

line "Compositor backend"
if [[ -n "${SWAYSOCK:-}" ]] && command -v swaymsg >/dev/null 2>&1; then
    ver="$(swaymsg -t get_version 2>/dev/null | tr -d '\n' | head -c 120 || true)"
    ok "Sway detected (SWAYSOCK set): ${ver:-version unknown}"
elif [[ -n "${HYPRLAND_INSTANCE_SIGNATURE:-}" ]]; then
    ok "Hyprland detected (HYPRLAND_INSTANCE_SIGNATURE set)"
elif [[ -n "${WAYLAND_DISPLAY:-}" ]]; then
    missing "no known wlroots IPC (Sway/Hyprland) — taskbar state bridge unavailable"
else
    missing "no compositor session detected"
fi
if command -v sway >/dev/null 2>&1; then
    ok "sway is installed (usable for scripts/run-nested.sh)"
else
    missing "sway not installed (needed for nested developer mode)"
fi

line "GTK4 / layer-shell (needed by apps/)"
if command -v pkg-config >/dev/null 2>&1; then
    if pkg-config --exists gtk4 2>/dev/null; then
        ok "gtk4 $(pkg-config --modversion gtk4)"
    else
        missing "gtk4 development files not found (dnf install gtk4-devel)"
        PROBLEMS=$((PROBLEMS + 1))
    fi
    if pkg-config --exists gtk4-layer-shell-0 2>/dev/null; then
        ok "gtk4-layer-shell $(pkg-config --modversion gtk4-layer-shell-0)"
    elif pkg-config --exists gtk4-layer-shell 2>/dev/null; then
        ok "gtk4-layer-shell $(pkg-config --modversion gtk4-layer-shell)"
    else
        missing "gtk4-layer-shell development files not found (dnf install gtk4-layer-shell-devel)"
        PROBLEMS=$((PROBLEMS + 1))
    fi
else
    missing "pkg-config not found; cannot probe GTK4/layer-shell"
    PROBLEMS=$((PROBLEMS + 1))
fi

line "Application registry"
count=0
IFS=':' read -r -a dirs <<<"${XDG_DATA_DIRS:-/usr/local/share:/usr/share}"
dirs=("${XDG_DATA_HOME:-$HOME/.local/share}" "${dirs[@]}")
for d in "${dirs[@]}"; do
    apps="$d/applications"
    if [[ -d "$apps" ]]; then
        n="$(find "$apps" -maxdepth 3 -name '*.desktop' 2>/dev/null | wc -l | tr -d ' ')"
        printf '  %-40s %s entry file(s)\n' "$apps" "$n"
        count=$((count + n))
    fi
done
if [[ "$count" -gt 0 ]]; then
    ok "found $count .desktop file(s) across XDG data dirs"
else
    missing "no .desktop entries found — the launcher would be empty"
fi

line "Workspace"
if [[ -f "$ROOT/Cargo.toml" ]]; then
    ok "workspace manifest: $ROOT/Cargo.toml"
else
    missing "no Cargo.toml at $ROOT"
fi

echo
if [[ "$PROBLEMS" -eq 0 ]]; then
    printf '\033[1;32m[glaze]\033[0m ready: host satisfies every runtime prerequisite.\n'
    exit 0
fi

printf '\033[1;33m[glaze]\033[0m %d prerequisite(s) missing (see [ -- ] lines).\n' "$PROBLEMS" >&2
if [[ "$STRICT" -eq 1 ]]; then
    exit 1
fi
exit 0
