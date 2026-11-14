#!/usr/bin/env bash
#
# Glaze — one-time system migration of the dev VM from the FeDe names
# (runs in the VM, idempotent). User config/data/state are copied by
# glaze-session itself (glaze_common::paths::migrate_legacy_dirs) and by the
# tty1 profile; this script does the system-wide parts that need sudo.
#
# Usage (VM): bash scripts/vm/migrate-glaze.sh

set -uo pipefail
cd "$(dirname "${BASH_SOURCE[0]}")/../.."
say() { echo "[migrate] $*"; }

# Helpers in /usr/local/bin: glaze-* replace fede-*.
sudo install -m 755 packaging/kilnlinux/bin/glaze-colour-scheme packaging/kilnlinux/bin/glaze-snap /usr/local/bin/
sudo rm -f /usr/local/bin/fede-colour-scheme /usr/local/bin/fede-snap
say "helpers: $(ls /usr/local/bin | grep -E '^(glaze|fede)-' | tr '\n' ' ')"
# Comments/paths in the lead's kiln-* helper and system files.
sudo sed -i 's/fede-colour-scheme/glaze-colour-scheme/g' /usr/local/bin/kiln-terminal-scheme
sudo install -m 644 packaging/kilnlinux/etc/profile.d/kiln-theme.sh /etc/profile.d/kiln-theme.sh
sudo sed -i 's/FeDe Settings/Glaze Settings/g' /etc/xdg/mimeapps.list
sudo sed -i 's|~/.config/fede/colour-scheme (written by fede-colour-scheme)|~/.config/glaze/colour-scheme (written by glaze-colour-scheme; kiln-terminal still reads the mirrored ~/.config/fede copy until it is rebuilt)|' /etc/xdg/kiln-terminal/kiln-terminal.ini
# Login profile.
cp -p ~/.bash_profile ~/.bash_profile.fede-bak 2>/dev/null || true
cp packaging/kilnlinux/bash_profile ~/.bash_profile
# User dirs (the same copy glaze-session does on start).
for b in ~/.config ~/.local/share ~/.local/state; do
    if [ -d "$b/fede" ] && [ ! -e "$b/glaze/.migrated-from-fede" ]; then
        mkdir -p "$b/glaze" && cp -an "$b/fede/." "$b/glaze/" && touch "$b/glaze/.migrated-from-fede" && say "merged $b/fede -> $b/glaze"
    fi
done
[ "$(cat ~/.config/glaze/session-backend 2>/dev/null)" = fede ] && echo glaze > ~/.config/glaze/session-backend
# Old launchers and binaries from the FeDe install.
rm -f ~/.local/share/applications/fede-*.desktop
for b in fede fede-service fede-session fede-settings fede-shell fede-compositor; do
    rm -f ~/.local/bin/$b
done
say "applications still mentioning fede:"
grep -ril fede ~/.local/share/applications /usr/share/applications 2>/dev/null || say "  none"
