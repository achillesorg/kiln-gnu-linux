# Kiln Files

Kiln Files is the file manager of Kiln GNU/Linux. It is a **derivative (fork) of Thunar**
from the Xfce project (https://gitlab.xfce.org/xfce/thunar), taken from tag `thunar-4.20.2`
(commit `11a519f8b40c2bd965fdc0b8e32d76d36b96ee7e`), the version packaged by Debian 13 (trixie).

Licence: GPL-2.0-or-later (see `COPYING`, `COPYING.LIB`). All upstream copyright headers are kept.
"Thunar" and "Xfce" are credited as the base project only; they are not part of this product name.

## Changes against upstream

- Product renamed Kiln Files: binary `kiln-files`, application id `org.kiln.Files`
  (D-Bus names `org.kiln.Files`, `org.kiln.FileManager`), desktop file `org.kiln.Files.desktop`
  (with `StartupWMClass=org.kiln.Files`), icon `org.kiln.Files`, config dir `~/.config/kiln-files`,
  window titles and About dialog ("Kiln Files", "Based on Thunar 4.20.2 by the Xfce project").
  Translations for strings that contain the old name fall back to English.
- `thunar/kiln-theme.c`: loads `kiln-light.css` / `kiln-dark.css` (installed to `$datadir/kiln-files`),
  follows `org.gnome.desktop.interface color-scheme` live (what `glaze-colour-scheme` sets),
  sets icon theme `Kiln` and font Inter. `KILN_SCHEME=light|dark` and `KILN_THEME_DIR` override for testing.
- `kiln-theme/*.css`: generated from `../theme/gtk3.css.in` by `../tools/gen-css.py` (do not edit by hand).
- Build with `./autogen.sh` (xfce4-dev-tools), then see `../build-all.sh` for the configure line
  (`--program-transform-name` renames the binary).
