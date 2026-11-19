#!/usr/bin/env bash
# F13 CI check: forbidden Debian marks + os-release correctness.
# Usage: check-branding.sh [--rootfs DIR] [--iso-tree DIR] [PACKAGING_DIR]
#   --rootfs DIR    built rootfs (chroot): checks os-release, issue, lsb-release, GRUB/Plymouth/fastfetch text
#   --iso-tree DIR  extracted ISO tree: checks boot menus + volume label file
#   PACKAGING_DIR   defaults to packaging/kilnlinux (scanned for marks, `ID=debian` checks, desktop-base)
# Allowed Debian text: "based on Debian" attribution only. Exit 1 on any violation.
set -uo pipefail
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PKG="$HERE/.."; ROOT=""; ISO=""
while [ $# -gt 0 ]; do case "$1" in --rootfs) ROOT="$2"; shift 2;; --iso-tree) ISO="$2"; shift 2;; *) PKG="$1"; shift;; esac; done
fail=0; bad() { echo "FAIL: $*" >&2; fail=1; }

# Lines containing Debian that are fine: attribution, ID_LIKE, upstream codename vars, URLs of deb.debian.org etc.
ALLOW='DEBIAN_FRONTEND|from Debian|Debian components|calamares-settings-debian|based on Debian|ID_LIKE=|DEBIAN_CODENAME|[a-z.]*debian\.org|Debian-security|debian-archive-keyring|debian-security|Debian [0-9]+|\(Debian\)|/debian|debootstrap|dpkg|[Dd]ebian-'
scan() {  # dir ; flag Debian mark lines
  grep -rIin --exclude-dir=.git --exclude-dir=tests --exclude='*.md' 'debian' "$1" 2>/dev/null \
    | grep -Eiv "$ALLOW" | grep -Eiv '^\S+:[0-9]+:\s*#' || true
}
hits=$(scan "$PKG"); [ -z "$hits" ] || { bad "Debian mark in packaging:"; echo "$hits" | head -20 >&2; }
grep -rIn --exclude-dir=tests -E 'Debian Kiln|Kiln Debian|desktop-base|debian-logo|openlogo' "$PKG" 2>/dev/null \
  | grep -Ev ':[0-9]+:\s*#' | while read -r l; do echo "FAIL: forbidden mark: $l" >&2; done | grep -q . && fail=1
grep -rIn --exclude-dir=tests -E 'ID *= *"?debian|ID *== *"?debian|\$ID" *= *"?debian' "$PKG" 2>/dev/null \
  | while read -r l; do echo "FAIL: ID=debian check: $l" >&2; done | grep -q . && fail=1
find "$PKG" -type f \( -iname '*swirl*' -o -iname 'debian*.png' -o -iname 'debian*.svg' \) | while read -r f; do echo "FAIL: Debian artwork $f" >&2; done | grep -q . && fail=1

osr() {  # file
  f="$1"; [ -r "$f" ] || { bad "missing $f"; return; }
  v() { sed -n "s/^$1=//p" "$f" | head -1 | tr -d '"'; }
  [ "$(v ID)" = kiln ] || bad "$f: ID must be kiln (got '$(v ID)')"
  [ "$(v ID_LIKE)" = debian ] || bad "$f: ID_LIKE must be debian"
  [ "$(v NAME)" = "Kiln GNU/Linux" ] || bad "$f: NAME must be 'Kiln GNU/Linux'"
  case "$(v PRETTY_NAME)" in "Kiln GNU/Linux"*) ;; *) bad "$f: PRETTY_NAME must start with Kiln GNU/Linux";; esac
  for k in VERSION_ID HOME_URL SUPPORT_URL BUG_REPORT_URL; do [ -n "$(v $k)" ] || bad "$f: $k unset"; done
  [ "$(v LOGO)" = kiln-logo ] || bad "$f: LOGO must be kiln-logo"
  [ -z "$(v CPE_NAME)" ] || bad "$f: CPE_NAME must be unset"
  [ -n "$(v VERSION_CODENAME)" ] || bad "$f: VERSION_CODENAME (trixie) must be kept for third-party repos"
  sed -n 's/^PRETTY_NAME=//p;s/^NAME=//p' "$f" | grep -qi debian && bad "$f: NAME/PRETTY_NAME contain Debian"
}

if [ -n "$ROOT" ]; then
  osr "$ROOT/usr/lib/os-release"; osr "$ROOT/etc/os-release"
  for f in etc/issue etc/issue.net etc/lsb-release etc/default/grub etc/default/grub.d usr/share/plymouth/themes usr/share/fastfetch etc/fastfetch boot/grub; do
    [ -e "$ROOT/$f" ] || continue
    h=$(scan "$ROOT/$f"); [ -z "$h" ] || { bad "Debian mark in rootfs/$f"; echo "$h" | head -5 >&2; }
  done
  [ -e "$ROOT/usr/share/desktop-base" ] && bad "desktop-base installed (Debian swirl theme)"
  [ -e "$ROOT/var/lib/dpkg/info/desktop-base.list" ] && bad "desktop-base package present"
  [ -e "$ROOT/var/lib/dpkg/info/popularity-contest.list" ] && bad "popularity-contest present"
  [ -e "$ROOT/etc/dpkg/origins/kiln" ] || bad "missing /etc/dpkg/origins/kiln"
  [ -e "$ROOT/source-offer.txt" ] || echo "WARN: /source-offer.txt missing (L9)" >&2
fi
if [ -n "$ISO" ]; then
  h=$(scan "$ISO/boot" ; scan "$ISO/isolinux"; scan "$ISO/.disk" 2>/dev/null); h=$(echo "$h" | sed '/^$/d')
  [ -z "$h" ] || { bad "Debian mark in ISO tree boot menus"; echo "$h" | head -10 >&2; }
fi
[ $fail = 0 ] && echo "branding check OK"
exit $fail
