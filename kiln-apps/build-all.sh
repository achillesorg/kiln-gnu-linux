#!/usr/bin/env bash
# build-all.sh: build and install all Kiln apps into /usr/local (idempotent). Run as a user with sudo, from a
# checkout of this repo. Builds in $BUILD (default ~/kiln-apps-build/out) and keeps only the installed result.
# Usage: ./build-all.sh [app ...]   (apps: icons files terminal text calculator monitor viewer store; default all)
set -euo pipefail
HERE="$(cd "$(dirname "$0")" && pwd)"
BUILD="${BUILD:-$HOME/kiln-apps-build/out}"
PREFIX=/usr/local
JOBS="${JOBS:-$(nproc)}"
SUDO=""; [ "$(id -u)" = 0 ] || SUDO=sudo
mkdir -p "$BUILD"

APT_COMMON="build-essential meson ninja-build cmake pkg-config git python3 xfce4-dev-tools desktop-file-utils libglib2.0-dev-bin gettext"
# Dev packages (also pulled in by `apt-get build-dep <debian package>`; both are listed so a plain image works):
APT_FILES="libgtk-3-dev libxfce4util-dev libxfce4ui-2-dev libexo-2-dev libxfconf-0-dev libgudev-1.0-dev libnotify-dev libpango1.0-dev libgdk-pixbuf-2.0-dev libgirepository1.0-dev libxfce4panel-2.0-dev"
APT_TERMINAL="libwayland-dev wayland-protocols libxkbcommon-dev libfontconfig-dev libpixman-1-dev libfcft-dev libtllist-dev libutf8proc-dev scdoc foot-terminfo libutempter-dev"
APT_ADW="libadwaita-1-dev libgtk-4-dev libgtksourceview-5-dev libgtop2-dev libgtkmm-4.0-dev libadwaita-1-dev libsystemd-dev valac libmpc-dev libmpfr-dev libxml2-dev libgee-0.8-dev libeditorconfig-dev libenchant-2-dev libspelling-dev libicu-dev libpcre2-dev gobject-introspection libgirepository1.0-dev itstool appstream"
APT_STORE="libgtk-4-dev libadwaita-1-dev libpackagekit-glib2-dev libflatpak-dev"

step() { printf '\n=== %s ===\n' "$*"; }
deps() { step "apt build dependencies"; $SUDO apt-get install -y --no-install-recommends $APT_COMMON $APT_FILES $APT_TERMINAL $APT_ADW $APT_STORE || true
         $SUDO apt-get build-dep -y thunar foot gnome-text-editor gnome-calculator gnome-system-monitor loupe || true; }

# Fetch a pinned upstream tag only when we need to rebuild from pristine sources (the repo already contains the forks).
build_meson() { # dir extra-args...
  local dir="$1"; shift
  ( cd "$HERE/$dir" && rm -rf "$BUILD/$dir" && meson setup "$BUILD/$dir" --prefix=$PREFIX --sysconfdir=/etc --buildtype=release -Dwerror=false "$@" \
    && ninja -C "$BUILD/$dir" -j"$JOBS" && $SUDO ninja -C "$BUILD/$dir" install ) && rm -rf "${BUILD:?}/$dir"
}

app_icons() { step "Kiln icon theme and app icons"
  python3 "$HERE/tools/gen-icons.py"; python3 "$HERE/tools/gen-icon-theme.py"
  $SUDO rm -rf $PREFIX/share/icons/Kiln; $SUDO mkdir -p $PREFIX/share/icons
  $SUDO cp -r "$HERE/icon-theme/Kiln" $PREFIX/share/icons/; $SUDO gtk-update-icon-cache -qf $PREFIX/share/icons/Kiln || true; }

app_files() { step "Kiln Files (Thunar 4.20.2 fork)"
  rm -rf "$BUILD/kiln-files"; cp -r "$HERE/kiln-files" "$BUILD/kiln-files"
  ( cd "$BUILD/kiln-files" && xdt-autogen >/dev/null \
    && ./configure --enable-maintainer-mode --prefix=$PREFIX --sysconfdir=/etc --libexecdir=$PREFIX/lib \
         --program-transform-name='s/^thunar$/kiln-files/' --disable-gtk-doc --disable-debug \
    && make -j"$JOBS" && $SUDO make install )
  $SUDO rm -f $PREFIX/bin/Thunar
  # re-use Debian's archive plugin (same ABI) so "Extract here" keeps working
  $SUDO mkdir -p $PREFIX/lib/thunarx-3
  for p in /usr/lib/*/thunarx-3/thunar-archive-plugin.so; do [ -e "$p" ] && $SUDO ln -sf "$p" $PREFIX/lib/thunarx-3/; done
  $SUDO ldconfig; rm -rf "$BUILD/kiln-files"; }

app_terminal() { step "Kiln Terminal (foot 1.21.0 fork)"
  build_meson kiln-terminal -Ddocs=disabled -Dthemes=false -Dtests=false -Dterminfo=disabled -Ddefault-terminfo=foot; }
app_text()       { step "Kiln Text (GNOME Text Editor 48.3 fork)"; build_meson kiln-text; $SUDO glib-compile-schemas $PREFIX/share/glib-2.0/schemas; }
app_calculator() { step "Kiln Calculator (GNOME Calculator 48.1 fork)"; build_meson kiln-calculator -Ddoc=false; $SUDO glib-compile-schemas $PREFIX/share/glib-2.0/schemas; $SUDO ldconfig; }
app_monitor()    { step "Kiln Monitor (GNOME System Monitor 48.1 fork)"; build_meson kiln-monitor; $SUDO glib-compile-schemas $PREFIX/share/glib-2.0/schemas; }
app_viewer()     { step "Kiln Viewer (Loupe 48.1 fork, Rust: needs cargo >= 1.80 and crates.io access)"
  # The repo ships the patch scripts, not the Loupe tree: fetch the pinned upstream tag and apply them.
  local src="$BUILD/loupe"; rm -rf "$src" "$BUILD/b-viewer"
  git clone -q --depth 1 --branch 48.1 https://gitlab.gnome.org/GNOME/loupe.git "$src"
  ( cd "$src" && python3 "$HERE/tools/rebrand-gnome.py" . org.gnome.Loupe org.kiln.Viewer ZZnoZZ ZZ "Image Viewer" "Kiln Viewer" >/dev/null \
    && python3 "$HERE/tools/patch-viewer.py" . \
    && python3 "$HERE/tools/gen-css.py" "$HERE/theme/adw.css.in" data/kiln-light.css data/kiln-dark.css \
    && cp "$HERE/icons/org.kiln.Viewer.svg" data/icons/hicolor/scalable/apps/org.kiln.Viewer.svg \
    && PATH="$HOME/.cargo/bin:$PATH" meson setup "$BUILD/b-viewer" --prefix=$PREFIX --buildtype=release \
    && PATH="$HOME/.cargo/bin:$PATH" ninja -C "$BUILD/b-viewer" -j"$JOBS" && $SUDO env PATH="$HOME/.cargo/bin:$PATH" ninja -C "$BUILD/b-viewer" install )
  $SUDO glib-compile-schemas $PREFIX/share/glib-2.0/schemas; rm -rf "$src" "$BUILD/b-viewer"; }
app_store()      { step "Kiln Store (new GTK4 app)"; [ -d "$HERE/kiln-store" ] && { build_meson kiln-store; $SUDO glib-compile-schemas $PREFIX/share/glib-2.0/schemas; } || echo "kiln-store not present"; }

finish() { $SUDO update-desktop-database $PREFIX/share/applications 2>/dev/null || true
           $SUDO gtk-update-icon-cache -qf $PREFIX/share/icons/hicolor 2>/dev/null || true; }

apps=("$@"); [ ${#apps[@]} -gt 0 ] || apps=(icons files terminal text calculator monitor viewer store)
[ "${KILN_SKIP_DEPS:-0}" = 1 ] || deps   # KILN_SKIP_DEPS=1: build deps already installed (Docker builder)
for a in "${apps[@]}"; do "app_$a"; done
finish
echo "done: $(ls $PREFIX/bin | grep '^kiln-' | tr '\n' ' ')"
