# Glaze Settings (`apps/glaze-settings`)

The settings application for Glaze on Kiln GNU/Linux. It is native GTK4 with no
libadwaita. Its structure follows the Windows 2000 Control Panel: categories in a
sidebar and one page per area. It uses the flat cut-paper look: paper cards,
1 px warm hairlines, small radii, terracotta accent, serif headings and Inter
for the interface. The CSS comes from the same `glaze-design` tokens as the shell,
so the window is its own live preview.

Owner: agent D. Schema requests for the shell and compositor:
[`docs/settings-needs.md`](settings-needs.md). Defaults chosen:
`docs/decisions.md`, section "Agent D defaults".

## Run

| How | What happens |
|---|---|
| Start menu, then **Glaze Settings** | `~/.local/share/applications/glaze-settings.desktop` runs `~/.local/bin/glaze-settings` |
| `glaze-settings [--page PAGE] [--search TEXT] [--focus SETTING-ID]` | Opens the window. If it is already open, it is raised and focused (single instance, `org.glaze.Settings`) |
| `glaze-settings --help` | Lists every option and command |

The window uses GTK's cairo renderer unless `GSK_RENDERER` is set, because it
maps faster on software rendering. When startup fails (for example, no
Wayland display), the app prints
`glaze-settings: cannot start the window: ...` and exits with status 1.

## Pages

| Category | Page | Controls |
|---|---|---|
| Personalise | Appearance | Clay or Classic; Light, Dark or Automatic; theme file; accent (swatches, colour picker, hex, with a contrast warning); corner radius; stroke weight; shadow strength; paper texture set and intensity (including none); reduced effects; icon style; fonts; interface scale |
| | Wallpaper | Grid of sets from `assets/wallpapers/manifest.toml` (paper-hills default, terracotta-waves, hills-hard-edge, kiln, draft, margin, night-paper, classic-teal) with thumbnails; custom image; light/dark variant; fit; per-monitor set; desktop icons |
| | Animation | Live motion preview; style (smooth, snappy, off); speed; reduce motion; stop-motion mode (disabled, "coming later"); frame-time logging |
| Desktop | Taskbar | Edge, density, height, auto-hide, button style, label mode, grouping, suggested pins, section order, tray items, clock (24 h, seconds, date and its format), float/tile toggle visibility and scope, per-monitor overrides |
| | Start menu | Cascading or compact grid, brand strip, icon size, anchor, width and height, pinned-apps editor, grid columns, recent apps, search result limit, power row |
| | Windows | Default stacking or tiling, per-workspace override, gaps, focus follows mouse, pointer follows focus, keybinding editor with conflict detection (Glaze action ids) |
| System | Displays | Per output: resolution, scale, rotation and position through Sway IPC (`swaymsg output`), or `wlr-randr` as a fallback. Apply, then keep or revert within 15 s |
| | Sound | Volume and mute through `wpctl` (or `pactl` as a fallback). Without a sound server it shows a "No sound server found" state |
| | Network | NetworkManager devices and addresses (`nmcli`); Wi-Fi radio switch |
| | Power | Batteries from `/sys/class/power_supply` (or a "No battery" state); reduced effects |
| | Default apps | Browser, file manager, text editor, image viewer, document viewer and mail, written to `~/.config/mimeapps.list` (XDG). Without an explicit default, the app GIO would pick is shown as "(system choice)". Glaze terminal choice |
| | About | Kiln GNU/Linux lockup (brand kit); OS, kernel, CPU, memory, host, display server, GTK; Glaze and settings versions; licences; config file paths; reset all |

Header: **Undo** (also Ctrl+Z), **Reset section...**, **Import theme...**,
**Export theme...**. Sidebar: search (Ctrl+F) matches titles, groups and
keywords on every page. Enter jumps to the first match, which is scrolled into
view, flashed and focused.

Rows marked **saved only** store a value that no Glaze component reads yet
(see `settings-needs.md`). Rows marked **applies now** call a system tool
directly.

## Window controls (Sway)

Sway ignores GTK's minimise and maximise requests for floating windows, so
Settings has its own buttons:

- **Minimise** moves the window to the scratchpad. The taskbar shows it as
  minimised and a click restores it. Launching Settings again also restores it.
- **Maximise** fills the workspace's usable area (the taskbar is excluded) and
  **Restore** puts back the previous geometry. Double-clicking the title bar
  toggles it.
- **Close** quits.

On other compositors the buttons send the normal GTK requests.

## Dark mode for other apps

Appearance > Colour scheme (Light, Dark, Automatic) writes `config.toml`, so
Glaze itself switches live. Settings then runs `glaze-colour-scheme light|dark`
(`packaging/kilnlinux/bin/`) whenever the effective scheme changes, including
on undo, reset, external edits and the 07:00/19:00 switch while Settings is
open. Results checked in the VM (`docs/screens/settings/colour-scheme/`):

| App | What happens |
|---|---|
| GTK 4 / libadwaita (Calculator) | Switches live |
| Glaze shell, wallpaper, Sway frames | Switch live |
| GTK 3 (Thunar), already open | Keeps the old scheme until it is restarted |
| Qt (VLC) | Follows `~/.config/glaze/colour-scheme` from the next login (`/etc/profile.d/kiln-theme.sh`) |

About > This computer shows the "Other apps" and "Qt apps" status.

## Saving and safety

- Every change is saved immediately to `~/.config/glaze/config.toml` with
  `toml_edit`, which keeps comments, key order and unknown keys. Writes are
  atomic (temp file plus rename). Keys that are not in the schema yet go to
  `~/.config/glaze/settings.toml`.
- **Live apply:** glaze-shell watches `~/.config/glaze/` and reloads CSS,
  taskbars and the wallpaper. Settings sends no extra signal. Settings also
  watches the directory and follows changes made elsewhere (shell pin edits,
  `--set`, a text editor).
- Values are validated before they are written: an invalid value shows an
  inline error on the row and the file is not touched. A broken
  `config.toml` puts the app in read-only mode with a banner that offers a
  reset (the broken file is backed up first).
- Undo restores the previous file text. A backup of both files is made once
  per session and before every destructive action (reset section, reset all,
  import), in `~/.local/state/glaze/settings-backups/` (newest 10 kept).
  Destructive actions ask for confirmation first.
- The app never needs root.

## Command line (same validation, backup and atomic write as the window)

```
glaze-settings --get desktop.wallpaper_set
glaze-settings --set desktop.wallpaper_set kiln      # VALUE is TOML or a bare string
glaze-settings --set appearance.scheme dark
glaze-settings --unset appearance.accent
glaze-settings --set settings:wallpaper.fit fit      # settings: = sidecar
glaze-settings --check                               # validate config.toml
glaze-settings --export-theme my.toml --name "Mine"
glaze-settings --import-theme my.toml
glaze-settings --reset-section taskbar --yes
glaze-settings --find clock | --list
```

## Build, test, verify

```
cd apps/glaze-settings
cargo test                    # portable core, any host (no GTK needed)
cargo fmt --check
cargo build --release         # Linux only (GTK 4.14+); in the VM: ~/glaze-settings
cargo clippy --release --all-targets -- -D warnings      # in the VM
scripts/vm-smoke.sh [BIN]     # headless Sway: window must map and stay alive
scripts/vm-shots.sh OUT light dark classic               # every page, per variant
KEEP=1 scripts/vm-smoke.sh; scripts/vm-input.sh click X Y type TEXT shot F cfg
```

`vm-input.sh` needs `tools/vptr` (`cd tools/vptr && cargo build --release`):
a headless Sway has no input devices, so pointer input comes from a
wlr-virtual-pointer helper. Keys go through `wtype`.

Screenshots: `docs/screens/settings/`.
