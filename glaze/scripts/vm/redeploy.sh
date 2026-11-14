#!/usr/bin/env bash
#
# Glaze — one-shot redeploy into the live KilnLinux VM session:
#   rsync tree -> build (shell-side crates + GTK apps) -> install -> restart
#   glaze-service and glaze-shell inside the running tty1 session.
#
# Only the shell-side binaries are rebuilt (glaze-service, glaze-session, glaze,
# glaze-shell, glaze-settings); the compositor crate is left to its own agent
# and its last built binary is reinstalled as is.
#
# Usage: scripts/vm/redeploy.sh [--debug] [--no-restart] [--shell-only]
#   --shell-only  skip the glaze-settings build (keeps its installed binary)
# Environment: as scripts/vm/sync-build.sh (GLAZE_VM_KEY, GLAZE_VM_PORT, ...).

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
KEY="${GLAZE_VM_KEY:-$ROOT/../fedeos_key}"
PORT="${GLAZE_VM_PORT:-2222}"
USER_AT="${GLAZE_VM_USER:-fede}@localhost"
DIR="${GLAZE_VM_DIR:-glaze}"
PROFILE="--release"
RESTART=1
SETTINGS=1
while [[ $# -gt 0 ]]; do
    case "$1" in
        --debug) PROFILE="" ;;
        --no-restart) RESTART=0 ;;
        --shell-only) SETTINGS=0 ;;
        -h|--help) sed -n '2,14p' "$0"; exit 0 ;;
        *) echo "unknown argument: $1" >&2; exit 2 ;;
    esac
    shift
done
SSH_OPTS=(-i "$KEY" -p "$PORT" -o StrictHostKeyChecking=no -o UserKnownHostsFile=/dev/null
          -o LogLevel=ERROR -o BatchMode=yes)

echo "[redeploy] rsync $ROOT/ -> $USER_AT:~/$DIR/"
rsync -a --exclude .git --exclude target --exclude 'apps/*/target' \
    -e "ssh ${SSH_OPTS[*]}" "$ROOT/" "$USER_AT:$DIR/"

echo "[redeploy] build (shell side) ${PROFILE:-debug} (-j2, /tmp/heavy-build.lock, detached)"
# Detached in the VM (survives ssh drops) and serialised with the other
# agents' builds through the shared lock; -j2 keeps the session responsive.
ssh "${SSH_OPTS[@]}" "$USER_AT" "rm -f ~/redeploy-build.done; nohup sh -c '. ~/.cargo/env; cd ~/$DIR &&
    flock /tmp/heavy-build.lock cargo build -j2 $PROFILE -p glaze-service -p glaze-session -p glaze-cli &&
    (cd apps/glaze-shell && flock /tmp/heavy-build.lock cargo build -j2 $PROFILE) &&
    { [ $SETTINGS = 0 ] || (cd apps/glaze-settings && flock /tmp/heavy-build.lock cargo build -j2 $PROFILE) ||
      echo WARNING: glaze-settings did not build, keeping its previous binary; };
    echo \$? > ~/redeploy-build.done' > ~/redeploy-build.log 2>&1 < /dev/null &"
until ssh "${SSH_OPTS[@]}" -o ConnectTimeout=20 "$USER_AT" 'test -f ~/redeploy-build.done' 2>/dev/null; do sleep 10; done
ssh "${SSH_OPTS[@]}" "$USER_AT" "grep -E '^(error|warning: unused)|^\s+-->|Finished|WARNING' ~/redeploy-build.log | head -40; exit \$(cat ~/redeploy-build.done)"

echo "[redeploy] install"
ssh "${SSH_OPTS[@]}" "$USER_AT" "cd ~/$DIR && bash scripts/install-session.sh $PROFILE 2>&1 | tail -n 2"

if [[ "$RESTART" -eq 1 ]]; then
    echo "[redeploy] restart live session processes"
    ssh "${SSH_OPTS[@]}" "$USER_AT" "cd ~/$DIR && GLAZE_GSK_RENDERER=${GLAZE_GSK_RENDERER:-} bash scripts/vm/restart-live.sh"
fi
