#!/usr/bin/env bash
# vm-sync.sh TEMPLATE_DIRNAME...: push theme/ and icon-theme/ to the VM stage dir and install icon theme.
set -e
cd "$(dirname "$0")/.."
tar c theme icon-theme icons tools | $HOME/fedeos/vm 'mkdir -p ~/kiln-apps-build/stage && tar x -C ~/kiln-apps-build/stage'
$HOME/fedeos/vm 'sudo rm -rf /usr/local/share/icons/Kiln && sudo cp -r ~/kiln-apps-build/stage/icon-theme/Kiln /usr/local/share/icons/ && sudo gtk-update-icon-cache -qf /usr/local/share/icons/Kiln'
