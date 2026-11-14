#!/usr/bin/env bash
# Kiln GNU/Linux: enable extra repositories and preinstall everyday apps (run on the Kiln machine, needs sudo + network).
#  - Debian contrib/non-free/non-free-firmware components (codecs, firmware)
#  - Mozilla's official apt repo -> regular Firefox (not ESR), key fingerprint verified
#  - Flatpak + Flathub remote
#  - the package set in packages.list
# Idempotent: safe to re-run. Log: /var/log/kiln-provision.log (when run with sudo tee) or stdout.
set -euo pipefail
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
export DEBIAN_FRONTEND=noninteractive
MOZ_FPR="35BAA0B33E9EB396F59CA838C0BA5CE6DC6315A3"

echo "== 1/5 Debian components"
if ! grep -q "non-free-firmware" /etc/apt/sources.list.d/debian.sources; then
  sudo sed -i 's/^Components: main$/Components: main contrib non-free non-free-firmware/' /etc/apt/sources.list.d/debian.sources
fi

echo "== 2/5 Mozilla apt repo (Firefox)"
sudo install -d -m 0755 /etc/apt/keyrings
if [ ! -s /etc/apt/keyrings/packages.mozilla.org.asc ]; then
  sudo apt-get update -qq
  sudo apt-get install -y -qq wget gnupg ca-certificates
  wget -q https://packages.mozilla.org/apt/repo-signing-key.gpg -O /tmp/moz.gpg
  export GNUPGHOME="$(mktemp -d)"
  got="$(gpg -n -q --import --import-options import-show /tmp/moz.gpg | awk '/^ +[0-9A-F]{40}$/{gsub(/ /,""); print; exit}')"
  if [ "$got" != "$MOZ_FPR" ]; then echo "Mozilla key fingerprint mismatch: $got" >&2; exit 1; fi
  sudo install -m 0644 /tmp/moz.gpg /etc/apt/keyrings/packages.mozilla.org.asc
fi
echo "deb [signed-by=/etc/apt/keyrings/packages.mozilla.org.asc] https://packages.mozilla.org/apt mozilla main" | sudo tee /etc/apt/sources.list.d/mozilla.list >/dev/null
printf 'Package: *\nPin: origin packages.mozilla.org\nPin-Priority: 1000\n' | sudo tee /etc/apt/preferences.d/mozilla >/dev/null

echo "== 3/5 apt update"
sudo apt-get update -qq

echo "== 4/5 packages"
mapfile -t PKGS < <(grep -v '^\s*#' "$HERE/packages.list" | grep -v '^\s*$')
sudo apt-get install -y --no-install-recommends "${PKGS[@]}"

echo "== 5/5 Flatpak + Flathub"
sudo flatpak remote-add --if-not-exists flathub https://dl.flathub.org/repo/flathub.flatpakrepo || echo "flathub remote not added (offline?)"

echo "done"
