#!/usr/bin/env bash
# Stage the NON-apt parts of Kiln GNU/Linux (built Glaze session, Kiln apps, Plymouth theme) into
# config/includes.chroot, so `lb build` bakes in exactly what runs on the dev VM.
# Run ON THE VM, after `scripts/vm/sync-build.sh --install` (Glaze in ~/.local) and `kiln-apps/build-all.sh` (apps in /usr/local).
# Everything staged here is gitignored (binary build output). Safe to re-run.
set -euo pipefail
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
INC="$HERE/config/includes.chroot"
LOCAL_SRC="${KILN_LOCAL:-/usr/local}"          # Kiln apps (build-all.sh installs here)
GLAZE_SRC="${GLAZE_PREFIX:-$HOME/.local}"       # deployed Glaze binaries (sync-build.sh --install)
GLAZE_REPO="${GLAZE_REPO:-$HOME/glaze-comp}"    # synced repo checkout (themes, assets, sway.conf)
mkdir -p "$INC/usr/local" "$INC/usr/share/plymouth/themes" "$INC/etc/skel/.config/glaze"

echo "== Kiln apps: all of $LOCAL_SRC (build-all.sh installs only Kiln apps there; ~26 MB)"
rsync -a --exclude 'bin/fede-*' --exclude 'include' --exclude '*.la' --exclude 'share/man' "$LOCAL_SRC/" "$INC/usr/local/"

echo "== Glaze session: binaries from $GLAZE_SRC/bin, data from the repo checkout $GLAZE_REPO"
mkdir -p "$INC/usr/local/bin" "$INC/usr/local/share/wayland-sessions" "$INC/usr/local/share/applications" "$INC/usr/local/share/glaze"
for b in glaze glaze-service glaze-session glaze-compositor glaze-shell glaze-settings; do
  install -m 0755 "$GLAZE_SRC/bin/$b" "$INC/usr/local/bin/$b"
done
cp -a "$GLAZE_REPO/themes" "$INC/usr/local/share/glaze/themes"
cp -a "$GLAZE_REPO/assets" "$INC/usr/local/share/glaze/assets"
cat > "$INC/usr/local/share/wayland-sessions/glaze.desktop" <<'EOF'
[Desktop Entry]
Type=Application
Name=Glaze
Comment=Glaze Desktop
Exec=/usr/local/bin/glaze-session run
TryExec=/usr/local/bin/glaze-session
DesktopNames=Glaze
EOF
sed 's#^Exec=.*#Exec=/usr/local/bin/glaze-settings#; s#^TryExec=.*#TryExec=/usr/local/bin/glaze-settings#' "$GLAZE_SRC/share/applications/glaze-settings.desktop" > "$INC/usr/local/share/applications/glaze-settings.desktop"
install -m 0644 "$GLAZE_REPO/packaging/sway/sway.conf" "$INC/etc/skel/.config/glaze/sway.conf"
# fastfetch/mime/theme env etc. already live in config/includes.chroot (tracked)

echo "== Plymouth theme"
cp -a "$HERE/../plymouth/kiln" "$INC/usr/share/plymouth/themes/"

echo "== staged size"; du -sh "$INC/usr/local" "$INC/usr/share/plymouth" "$INC/etc/skel"
