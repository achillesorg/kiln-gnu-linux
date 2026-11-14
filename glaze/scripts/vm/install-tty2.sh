#!/usr/bin/env bash
#
# Glaze M4: install (or remove) the tty2 test path for the Glaze compositor
# session, without touching the tty1 Sway session.
#
#   * /etc/systemd/system/getty@tty2.service.d/autologin.conf (from
#     packaging/kilnlinux/systemd/getty@tty2-autologin.conf): autologin glaze
#     on tty2, like tty1;
#   * ~/.bash_profile from packaging/kilnlinux/bash_profile (tty1 branch
#     unchanged; the tty2 branch waits until tty2 is the active VT, then runs
#     `dbus-run-session glaze-session run --backend glaze` with a private
#     XDG_RUNTIME_DIR); backup in ~/.bash_profile.glaze-bak (first run only);
#   * the flag ~/.config/glaze/tty2-session that enables the tty2 branch.
#
# Then: `sudo chvt 2` to start/see the Glaze session, `sudo chvt 1` to go back.
# Log: ~/session-tty2.log.
#
# Usage: scripts/vm/install-tty2.sh [--remove]
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
VM="${GLAZE_VM:-$ROOT/../vm}"

if [[ "${1:-}" == "--remove" ]]; then
    "$VM" 'rm -f ~/.config/glaze/tty2-session
           sudo rm -f /etc/systemd/system/getty@tty2.service.d/autologin.conf
           sudo rmdir /etc/systemd/system/getty@tty2.service.d 2>/dev/null || true
           sudo systemctl daemon-reload; sudo systemctl restart getty@tty2.service'
    echo "[vm] tty2 test path removed (getty@tty2 back to a login prompt)"
    exit 0
fi

"$ROOT/scripts/vm/install-login.sh"
"$VM" 'sudo mkdir -p /etc/systemd/system/getty@tty2.service.d && sudo tee /etc/systemd/system/getty@tty2.service.d/autologin.conf >/dev/null' \
    <"$ROOT/packaging/kilnlinux/systemd/getty@tty2-autologin.conf"
"$VM" 'mkdir -p ~/.config/glaze && touch ~/.config/glaze/tty2-session
       sudo systemctl daemon-reload && sudo systemctl enable getty@tty2.service && sudo systemctl restart getty@tty2.service
       systemctl is-active getty@tty2.service'
echo "[vm] tty2 test path installed: sudo chvt 2 starts the Glaze session, sudo chvt 1 returns"
