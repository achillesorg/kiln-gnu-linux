#!/usr/bin/env bash
# Build the Kiln GNU/Linux live ISO (amd64 or arm64) in a throwaway work dir, copy the ISO out, delete the work dir.
# Usage:  ./build.sh [--slim] [--keep]      (normal user with sudo, or root, e.g. inside the Docker builder)
#   --slim  leave out the heavy apps (config/package-lists/kiln-extra.list.chroot): LibreOffice, GIMP, Inkscape, VLC, ...
#   --keep  keep the work dir (default: delete it, success or failure)
# Environment:
#   KILN_ARCH       amd64 | arm64            (default: host dpkg architecture)
#   KILN_ISO_WORK   work dir                 (default: $HOME/kiln-iso-work)
#   KILN_ISO_OUT    output dir               (default: $HOME/kiln-iso-out)
#   KILN_MIN_FREE_GB  free space check       (default: 8 slim / 16 full)
#   KILN_NO_LOCK=1  do not take /tmp/heavy-build.lock (the shared lock of the arm64 dev VM)
# Needs live-build (+ debootstrap, squashfs-tools, xorriso, mtools, dosfstools; amd64 also syslinux, isolinux, grub-pc-bin,
# grub-efi-amd64-bin). Run ./stage.sh first so the Glaze session and Kiln apps are baked in.
set -euo pipefail
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ARCH="${KILN_ARCH:-$(dpkg --print-architecture)}"
WORK="${KILN_ISO_WORK:-$HOME/kiln-iso-work}"
OUT="${KILN_ISO_OUT:-$HOME/kiln-iso-out}"
SLIM=0; KEEP=0
for a in "$@"; do case "$a" in --slim) SLIM=1;; --keep) KEEP=1;; *) echo "unknown arg $a" >&2; exit 2;; esac; done
SUDO=""; [ "$(id -u)" = 0 ] || SUDO=sudo

mkdir -p "$(dirname "$WORK")" "$OUT"
min_free_gb="${KILN_MIN_FREE_GB:-$([ "$SLIM" = 1 ] && echo 8 || echo 16)}"
free_gb=$(df -BG --output=avail "$(dirname "$WORK")" | tail -1 | tr -dc 0-9)
if [ "$free_gb" -lt "$min_free_gb" ]; then
  echo "only ${free_gb} GB free, need ${min_free_gb} GB for this variant (try --slim, or free space)" >&2; exit 1
fi
command -v lb >/dev/null || { echo "live-build missing: apt install live-build" >&2; exit 1; }

cleanup() { [ "$KEEP" = 1 ] || { (cd "$WORK" 2>/dev/null && $SUDO lb clean --purge >/dev/null 2>&1) || true; cd "$HOME"; $SUDO rm -rf "$WORK"; }; }
trap cleanup EXIT

$SUDO rm -rf "$WORK"; mkdir -p "$WORK"
rsync -a --exclude build.sh --exclude README.md "$HERE/" "$WORK/"
cd "$WORK"
[ "$SLIM" = 1 ] && rm -f config/package-lists/kiln-extra.list.chroot
KILN_ARCH="$ARCH" sh auto/config
LOCK=(); [ -n "${KILN_NO_LOCK:-}" ] || LOCK=(flock /tmp/heavy-build.lock)
"${LOCK[@]}" $SUDO nice -n 10 lb build 2>&1 | tee "$OUT/lb-build.log" | tail -n 40
iso="$(ls -1 ./*.iso | head -1)"
name="kiln-gnulinux-${ARCH}$([ "$SLIM" = 1 ] && echo -slim).iso"
mv "$iso" "$OUT/$name"
( cd "$OUT" && sha256sum "$name" > "$name.sha256" )
echo "ISO: $OUT/$name ($(du -h "$OUT/$name" | cut -f1))"
