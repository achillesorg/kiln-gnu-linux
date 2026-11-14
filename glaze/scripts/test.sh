#!/usr/bin/env bash
#
# Glaze — run the full check suite.
#
# 1. cargo fmt --all -- --check
# 2. cargo clippy --workspace --all-targets -- -D warnings
# 3. cargo test --workspace
# 4. Accessibility audit (glaze-a11y-report must report 0 failures).
# 5. Cross-target compile checks (x86_64/aarch64 linux) when the rustup
#    targets are installed — this is what catches Linux-only code that never
#    compiles on a non-Linux dev host.
# 6. On Linux only: build the GTK4 apps under apps/ (they need GTK4 +
#    gtk4-layer-shell), so their code is at least compiled.
#
# Usage:
#   scripts/test.sh [--no-apps] [--no-clippy] [--no-cross] [--help]

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
RUN_APPS=1
RUN_CLIPPY=1
RUN_CROSS=1
FAILED=0

usage() {
    cat <<'EOF'
test.sh — format, lint and test Glaze

USAGE:
    scripts/test.sh [OPTIONS]

OPTIONS:
    --no-apps     Skip building the GTK4 apps (useful off Linux / fast checks).
    --no-clippy   Skip the clippy pass.
    --no-cross    Skip the cross-target compile checks.
    -h, --help    Show this help.
EOF
}

log()  { printf '\033[1;34m[glaze]\033[0m %s\n' "$*"; }
warn() { printf '\033[1;33m[glaze]\033[0m %s\n' "$*" >&2; }
die()  { printf '\033[1;31m[glaze]\033[0m %s\n' "$*" >&2; exit 1; }

while [[ $# -gt 0 ]]; do
    case "$1" in
        --no-apps)   RUN_APPS=0 ;;
        --no-clippy) RUN_CLIPPY=0 ;;
        --no-cross)  RUN_CROSS=0 ;;
        -h|--help) usage; exit 0 ;;
        *) die "unknown argument: $1 (try --help)" ;;
    esac
    shift
done

command -v cargo >/dev/null 2>&1 || die "cargo not found. Run scripts/bootstrap-fedora.sh first."
cd "$ROOT"

step() {
    # Run a command, record failure but keep going so all issues are reported.
    local label="$1"; shift
    log "$label"
    if "$@"; then
        log "ok: $label"
    else
        FAILED=1
        warn "FAILED: $label"
    fi
}

step "cargo fmt --all -- --check" \
    cargo fmt --all -- --check

if [[ "$RUN_CLIPPY" -eq 1 ]]; then
    step "cargo clippy --workspace --all-targets -- -D warnings" \
        cargo clippy --workspace --all-targets -- -D warnings
fi

step "cargo test --workspace" \
    cargo test --workspace

# --- Accessibility audit ----------------------------------------------------

step "glaze-a11y report (WCAG 2.2 AA)" \
    cargo run -q -p glaze-a11y --bin glaze-a11y-report

# --- Cross-target compile checks --------------------------------------------
#
# Compile/lint the workspace for the supported Linux targets. These are
# compile-only (no linking, no execution) but they are the mechanism that
# catches `#[cfg(unix)]` code which a non-Linux host would otherwise never
# type-check. Skipped automatically when the rustup target is not installed.

cross_target() {
    local target="$1"
    if rustup target list --installed 2>/dev/null | grep -qx "$target"; then
        step "cargo check --workspace --all-targets --target $target" \
            cargo check --workspace --all-targets --target "$target"
    else
        warn "target $target not installed; skipping (run: rustup target add $target)"
    fi
}

if [[ "$RUN_CROSS" -eq 1 ]]; then
    if command -v rustup >/dev/null 2>&1; then
        cross_target x86_64-unknown-linux-gnu
        cross_target aarch64-unknown-linux-gnu
        if rustup target list --installed 2>/dev/null | grep -qx aarch64-unknown-linux-gnu; then
            step "cargo clippy aarch64 (deny warnings)" \
                cargo clippy --workspace --all-targets --target aarch64-unknown-linux-gnu -- -D warnings
        fi
    else
        warn "rustup not found; skipping cross-target checks."
    fi
fi

# --- GTK apps (Linux only) --------------------------------------------------

if [[ "$RUN_APPS" -eq 1 ]]; then
    if [[ "$(uname -s)" != "Linux" ]]; then
        warn "not Linux: skipping GTK app builds."
    else
        shopt -s nullglob
        app_manifests=("$ROOT"/apps/*/Cargo.toml)
        shopt -u nullglob
        if [[ "${#app_manifests[@]}" -eq 0 ]]; then
            warn "no app manifests under apps/ yet."
        else
            for manifest in "${app_manifests[@]}"; do
                step "build app $(basename "$(dirname "$manifest")")" \
                    cargo build --manifest-path "$manifest"
            done
        fi
    fi
fi

echo
if [[ "$FAILED" -eq 0 ]]; then
    log "ALL CHECKS PASSED"
else
    warn "SOME CHECKS FAILED — see the log above."
fi
exit "$FAILED"
