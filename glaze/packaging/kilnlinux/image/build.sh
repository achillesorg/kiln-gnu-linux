#!/usr/bin/env bash
# Build the Kiln GNU/Linux arm64 live ISO in a throwaway work dir, copy the ISO out, delete the work dir.
# Usage (on the arm64 VM, as the normal user with sudo):  ./build.sh [--slim] [--keep]
#   --slim  leave out the heavy apps (config/package-lists/kiln-extra.list.chroot): LibreOffice, GIMP, Inkscape, VLC, ...
#   --keep  keep the work dir (default: delete it, success or failure)
# Needs: `sudo apt install live-build` (pulls debootstrap, squashfs-tools, xorriso, grub-efi-arm64-bin, mtools, dosfstools).
# Run ./stage.sh first so the Glaze session and Kiln apps are baked in. Takes the shared heavy-build lock.
set -euo pipefail
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
WORK="${KILN_ISO_WORK:-$HOME/kiln-iso-work}"
OUT="${KILN_ISO_OUT:-$HOME/kiln-iso-out}"
SLIM=0; KEEP=0
for a in "$@"; do case "$a" in --slim) SLIM=1;; --keep) KEEP=1;; *) echo "unknown arg $a" >&2; exit 2;; esac; done

min_free_gb=$([ "$SLIM" = 1 ] && echo 8 || echo 16)
free_gb=$(df -BG --output=avail "$HOME" | tail -1 | tr -dc 0-9)
if [ "$free_gb" -lt "$min_free_gb" ]; then
  echo "only ${free_gb} GB free, need ${min_free_gb} GB for this variant (try --slim, or free space)" >&2; exit 1
fi
command -v lb >/dev/null || { echo "live-build missing: sudo apt install live-build" >&2; exit 1; }

cleanup() { [ "$KEEP" = 1 ] || { sudo lb clean --purge >/dev/null 2>&1 || true; cd "$HOME"; sudo rm -rf "$WORK"; }; }
trap cleanup EXIT

sudo rm -rf "$WORK"; mkdir -p "$WORK" "$OUT"
rsync -a --exclude build.sh --exclude README.md "$HERE/" "$WORK/"
cd "$WORK"
[ "$SLIM" = 1 ] && rm -f config/package-lists/kiln-extra.list.chroot
sh auto/config
flock /tmp/heavy-build.lock sudo nice -n 10 lb build 2>&1 | tee "$OUT/lb-build.log" | tail -n 40
iso="$(ls -1 ./*.iso | head -1)"
name="kiln-gnulinux-arm64$([ "$SLIM" = 1 ] && echo -slim).iso"
mv "$iso" "$OUT/$name"
( cd "$OUT" && sha256sum "$name" > "$name.sha256" )
echo "ISO: $OUT/$name ($(du -h "$OUT/$name" | cut -f1))"
