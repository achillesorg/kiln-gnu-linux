#!/usr/bin/env bash
#
# glaze-compositor: rsync the repo into the KilnLinux VM's ~/glaze-comp (NOT ~/glaze,
# which belongs to the live session builds) and build the compositor there.
#
# Usage: crates/glaze-compositor/scripts/vm-sync-build.sh [--release] [--shell]
#   --release  build with the release profile (default: debug, faster to build)
#   --shell    also build apps/glaze-shell (the GTK layer-shell panel)
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
KEY="${GLAZE_VM_KEY:-$ROOT/../fedeos_key}"
PORT="${GLAZE_VM_PORT:-2222}"
USER_AT="${GLAZE_VM_USER:-fede}@localhost"
DIR="${GLAZE_COMP_VM_DIR:-glaze-comp}"
PROFILE=""
SHELL_TOO=0
while [[ $# -gt 0 ]]; do
    case "$1" in
        --release) PROFILE="--release" ;;
        --shell) SHELL_TOO=1 ;;
        *) echo "unknown argument: $1" >&2; exit 2 ;;
    esac
    shift
done
SSH_OPTS=(-i "$KEY" -p "$PORT" -o StrictHostKeyChecking=no -o UserKnownHostsFile=/dev/null
          -o LogLevel=ERROR -o BatchMode=yes)

rsync -a --delete --exclude .git --exclude target --exclude 'apps/*/target' \
    --exclude 'docs/screens' -e "ssh ${SSH_OPTS[*]}" "$ROOT/" "$USER_AT:$DIR/"
ssh "${SSH_OPTS[@]}" "$USER_AT" ". ~/.cargo/env && cd ~/$DIR && flock /tmp/heavy-build.lock cargo build -j2 $PROFILE -p glaze-compositor 2>&1 | grep -E '^(error|warning: unused)|^\s+-->|Finished|error\[' | head -80"
if [[ "$SHELL_TOO" -eq 1 ]]; then
    ssh "${SSH_OPTS[@]}" "$USER_AT" ". ~/.cargo/env && cd ~/$DIR/apps/glaze-shell && flock /tmp/heavy-build.lock cargo build -j2 $PROFILE 2>&1 | tail -3"
fi
