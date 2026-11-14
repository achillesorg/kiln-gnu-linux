#!/usr/bin/env bash
#
# Glaze — install developer symlinks (optional).
#
# Symlinks the freshly built Glaze binaries into a user bin directory and
# installs `.desktop` entries pointing at them, so the shell can be launched
# like a normal app. Everything lives under your home directory; nothing needs
# root and no session/login-manager configuration is touched.
#
# Idempotent: existing symlinks are replaced; unrelated files are left alone.
#
# Usage:
#   scripts/install-dev.sh [--release] [--bindir DIR] [--uninstall] [--help]

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
PROFILE="debug"
BINDIR="${HOME}/.local/bin"
APPDIR="${XDG_DATA_HOME:-$HOME/.local/share}/applications"
UNINSTALL=0

usage() {
    cat <<'EOF'
install-dev.sh — symlink built Glaze binaries for local development

USAGE:
    scripts/install-dev.sh [OPTIONS]

OPTIONS:
    --release       Link the release build instead of the debug build.
    --bindir DIR    Target bin directory (default: ~/.local/bin).
    --uninstall     Remove the symlinks and desktop entries this script made.
    -h, --help      Show this help.

NOTES:
    * No root required; nothing outside your home directory is modified.
    * Build first: scripts/build.sh [--release]
EOF
}

log()  { printf '\033[1;34m[glaze]\033[0m %s\n' "$*"; }
warn() { printf '\033[1;33m[glaze]\033[0m %s\n' "$*" >&2; }
die()  { printf '\033[1;31m[glaze]\033[0m %s\n' "$*" >&2; exit 1; }

while [[ $# -gt 0 ]]; do
    case "$1" in
        --release) PROFILE="release" ;;
        --bindir) shift || true; BINDIR="${1:-}"; [[ -n "$BINDIR" ]] || die "--bindir needs a value" ;;
        --uninstall) UNINSTALL=1 ;;
        -h|--help) usage; exit 0 ;;
        *) die "unknown argument: $1 (try --help)" ;;
    esac
    shift
done

BINARIES=(glaze-shell glaze-settings glaze-session glaze-service glaze-compositor glaze)

if [[ "$UNINSTALL" -eq 1 ]]; then
    log "removing developer symlinks from $BINDIR"
    for b in "${BINARIES[@]}"; do
        if [[ -L "$BINDIR/$b" ]]; then rm -f "$BINDIR/$b"; log "  removed $BINDIR/$b"; fi
    done
    if [[ -f "$APPDIR/glaze-shell-dev.desktop" ]]; then
        rm -f "$APPDIR/glaze-shell-dev.desktop"
        log "  removed $APPDIR/glaze-shell-dev.desktop"
    fi
    exit 0
fi

mkdir -p "$BINDIR" "$APPDIR"

linked=0
for b in "${BINARIES[@]}"; do
    src="$ROOT/target/$PROFILE/$b"
    if [[ -x "$src" ]]; then
        ln -sfn "$src" "$BINDIR/$b"
        log "linked $BINDIR/$b -> $src"
        linked=$((linked + 1))
    fi
done

if [[ "$linked" -eq 0 ]]; then
    die "no binaries found under $ROOT/target/$PROFILE. Run scripts/build.sh first."
fi

if [[ -x "$BINDIR/glaze-shell" ]]; then
    cat >"$APPDIR/glaze-shell-dev.desktop" <<EOF
[Desktop Entry]
Type=Application
Name=Glaze Shell (dev)
Comment=Development launcher for the nested Glaze shell
Exec=$BINDIR/glaze-shell
Terminal=false
Categories=System;
EOF
    log "wrote $APPDIR/glaze-shell-dev.desktop"
fi

case ":$PATH:" in
    *":$BINDIR:"*) ;;
    *) warn "$BINDIR is not on PATH; add it to use the linked binaries." ;;
esac

log "done. Undo with: scripts/install-dev.sh --uninstall"
