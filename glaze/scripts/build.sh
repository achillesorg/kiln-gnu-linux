#!/usr/bin/env bash
#
# Glaze — build the workspace (and, on Linux, the GTK4 apps).
#
# The portable workspace (crates/) builds on any host with Rust. The GTK4 apps
# under apps/ need system GTK4 + gtk4-layer-shell and therefore only build on
# Linux; on any other host they are skipped with an explicit message.
#
# Usage:
#   scripts/build.sh [--release] [--workspace-only] [--help]

set -euo pipefail

PROFILE_FLAG=""
WORKSPACE_ONLY=0
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

usage() {
    cat <<'EOF'
build.sh — build Glaze

USAGE:
    scripts/build.sh [OPTIONS]

OPTIONS:
    --release         Build the optimized (release) profile.
    --workspace-only  Build only the portable workspace; skip the GTK apps.
    -h, --help        Show this help.

WHAT IT DOES:
    1. `cargo build` for the portable workspace at the repository root.
    2. On Linux, `cargo build` for every app manifest under apps/*/Cargo.toml
       (glaze-shell, glaze-settings). Off Linux those apps are skipped, because
       they require GTK4 + gtk4-layer-shell.
EOF
}

log()  { printf '\033[1;34m[glaze]\033[0m %s\n' "$*"; }
warn() { printf '\033[1;33m[glaze]\033[0m %s\n' "$*" >&2; }
die()  { printf '\033[1;31m[glaze]\033[0m %s\n' "$*" >&2; exit 1; }

while [[ $# -gt 0 ]]; do
    case "$1" in
        --release) PROFILE_FLAG="--release" ;;
        --workspace-only) WORKSPACE_ONLY=1 ;;
        -h|--help) usage; exit 0 ;;
        *) die "unknown argument: $1 (try --help)" ;;
    esac
    shift
done

command -v cargo >/dev/null 2>&1 || die "cargo not found. Run scripts/bootstrap-fedora.sh first."

cd "$ROOT"

log "building portable workspace ($ROOT) ${PROFILE_FLAG:-(debug)}"
cargo build ${PROFILE_FLAG:+$PROFILE_FLAG} --workspace

if [[ "$WORKSPACE_ONLY" -eq 1 ]]; then
    log "skipping GTK apps (--workspace-only)"
    exit 0
fi

if [[ "$(uname -s)" != "Linux" ]]; then
    warn "not running on Linux: skipping the GTK4 apps under apps/."
    warn "  They need GTK4 + gtk4-layer-shell; build them on Fedora with:"
    warn "    scripts/build.sh"
    exit 0
fi

shopt -s nullglob
app_manifests=("$ROOT"/apps/*/Cargo.toml)
shopt -u nullglob

if [[ "${#app_manifests[@]}" -eq 0 ]]; then
    warn "no app manifests under apps/ yet; nothing to build there."
    exit 0
fi

for manifest in "${app_manifests[@]}"; do
    app_dir="$(dirname "$manifest")"
    log "building app: $(basename "$app_dir")"
    cargo build ${PROFILE_FLAG:+$PROFILE_FLAG} --manifest-path "$manifest"
done

log "build complete."
