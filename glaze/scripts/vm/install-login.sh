#!/usr/bin/env bash
#
# Glaze — install the tty1 autologin profile into the KilnLinux VM.
#
# Copies packaging/kilnlinux/bash_profile to the VM user's ~/.bash_profile,
# keeping the previous file as ~/.bash_profile.glaze-bak (first run only).
# Takes effect at the next tty1 login (reboot the VM, or restart getty@tty1).
#
# Usage: scripts/vm/install-login.sh

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
KEY="${GLAZE_VM_KEY:-$ROOT/../fedeos_key}"
PORT="${GLAZE_VM_PORT:-2222}"
USER_AT="${GLAZE_VM_USER:-fede}@localhost"
SSH_OPTS=(-i "$KEY" -p "$PORT" -o StrictHostKeyChecking=no -o UserKnownHostsFile=/dev/null
          -o LogLevel=ERROR -o BatchMode=yes)

ssh "${SSH_OPTS[@]}" "$USER_AT" \
    '[ -f ~/.bash_profile.glaze-bak ] || cp -p ~/.bash_profile ~/.bash_profile.glaze-bak 2>/dev/null || true; cat > ~/.bash_profile' \
    <"$ROOT/packaging/kilnlinux/bash_profile"
echo "[vm] installed ~/.bash_profile (backup: ~/.bash_profile.glaze-bak)"
