#!/usr/bin/env bash
#
# Glaze — sync this tree into the KilnLinux QEMU VM and build it there.
#
# The VM copy (~/glaze) is not a git repo; it is overwritten from the host tree
# (excluding .git and target/). Build output stays in the VM.
#
# Usage:
#   scripts/vm/sync-build.sh [--debug] [--install] [--no-build]
#
# Environment (defaults match $HOME/fedeos/run.sh):
#   GLAZE_VM_KEY   ssh key           (default: <repo>/../fedeos_key)
#   GLAZE_VM_PORT  forwarded ssh port (default: 2222)
#   GLAZE_VM_USER  VM user            (default: fede)
#   GLAZE_VM_DIR   VM checkout        (default: ~/glaze)

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
KEY="${GLAZE_VM_KEY:-$ROOT/../fedeos_key}"
PORT="${GLAZE_VM_PORT:-2222}"
USER_AT="${GLAZE_VM_USER:-fede}@localhost"
DIR="${GLAZE_VM_DIR:-glaze}"
PROFILE="--release"
INSTALL=0
BUILD=1

while [[ $# -gt 0 ]]; do
    case "$1" in
        --debug) PROFILE="" ;;
        --install) INSTALL=1 ;;
        --no-build) BUILD=0 ;;
        -h|--help) sed -n '2,16p' "$0"; exit 0 ;;
        *) echo "unknown argument: $1" >&2; exit 2 ;;
    esac
    shift
done

SSH_OPTS=(-i "$KEY" -p "$PORT" -o StrictHostKeyChecking=no -o UserKnownHostsFile=/dev/null
          -o LogLevel=ERROR -o BatchMode=yes)

echo "[vm] rsync $ROOT/ -> $USER_AT:~/$DIR/"
rsync -a --exclude .git --exclude target --exclude 'apps/*/target' \
    -e "ssh ${SSH_OPTS[*]}" "$ROOT/" "$USER_AT:$DIR/"

if [[ "$BUILD" -eq 1 ]]; then
    echo "[vm] scripts/build.sh $PROFILE"
    ssh "${SSH_OPTS[@]}" "$USER_AT" ". ~/.cargo/env && cd ~/$DIR && bash scripts/build.sh $PROFILE"
fi
if [[ "$INSTALL" -eq 1 ]]; then
    echo "[vm] scripts/install-session.sh $PROFILE"
    ssh "${SSH_OPTS[@]}" "$USER_AT" "cd ~/$DIR && bash scripts/install-session.sh $PROFILE"
    # Always make the live tty1 session pick up the new binaries.
    echo "[vm] restart glaze-service + glaze-shell in the live session"
    ssh "${SSH_OPTS[@]}" "$USER_AT" "cd ~/$DIR && bash scripts/vm/restart-live.sh" || \
        echo "[vm] live restart failed (see above)" >&2
fi
