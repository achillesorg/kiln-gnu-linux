# Kiln Calculator

Kiln Calculator is the calculator of Kiln GNU/Linux. It is a **derivative (fork) of GNOME Calculator**
(https://gitlab.gnome.org/GNOME/gnome-calculator), tag `48.1`
(commit `085ea4b91f12954ef655b92a89ef03c46b9c90d8`), the version packaged by Debian 13 (trixie).

Licence: GPL-3.0-or-later (see `COPYING`); upstream copyright headers are kept. "GNOME" is credited as the base project only.

## Changes against upstream

- Renamed (by `../tools/rebrand-gnome.py`, then `../tools/patch-calculator.py`): binary `kiln-calculator`,
  app id `org.kiln.Calculator`, GSettings schema `org.kiln.calculator`, resources, desktop file
  (`StartupWMClass=org.kiln.Calculator`), icon, title and About dialog ("Kiln Calculator", "Based on GNOME Calculator 48.1").
- `src/kiln-theme-adw.c` + `src/kiln-{light,dark}.css` (from `../theme/adw.css.in`): Kiln theme following AdwStyleManager dark.
- The search provider, help, docs and tests are not built. Internal shared libraries (libgcalc, libgci) keep their
  upstream names and install to `/usr/local/lib` (same version as Debian's, so compatible).
- Build: `meson setup build --prefix=/usr/local -Dwerror=false -Ddoc=false`; see `../build-all.sh`.
