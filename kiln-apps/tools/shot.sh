#!/usr/bin/env bash
# shot.sh NAME : grim the live tty1 Sway output (wayland-1) into kiln-apps/screens/NAME.png
set -e
D="$(cd "$(dirname "$0")/.." && pwd)"
$HOME/fedeos/vm 'XDG_RUNTIME_DIR=/run/user/1000 WAYLAND_DISPLAY=wayland-1 grim /tmp/kiln-shot.png'
$HOME/fedeos/vm 'base64 /tmp/kiln-shot.png' | base64 -D > "$D/screens/$1.png"
echo "$D/screens/$1.png"
