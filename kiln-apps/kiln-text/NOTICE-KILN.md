# Kiln Text

Kiln Text is the text editor of Kiln GNU/Linux. It is a **derivative (fork) of GNOME Text Editor**
(https://gitlab.gnome.org/GNOME/gnome-text-editor), tag `48.3`
(commit `6f40f0bf2d443471ecc0267e8e87d202b6d1ff6c`), the version packaged by Debian 13 (trixie).

Licence: GPL-3.0-or-later (see `COPYING`); upstream copyright headers are kept. "GNOME" is credited as the base project only.

## Changes against upstream

- Renamed: binary `kiln-text`, app id `org.kiln.Text` (GSettings schema and path, resource path, D-Bus service,
  desktop file with `StartupWMClass=org.kiln.Text`, icon). Product name "Kiln Text"; About dialog says
  "Kiln GNU/Linux", "Based on GNOME Text Editor 48.3". Applied by `../tools/rebrand-gnome.py` then `../tools/patch-text.py`.
- `src/kiln-theme-adw.c` (shared with the other libadwaita Kiln apps): loads `kiln-light.css`/`kiln-dark.css`
  (generated from `../theme/adw.css.in`, overriding libadwaita named colours) following AdwStyleManager dark.
- GtkSourceView schemes `kiln` and `kiln-dark` (`data/styles/`, from `../tools/gen-sourceview.py`), default scheme `kiln`.
- The upstream help pages and the old metainfo are not shipped (metainfo replaced by a minimal one).
- Build: `meson setup build --prefix=/usr/local -Dwerror=false`; see `../build-all.sh`.
