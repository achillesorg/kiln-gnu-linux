# Kiln Monitor

Derivative (fork) of **GNOME System Monitor** (https://gitlab.gnome.org/GNOME/gnome-system-monitor), tag `48.1`
(commit `68dab9ea315aae5a6318153e05797657092d9d9e`), Debian 13 version. Licence: GPL-2.0-or-later (`COPYING`).
Renamed by `../tools/rebrand-gnome.py` (+ schema pre-pass to `org.kiln.monitor`) and `../tools/patch-monitor.py`:
binary `kiln-monitor`, app id `org.kiln.Monitor`, Kiln theme loader (`src/kiln-theme-adw.c`, follows the system scheme; upstream forced prefer-light), About text.
Build: `meson setup build --prefix=/usr/local -Dwerror=false`, run with `flock /tmp/heavy-build.lock ninja -j2`.
