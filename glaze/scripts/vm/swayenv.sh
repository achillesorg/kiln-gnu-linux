#!/usr/bin/env bash
# Glaze — source in the VM to talk to the live tty1 sway session:
#   . ~/glaze/scripts/vm/swayenv.sh; swaymsg -t get_tree
export XDG_RUNTIME_DIR=/run/user/$(id -u)
export SWAYSOCK=$(ls "$XDG_RUNTIME_DIR"/sway-ipc.*.sock 2>/dev/null | head -n1)
export WAYLAND_DISPLAY=$(cd "$XDG_RUNTIME_DIR" && ls wayland-[0-9]* 2>/dev/null | grep -v lock | head -n1)
