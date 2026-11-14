#!/usr/bin/env bash
#
# Glaze — quick smoke test.
#
# Runs the portable checks that work on any host with bash + cargo, and adds the
# Linux-only readiness report when asked. It installs nothing and never touches
# the running session.
#
# Usage:
#   tests/smoke.sh [--with-check] [--help]

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
WITH_CHECK=0
FAILED=0

while [[ $# -gt 0 ]]; do
    case "$1" in
        --with-check) WITH_CHECK=1 ;;
        -h|--help) sed -n '2,12p' "$0"; exit 0 ;;
        *) printf 'unknown argument: %s\n' "$1" >&2; exit 2 ;;
    esac
    shift
done

log()  { printf '\033[1;34m[glaze-smoke]\033[0m %s\n' "$*"; }
warn() { printf '\033[1;33m[glaze-smoke]\033[0m %s\n' "$*" >&2; }

cd "$ROOT"

step() {
    local label="$1"; shift
    log "$label"
    if "$@"; then
        log "ok: $label"
    else
        FAILED=1
        warn "FAILED: $label"
    fi
}

command -v cargo >/dev/null 2>&1 || { warn "cargo not found"; exit 1; }

step "scripts syntax (bash -n)" bash -c 'for f in scripts/*.sh tests/smoke.sh; do bash -n "$f" || exit 1; done'
step "cargo test --workspace"   cargo test --workspace --quiet
step "cargo fmt --all --check"  cargo fmt --all -- --check

if [[ "$WITH_CHECK" -eq 1 ]]; then
    if [[ "$(uname -s)" == "Linux" ]]; then
        step "scripts/check.sh" scripts/check.sh
    else
        warn "not Linux: skipping scripts/check.sh"
    fi
fi

echo
if [[ "$FAILED" -eq 0 ]]; then
    log "SMOKE OK"
else
    warn "SMOKE FAILED"
fi
exit "$FAILED"
