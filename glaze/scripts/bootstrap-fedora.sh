#!/usr/bin/env bash
#
# Glaze — Fedora development bootstrap.
#
# Checks for the build dependencies Glaze needs on a Fedora host and installs
# only the ones that are missing. It is idempotent: running it again when
# everything is present does nothing but report.
#
# Safety contract (see GLAZE.md §2 "Install safety" and AGENTS.md):
#   * This script NEVER touches the login manager, the bootloader, the default
#     session or any other desktop's configuration.
#   * It only installs the packages listed below, and only after explaining
#     what it is about to do.
#   * `--dry-run` prints the exact commands without changing anything.
#
# Supported: Fedora Linux on x86_64 and aarch64.
#
# Usage:
#   scripts/bootstrap-fedora.sh [--dry-run] [--yes] [--minimal] [--help]

set -euo pipefail

DRY_RUN=0
ASSUME_YES=0
MINIMAL=0

usage() {
    cat <<'EOF'
bootstrap-fedora.sh — install Glaze's Fedora build dependencies (idempotent)

USAGE:
    scripts/bootstrap-fedora.sh [OPTIONS]

OPTIONS:
    --dry-run   Show what would be installed/run; change nothing.
    --yes       Do not ask for confirmation before installing.
    --minimal   Install only the packages needed to build the portable
                (non-GTK) workspace; skip GTK4 / layer-shell / Wayland devel.
    -h, --help  Show this help and exit.

NOTES:
    * Requires a Fedora host (detected via /etc/os-release).
    * Uses `dnf` and, if not already root, `sudo` for the install step only.
    * Never modifies the login manager, bootloader or session configuration.
EOF
}

log()  { printf '\033[1;34m[glaze]\033[0m %s\n' "$*"; }
warn() { printf '\033[1;33m[glaze]\033[0m %s\n' "$*" >&2; }
die()  { printf '\033[1;31m[glaze]\033[0m %s\n' "$*" >&2; exit 1; }

# Run a command, or just print it under --dry-run.
run() {
    if [[ "$DRY_RUN" -eq 1 ]]; then
        printf '  [dry-run] %s\n' "$*"
    else
        "$@"
    fi
}

# --- argument parsing -------------------------------------------------------

while [[ $# -gt 0 ]]; do
    case "$1" in
        --dry-run) DRY_RUN=1 ;;
        --yes|-y)  ASSUME_YES=1 ;;
        --minimal) MINIMAL=1 ;;
        -h|--help) usage; exit 0 ;;
        *) die "unknown argument: $1 (try --help)" ;;
    esac
    shift
done

# --- host detection ---------------------------------------------------------

OS_ID="unknown"
OS_PRETTY="unknown"
if [[ -r /etc/os-release ]]; then
    # shellcheck disable=SC1091
    . /etc/os-release
    OS_ID="${ID:-unknown}"
    OS_PRETTY="${PRETTY_NAME:-unknown}"
fi

ARCH="$(uname -m)"
case "$ARCH" in
    x86_64|aarch64) ;;
    *) warn "untested architecture '$ARCH' (Glaze targets x86_64 and aarch64); continuing." ;;
esac

log "host: $OS_PRETTY"
log "arch: $ARCH"

if [[ "$OS_ID" != "fedora" ]]; then
    warn "this bootstrap targets Fedora Linux (detected ID='$OS_ID')."
    warn "On other distributions, install the equivalent packages manually:"
    warn "  rust/cargo, gtk4-devel, gtk4-layer-shell-devel, pkgconf,"
    warn "  wayland-devel, wayland-protocols-devel, mesa-libEGL-devel"
    if [[ "$DRY_RUN" -eq 0 ]]; then
        die "refusing to run dnf on a non-Fedora host (use --dry-run to preview)."
    fi
fi

if ! command -v dnf >/dev/null 2>&1; then
    die "dnf not found; is this a Fedora host?"
fi

# --- package sets -----------------------------------------------------------

# Build deps for the portable workspace + the GTK4 shell.
REQUIRED=(
    gcc
    gcc-c++
    make
    pkgconf-pkg-config
    rust
    cargo
    git
)

# Only needed to build/run the GTK4 shell, layer-shell integration and nested
# compositor development.
GTK_WAYLAND=(
    gtk4-devel
    gtk4-layer-shell-devel
    wayland-devel
    wayland-protocols-devel
    libxkbcommon-devel
    mesa-libEGL-devel
    mesa-libGL-devel
    systemd-devel
    desktop-file-utils
)

# Recommended developer conveniences (not build deps).
DEV_TOOLS=(
    sway
)

if [[ "$MINIMAL" -eq 1 ]]; then
    TARGETS=("${REQUIRED[@]}")
else
    TARGETS=("${REQUIRED[@]}" "${GTK_WAYLAND[@]}" "${DEV_TOOLS[@]}")
fi

# --- figure out what is missing (idempotency) -------------------------------

missing=()
present=()
for pkg in "${TARGETS[@]}"; do
    if rpm -q "$pkg" >/dev/null 2>&1; then
        present+=("$pkg")
    else
        missing+=("$pkg")
    fi
done

log "already installed: ${#present[@]} package(s)"
if [[ "${#present[@]}" -gt 0 ]]; then
    printf '    %s\n' "${present[@]}"
fi

if [[ "${#missing[@]}" -eq 0 ]]; then
    log "nothing to do — every dependency is present."
    log "next: scripts/build.sh && scripts/test.sh"
    exit 0
fi

log "missing: ${#missing[@]} package(s)"
printf '    %s\n' "${missing[@]}"

cat <<EOF

The following Fedora packages will be installed because Glaze needs them:
  * rust / cargo             — build the Rust workspace
  * gcc / make / pkgconf     — native build toolchain and dependency discovery
  * gtk4-devel               — GTK4 headers for apps/glaze-shell and glaze-settings
  * gtk4-layer-shell-devel   — Wayland layer-shell panel support
  * wayland-devel + protocols— Wayland client/server bindings
  * mesa-libEGL/GL-devel     — GPU/EGL headers for the compositor work
  * systemd-devel            — systemd --user / D-Bus integration
  * desktop-file-utils       — desktop entry validation + database refresh

Nothing here changes your session, login manager or boot configuration.

EOF

if [[ "$DRY_RUN" -eq 1 ]]; then
    run sudo dnf install -y "${missing[@]}"
    log "dry-run complete — nothing was changed."
    exit 0
fi

if [[ "$ASSUME_YES" -ne 1 ]]; then
    read -r -p "Install these packages now? [y/N] " reply
    case "$reply" in
        y|Y|yes|YES) ;;
        *) log "aborted by user."; exit 0 ;;
    esac
fi

# Use sudo only if we are not already root; never escalate beyond dnf.
if [[ "$(id -u)" -eq 0 ]]; then
    run dnf install -y "${missing[@]}"
else
    command -v sudo >/dev/null 2>&1 || die "sudo not found and not root."
    run sudo dnf install -y "${missing[@]}"
fi

log "bootstrap complete."
log "verify the toolchain with: rustc --version && cargo --version"
log "then build with:          scripts/build.sh"
log "and test with:            scripts/test.sh"
