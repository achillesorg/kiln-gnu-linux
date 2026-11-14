#!/usr/bin/env bash
# Glaze M4: start a client inside the tty2 Glaze session (detached).
# Usage: scripts/vm/m4-run.sh 'foot --title T'
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
VM="${GLAZE_VM:-$ROOT/../vm}"
R=/run/user/1000/glaze-tty2
"$VM" "pid=\$(for p in \$(pgrep -u \$(id -u) -x glaze-shell); do grep -qz '^XDG_RUNTIME_DIR=$R\$' /proc/\$p/environ && echo \$p; done | head -1)
  [ -n \"\$pid\" ] || { echo 'no tty2 glaze-shell' >&2; exit 1; }
  env -i \$(tr '\0' '\n' </proc/\$pid/environ | grep -E '^(HOME|USER|PATH|LANG|XDG_[A-Z_]*|WAYLAND_DISPLAY|DISPLAY|DBUS_SESSION_BUS_ADDRESS|GLAZE_[A-Z_]*|GDK_BACKEND|QT_[A-Z_]*|MOZ_ENABLE_WAYLAND)=') \
    setsid sh -c '$1' >/dev/null 2>&1 < /dev/null & echo started"
