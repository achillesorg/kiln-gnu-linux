#!/usr/bin/env bash
#
# Glaze M4: make the Glaze compositor the tty1 session (next login), keeping
# Sway as the automatic fallback, and leave a rollback note in the VM.
#   * installs the M4 builds from ~/.local/glaze-m4/bin into ~/.local/bin
#     (previous binaries kept as ~/.local/bin/<name>.pre-glaze);
#   * writes `glaze` to ~/.config/glaze/session-backend (read by ~/.bash_profile);
#   * writes ~/README-glaze-session.txt.
# Usage: scripts/vm/make-glaze-default.sh [--rollback]
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
VM="${GLAZE_VM:-$ROOT/../vm}"
if [[ "${1:-}" == "--rollback" ]]; then
    "$VM" 'echo sway > ~/.config/glaze/session-backend; echo "tty1 session: sway (next login)"'
    exit 0
fi
"$ROOT/scripts/vm/install-login.sh"
"$VM" 'set -e
  for b in glaze-compositor glaze-session glaze-shell glaze-toplevel-ctl; do
    [ -x ~/.local/glaze-m4/bin/$b ] || continue
    [ -e ~/.local/bin/$b ] && [ ! -e ~/.local/bin/$b.pre-glaze ] && cp -p ~/.local/bin/$b ~/.local/bin/$b.pre-glaze
    install -m 755 ~/.local/glaze-m4/bin/$b ~/.local/bin/$b
  done
  mkdir -p ~/.config/glaze && echo glaze > ~/.config/glaze/session-backend
  cat > ~/README-glaze-session.txt <<TXT
Glaze session (Kiln GNU/Linux dev VM)
====================================

tty1 now logs in to the Glaze compositor (glaze-compositor) instead of Sway.
Chosen in ~/.config/glaze/session-backend ("glaze" or "sway"), read by
~/.bash_profile. glaze-session falls back to Sway by itself when the
compositor is missing, does not start within 20 s, or crashed in the last
10 minutes (~/.local/state/glaze/compositor-failed). Log: ~/session.log.

Roll back to Sway (takes effect at the next login / reboot):
    echo sway > ~/.config/glaze/session-backend
or for one login:   GLAZE_BACKEND=sway   (in the environment of the tty1 login)
or restore the old profile:   cp ~/.bash_profile.glaze-bak ~/.bash_profile
Previous binaries: ~/.local/bin/*.pre-glaze

Keys: Super+arrows snap, Super+M minimise, Super+Shift+M restore, Alt+Tab,
Super+T tiling, Super+L lock (swaylock), Super+Return terminal,
Ctrl+Alt+F1..F12 switch VT, Ctrl+Alt+Backspace quits the compositor.
TXT
  echo "tty1 session: glaze (next login); note: ~/README-glaze-session.txt"'
