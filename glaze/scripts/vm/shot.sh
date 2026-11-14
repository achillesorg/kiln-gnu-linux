#!/usr/bin/env bash
#
# Glaze — screenshot the KilnLinux VM into docs/screens/<milestone>/NAME.png.
#
# 1. QEMU QMP `screendump` on the host (works with any compositor, needs
#    `-qmp unix:<sock>,server,nowait` in $HOME/fedeos/run.sh).
# 2. Fallback: `grim` inside the running Sway session over ssh.
#
# Usage: scripts/vm/shot.sh NAME [MILESTONE]     (MILESTONE default: M0)
# Environment: GLAZE_QMP_SOCK (default: <repo>/../qmp.sock), GLAZE_VM_KEY,
#              GLAZE_VM_PORT, GLAZE_VM_USER, GLAZE_SHOT_DELAY (seconds, default 2).
#
# Caveat (seen in M0): QEMU's screendump can return a frame a few seconds old
# right after the guest changes the screen, so the capture waits
# GLAZE_SHOT_DELAY first. Check pixels against a scripted state when it matters.
# GLAZE_SHOT_GRIM=1 skips QMP and captures the compositor output with grim
# (exact current frame, no cursor).
# Prints the PNG path and how it was captured.

set -euo pipefail

NAME="${1:?usage: shot.sh NAME [MILESTONE]}"
MILESTONE="${2:-M0}"
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
SOCK="${GLAZE_QMP_SOCK:-$ROOT/../qmp.sock}"
KEY="${GLAZE_VM_KEY:-$ROOT/../fedeos_key}"
PORT="${GLAZE_VM_PORT:-2222}"
USER_AT="${GLAZE_VM_USER:-fede}@localhost"
OUT_DIR="$ROOT/docs/screens/$MILESTONE"
OUT="$OUT_DIR/$NAME.png"
mkdir -p "$OUT_DIR"
sleep "${GLAZE_SHOT_DELAY:-2}"

if [[ "${GLAZE_SHOT_GRIM:-0}" != 1 && -S "$SOCK" ]] && python3 "$ROOT/scripts/vm/qmp_screendump.py" "$SOCK" "$OUT"; then
    echo "$OUT (qmp screendump via $SOCK)"
    exit 0
fi
echo "[shot] QMP unavailable at $SOCK; falling back to grim over ssh" >&2
# grim would capture the tty1 Sway session, not the active VT: opt out.
[[ "${GLAZE_SHOT_NOGRIM:-0}" == 1 ]] && { echo "[shot] no grim fallback (GLAZE_SHOT_NOGRIM=1)" >&2; exit 1; }

SSH_OPTS=(-i "$KEY" -p "$PORT" -o StrictHostKeyChecking=no -o UserKnownHostsFile=/dev/null
          -o LogLevel=ERROR -o BatchMode=yes)
# Borrow the session's WAYLAND_DISPLAY from the running glaze-shell.
ssh "${SSH_OPTS[@]}" "$USER_AT" '
    pid=$(pgrep -u "$(id -u)" -x glaze-shell | head -n1)
    [ -n "$pid" ] || { echo "no glaze-shell running" >&2; exit 1; }
    export XDG_RUNTIME_DIR=/run/user/$(id -u)
    export $(tr "\0" "\n" </proc/$pid/environ | grep "^WAYLAND_DISPLAY=")
    grim -t png -' >"$OUT"
echo "$OUT (grim over ssh)"
