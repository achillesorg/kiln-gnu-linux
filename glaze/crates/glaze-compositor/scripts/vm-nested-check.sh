#!/usr/bin/env bash
#
# glaze-compositor M3 runtime checks in the KilnLinux VM.
#
# Everything runs under a PRIVATE runtime dir (/tmp/xdg-comp) with a throwaway
# headless Sway as the parent, so the live tty1 session (/run/user/1000) is
# never touched. Only processes this script started are stopped (by pid file).
#
# Steps: start parent (if needed) -> start glaze-compositor nested with a
# wallpaper (swaybg), two foot windows and glaze-shell (if built) -> list
# windows over wlr-foreign-toplevel -> screenshots -> tiling toggle over the
# control socket -> minimise / restore over foreign-toplevel -> frame-time log.
# Screenshots are taken from the PARENT with grim and copied to
# docs/screens/M3/ on the host.
#
# Usage: crates/glaze-compositor/scripts/vm-nested-check.sh [--keep]
#   --keep  leave the compositor running afterwards
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
VM="${GLAZE_VM:-$ROOT/../vm}"
OUT="$ROOT/docs/screens/M3"
KEEP=0
[[ "${1:-}" == "--keep" ]] && KEEP=1
mkdir -p "$OUT"

R=/tmp/xdg-comp
BIN='~/glaze-comp/target/debug'
ENV="export XDG_RUNTIME_DIR=$R WAYLAND_DISPLAY=wayland-1;"
WALL='~/glaze-comp/assets/wallpapers/kiln/kiln-light-landscape-1920x1080.png'

vm() { "$VM" "$@"; }
shot() { # name
    vm "$ENV grim $R/$1.png" && vm "cat $R/$1.png" > "$OUT/$1.png"
    echo "[shot] docs/screens/M3/$1.png"
}
# %q keeps multi-word titles ("Terminal two") as one argument across ssh
ctl() { vm "$ENV WAYLAND_DISPLAY=\$(cat $R/comp.socket) $BIN/glaze-toplevel-ctl $(printf '%q ' "$@")"; }
control() { # json
    vm "python3 -c 'import socket,sys; s=socket.socket(socket.AF_UNIX); s.connect(\"$R/glaze/compositor.sock\"); s.sendall(sys.argv[1].encode()+b\"\\n\"); print(s.makefile().readline().strip())' '$1'"
}

echo "== parent: throwaway headless sway in $R"
vm "mkdir -p $R && chmod 700 $R; if ! ls $R/sway-ipc.*.sock >/dev/null 2>&1; then
      cd $R; (XDG_RUNTIME_DIR=$R WLR_BACKENDS=headless WLR_RENDERER=pixman WLR_LIBINPUT_NO_DEVICES=1 setsid sway -c /dev/null > $R/sway.log 2>&1 & echo \$! > $R/sway.pid); sleep 2; fi
    XDG_RUNTIME_DIR=$R SWAYSOCK=\$(ls $R/sway-ipc.*.sock | head -1) swaymsg -q output HEADLESS-1 mode 1600x1000"

echo "== compositor"
vm "if [ -f $R/comp.pid ] && kill -0 \$(cat $R/comp.pid) 2>/dev/null; then kill \$(cat $R/comp.pid); sleep 1; fi
    # anchored with ^ so the pattern does not match this remote shell's own
    # command line (which contains the same text in the --spawn argument)
    pkill -u \$(id -u) -f '^foot --app-id glaze-check' || true
    $ENV cd $R; (RUST_LOG=info setsid $BIN/glaze-compositor \
        --spawn 'swaybg -m fill -i $WALL' \
        --spawn 'foot --app-id glaze-check --title \"Terminal one\"' \
        > $R/comp.log 2>&1 & echo \$! > $R/comp.pid); sleep 3
    sed -n 's/.*listening on the Wayland socket socket=\"\\(wayland-[0-9]*\\)\".*/\\1/p' $R/comp.log | head -1 > $R/comp.socket
    cat $R/comp.socket"
vm "$ENV WAYLAND_DISPLAY=\$(cat $R/comp.socket) setsid foot --app-id glaze-check --title 'Terminal two' >/dev/null 2>&1 &"
# glaze-shell: our own build in ~/glaze-comp if present, else the installed one
# (~/.local/bin, built by the shell workstream). Run read-only, never replaced.
SHELL_BIN=$(vm 'for b in ~/glaze-comp/apps/glaze-shell/target/debug/glaze-shell ~/.local/bin/glaze-shell; do [ -x "$b" ] && { echo "$b"; break; }; done' || true)
if [[ -n "$SHELL_BIN" ]]; then
    vm "$ENV export WAYLAND_DISPLAY=\$(cat $R/comp.socket) GLAZE_COMPOSITOR_SOCKET=$R/glaze/compositor.sock; cd $R; setsid $SHELL_BIN > $R/shell.log 2>&1 &"
    echo "glaze-shell started: $SHELL_BIN"
else
    echo "glaze-shell binary not found: skipped"
fi
sleep 4

echo "== windows via wlr-foreign-toplevel"
ctl list
shot nested-stacking

echo "== tiling toggle (control socket, active workspace)"
control '{"kind":"set-window-layout","mode":"toggle","workspace":0}'
sleep 1
shot nested-tiling
vm "grep 'layout target' $R/comp.log | tail -4 | sed 's/.*INFO //'"
control '{"kind":"list-windows"}'
control '{"kind":"set-window-layout","mode":"stacking","workspace":null}'
sleep 1
vm "grep 'layout target' $R/comp.log | tail -2 | sed 's/.*INFO //'"

echo "== minimise / restore via wlr-foreign-toplevel"
ctl minimize 'Terminal two'
sleep 1
ctl list
shot nested-minimized
ctl unminimize 'Terminal two'
sleep 1
ctl list
shot nested-restored

echo "== frame-time log"
sleep 5
vm "grep 'frame-time' $R/comp.log | tail -5 | sed 's/.*INFO //'"
vm "grep -E 'ERROR|panicked' $R/comp.log | head -5 || true"

if [[ "$KEEP" -eq 0 ]]; then
    vm "kill \$(cat $R/comp.pid) 2>/dev/null || true"
    echo "compositor stopped (parent sway left running in $R)"
fi
