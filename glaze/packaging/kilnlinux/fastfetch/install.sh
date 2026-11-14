#!/usr/bin/env bash
# Install the Kiln GNU/Linux fastfetch branding system-wide (run on the Kiln machine, needs sudo).
# - ASCII arch logo -> /usr/share/kiln/logo.txt
# - config          -> /etc/xdg/fastfetch/config.jsonc (used by every user without their own config)
# - os-release      -> PRETTY_NAME without the word Debian
set -euo pipefail
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
sudo install -D -m 0644 "$HERE/logo.txt" /usr/share/kiln/logo.txt
sudo install -D -m 0644 "$HERE/config.jsonc" /etc/xdg/fastfetch/config.jsonc
sudo sed -i 's/^PRETTY_NAME=.*/PRETTY_NAME="Kiln GNU\/Linux"/' /etc/os-release
echo "installed; try: fastfetch"
