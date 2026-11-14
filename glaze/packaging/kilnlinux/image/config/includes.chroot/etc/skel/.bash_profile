# ~/.bash_profile for the Kiln GNU/Linux live image (user glaze, tty1 autologin).
# getty autologs glaze in on tty1 (systemd drop-in in /etc/systemd/system/getty@tty1.service.d).
# pam_systemd has started `systemd --user` and, with dbus-user-session, its session bus.
[ -f ~/.bashrc ] && . ~/.bashrc
export PATH="/usr/local/bin:$HOME/.local/bin:$PATH"

if [ -z "$WAYLAND_DISPLAY" ] && [ "$(tty)" = /dev/tty1 ]; then
    export DBUS_SESSION_BUS_ADDRESS="unix:path=${XDG_RUNTIME_DIR:-/run/user/$(id -u)}/bus"
    # virtio-gpu without virgl: software cursors are more reliable.
    export WLR_NO_HARDWARE_CURSORS=1
    # Session compositor: kernel cmdline kiln.session=sway (boot menu "safe graphics"), else $GLAZE_BACKEND,
    # else ~/.config/glaze/session-backend, else "glaze" (Glaze DRM compositor; glaze-session falls back
    # to Sway by itself when the compositor is missing, finds no GPU or crashes at start).
    backend=""
    case " $(cat /proc/cmdline 2>/dev/null) " in *" kiln.session=sway "*) backend=sway ;; esac
    [ -n "$backend" ] || backend="${GLAZE_BACKEND:-$(cat "$HOME/.config/glaze/session-backend" 2>/dev/null)}"
    case "$backend" in glaze|sway) ;; *) backend=glaze ;; esac
    exec glaze-session run --backend "$backend" >"$HOME/session.log" 2>&1
fi
