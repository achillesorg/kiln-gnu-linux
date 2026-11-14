# Kiln Terminal

Kiln Terminal is the terminal emulator of Kiln GNU/Linux. It is a **derivative (fork) of foot**
(https://codeberg.org/dnkl/foot) by Daniel Eklöf, taken from tag `1.21.0`
(commit `68f5eab0b0fa08becebbed412947ba19246c2518`), the version packaged by Debian 13 (trixie).

Licence: MIT (see `LICENSE`); all upstream copyright notices are kept. The Kiln additions are MIT too.

## Changes against upstream

- Renamed: binaries `kiln-terminal` and `kiln-terminal-client`, Wayland app id `org.kiln.Terminal`
  (`org.kiln.Terminal.client` for client windows), desktop file `org.kiln.Terminal.desktop`
  (`StartupWMClass=org.kiln.Terminal`), icon `org.kiln.Terminal`, default title "Kiln Terminal",
  config `~/.config/kiln-terminal/kiln-terminal.ini` (system default `/etc/xdg/kiln-terminal/`).
  The terminfo entry is still `foot` (install Debian's `foot-terminfo`).
- `kiln-colours.c`: built-in Kiln light and dark colour schemes (design tokens). Chosen at start from
  `~/.config/glaze/colour-scheme` (written by `glaze-colour-scheme`; `KILN_SCHEME=light|dark` overrides) and
  switched **live** on `SIGUSR1` (dark) / `SIGUSR2` (light) in `main.c` / `terminal.c`
  (`term_apply_conf_colors`). The helper `kiln-terminal-scheme dark|light` sends the signal to all windows.
  A user `[colors]` section turns the built-in schemes off.
- Defaults (`kiln-terminal.ini`): Noto Sans Mono 10, padding 10x8, 10000 lines scrollback, URL launching via
  `xdg-open`, beam cursor, server-side decorations preferred.
- Upstream `completions`, `icons`, `utils`, systemd units, themes and man pages are not installed
  (docs disabled in the build recipe); build with `-Dwerror=false` (newer xdg-shell enum values).
- Build: see `../build-all.sh`.
