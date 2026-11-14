#!/usr/bin/env bash
#
# Type-check the GTK4 apps for the Linux targets WITHOUT needing GTK installed.
#
# Why this exists: `apps/glaze-shell` and `apps/glaze-settings` cannot be compiled
# on a non-Linux authoring host because `gtk4-sys`/`gio-sys` build scripts call
# `pkg-config` for the system GTK. `cargo check`/`cargo clippy` never link, so a
# stub `pkg-config` is enough to get the compiler to actually type-check the app
# code — which is how several real API/borrow errors were found.
#
# On Fedora (with gtk4-devel + gtk4-layer-shell-devel) run `scripts/build.sh`
# instead; this script is the fallback for hosts without GTK.
#
# Usage: scripts/typecheck-apps.sh [--target <triple>]...

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
STUB_MANIFEST="$ROOT/scripts/dev/pkg-config-stub/Cargo.toml"
TMP_DIR="$(mktemp -d)"
trap 'rm -rf "$TMP_DIR"' EXIT

TARGETS=("x86_64-unknown-linux-gnu" "aarch64-unknown-linux-gnu")
if [[ "${1:-}" == "--target" && -n "${2:-}" ]]; then
    shift
    TARGETS=("$@")
fi

log() { printf '\033[1;34m[glaze]\033[0m %s\n' "$*"; }

command -v cargo >/dev/null 2>&1 || { echo "cargo not found" >&2; exit 1; }

log "building pkg-config stub"
cargo build --release --manifest-path "$STUB_MANIFEST" --target-dir "$TMP_DIR/stub" >/dev/null

export PKG_CONFIG="$TMP_DIR/stub/release/pkg-config-stub"
export PKG_CONFIG_ALLOW_CROSS=1
export PKG_CONFIG_ALLOW_SYSTEM_CFLAGS=1
export PKG_CONFIG_ALLOW_SYSTEM_LIBS=1

shopt -s nullglob
manifests=("$ROOT"/apps/*/Cargo.toml)
shopt -u nullglob
if [[ "${#manifests[@]}" -eq 0 ]]; then
    echo "no app manifests under apps/" >&2
    exit 1
fi

status=0
for manifest in "${manifests[@]}"; do
    name="$(basename "$(dirname "$manifest")")"
    for target in "${TARGETS[@]}"; do
        log "check $name ($target)"
        cargo check --manifest-path "$manifest" --target "$target" || status=1
        log "clippy $name ($target) -D warnings"
        cargo clippy --manifest-path "$manifest" --target "$target" --all-targets -- -D warnings || status=1
    done
done

exit "$status"
