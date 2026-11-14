# Settings app: schema and API requests for agent A (and B)

`apps/glaze-settings` (agent D) works against the **current** `glaze-common`
config v3 and `glaze-design` APIs. Everything below is something the app
already shows and saves, but that no Glaze component reads yet. Until a
request is done, the value lives in the settings sidecar
`$XDG_CONFIG_HOME/glaze/settings.toml` (owned by glaze-settings, TOML, same
section/key names as requested below) and the row carries a "saved only"
badge. When a key lands in `glaze-common`, glaze-settings moves it to
`config.toml` (adapter code in `apps/glaze-settings/src/ui/pages/*`).

Status legend: **open** = not in the schema; nothing reads it.

## 0. Live-apply contract (A) — needed now (user: "wallpaper change needs a DE reload")

What glaze-settings guarantees (code: `apps/glaze-settings/src/store.rs`):

- Every change is written **atomically** to `$XDG_CONFIG_HOME/glaze/config.toml`
  (default `~/.config/glaze/config.toml`): the new text goes to
  `~/.config/glaze/.config.toml.glaze-settings.tmp` (same directory), then
  `rename(2)` over `config.toml`. With `gio::FileMonitorFlags::NONE` the shell's
  directory monitor sees `Created`/`MovedIn` for `config.toml`, never a
  half-written file. Keys not in the schema go to `settings.toml` in the same
  directory the same way (it also triggers the shell's reload today; harmless).
- One user action = one write (a batch such as Reset section is one write).
  Slider drags are debounced (written 280 ms after the last movement), so
  one drag is one write.
- Keys written are exactly the `glaze-common` v3 paths: wallpaper =
  `desktop.wallpaper_set` (+ `desktop.wallpaper` = explicit file or removed),
  scheme = `appearance.scheme`, family = `appearance.theme`, accent =
  `appearance.accent`, texture = `appearance.texture` /
  `appearance.texture_intensity`, radius/stroke/shadow =
  `appearance.radius` / `stroke_weight` / `shadow_strength`, taskbar =
  `panel.*`, start menu = `launcher.*`, motion = `motion.*`.
- Scriptable equivalent (same validation, backup and atomic write):
  `glaze-settings --set desktop.wallpaper_set kiln`,
  `glaze-settings --set appearance.scheme dark`, `glaze-settings --get KEY`.
- glaze-settings sends **no** extra notification: the file is the contract.
  There is no `config reload` request on the control socket for the shell
  (`Request::Reload` is answered by glaze-service for its own registry) and
  glaze-settings will not invent one. If A wants a push channel too, add a
  `glaze config reload` CLI that pokes the shell and glaze-settings will call it
  after each write.

What A must do (observed in the live VM 2026-11-12: `live reload applied in
20-90 ms` is logged on every write, CSS and taskbars update, but **the
desktop surface keeps the old wallpaper** until glaze-shell restarts;
`desktop_ui.rs` still says "restart glaze-shell to apply"):

1. In `Shell::reload`, also re-resolve the wallpaper
   (`desktop.wallpaper` > `desktop.wallpaper_set` + resolved scheme) and swap
   the picture on the existing background layer surface (no new surface, no
   flash); rebuild desktop icons only when `desktop.show_icons` changed.
2. The same reload path must apply `appearance.scheme`, `appearance.accent`,
   `appearance.texture*` to the desktop surface and menus (taskbar already
   follows).
3. Log `[glaze-shell] live reload: wallpaper -> <path>` so the change is
   verifiable from the log.

**Status (A, 2026-11-12): done.** `Shell::reload` now re-resolves the
wallpaper (`desktop.wallpaper` > `desktop.wallpaper_set` + the effective
scheme) and cross-fades it on the existing background surface (450 ms, 0
under reduced motion; `desktop_ui::set_wallpaper`). Log line:
`[glaze-shell] wallpaper (live): <path>`. Scheme, accent, texture, radius,
stroke, shadows, panel edge/size/sections and start-menu options all go
through the same reload (CSS re-emitted, taskbars rebuilt). Desktop icons are
not rebuilt on reload yet (`desktop.show_icons` still needs a restart).

## 1. Preserve unknown keys and comments when the shell saves (A) — open

`glaze-shell::save_pins` rewrites `config.toml` with
`GlazeConfig::save_to`, which drops comments, key order and unknown keys.
glaze-settings edits the file with `toml_edit` and preserves all three.
Request: save pins with an in-place edit (e.g. `toml_edit`, already in the
lock via `toml 0.8`) of `launcher.pinned` and `panel.default_pins` only.
glaze-settings re-reads the file before every write, so concurrent pin edits
are not lost on its side.

## 2. `appearance.scheme = "auto"` (A) — done as `appearance.scheme_mode`

**Landed (A, 2026-11-12):** `config.toml` key `[appearance] scheme_mode =
"manual" | "auto"` (`glaze_common::config::SchemeMode`, default `manual`).
`ColorScheme` itself did not gain `Auto` (that would break every exhaustive
match on it, including glaze-settings). With `auto`, the shell resolves light
07:00-19:00 local time, dark otherwise, re-checks every 60 s and switches
live (wallpaper variant included); `appearance.scheme` is then ignored.
Settings: please write `appearance.scheme_mode` into `config.toml` (it is in
the schema now) instead of the sidecar. Portal `color-scheme` is not read
yet.

Original request:

- Type: `ColorScheme` gains `Auto` (`"auto"`), default stays `Light`.
- Semantics: follow the system/portal preference
  (`org.freedesktop.appearance color-scheme`) when available, else local
  time (light 07:00-19:00). Re-evaluate at least every minute and on portal
  change; tokens resolve to the concrete scheme.
- Today: sidecar `[appearance] scheme_mode = "auto"`; glaze-settings writes
  the concrete scheme for the current time into `config.toml` when the
  option is chosen and whenever Settings starts.

## 3. Wallpaper fit, variant and per-monitor sets (A) — open

```toml
[desktop]
wallpaper_fit = "fill"          # fill | fit | center | tile | stretch (default fill)
wallpaper_variant = "follow"    # follow | light | dark (default follow = scheme)
[desktop.monitors."HDMI-A-1"]
wallpaper_set = "kiln"          # overrides desktop.wallpaper_set on that output
wallpaper = "/path/img.png"     # optional, wins like desktop.wallpaper
```
Sidecar today: `[wallpaper] fit`, `variant`, `[wallpaper.monitors] <connector> = "<set id>"`.

## 4. Window management defaults (A for Sway backend, B for glaze-compositor) — open

```toml
[windows]
default_layout = "stacking"      # stacking | tiling (global mode at session start)
gaps = 8                         # px, 0..=48, tiling gap
focus_follows_mouse = "no"       # no | yes | always
mouse_warping = false            # pointer follows keyboard focus
[windows.workspace_layouts]      # per-workspace override, 1-based keys
"1" = "tiling"
```
Live today: `set-window-layout` on the compositor socket when it exists;
`swaymsg gaps inner all set N`, `swaymsg focus_follows_mouse ...`,
`swaymsg mouse_warping ...` in a Sway session (not persisted across logins).
Request for B: read `[windows]` at startup (and on config reload) and add
`gaps`/`focus_follows_mouse` to the compositor policy.

## 5. Keybindings (B, and A for the Sway session) — open

```toml
[keybindings]                    # action id = chord; "" disables
close-window = "super+shift+q"
switch-workspace = "super"       # family: modifier prefix for 1..9
move-to-workspace = "super+shift"
```
Action ids (20): close-window, toggle-maximize, snap-left, snap-right,
snap-bottom, move-left, move-right, toggle-tiling, toggle-tiling-global,
minimize, cycle-windows, cycle-windows-back, launcher, next-workspace,
prev-workspace, switch-workspace, move-to-workspace, terminal, fullscreen,
exit-session. Chord syntax = `glaze_wm::keybindings::Binding::parse`.
Defaults and conflict rules: `apps/glaze-settings/src/keys.rs`
(`keys::effective`, `keys::conflicts`).
Requests: B builds `Keymap` from this table over `Keymap::default_glaze`
(needs `WmAction` for terminal/fullscreen/exit or a separate exec table);
A generates a Sway `bindsym` include (e.g. `~/.config/glaze/keybindings.sway`,
`include`d by the session sway.conf) for the fallback session.

## 6. Animation speed in the compositor (B) — open

B currently takes `--anim-speed` / `GLAZE_ANIM_SPEED`. glaze-settings writes
`motion.speed` (0.25..=4) and `motion.style` (smooth | snappy | off) in
`config.toml`; request: glaze-compositor reads those (and
`appearance.reduced_motion`) at startup and on reload.

## 7. Default terminal (A) — open

```toml
[default_apps]
terminal = "foot.desktop"        # desktop id; absent = first TerminalEmulator
```
Used by Super+Return and `panel.default_pins`. Other default apps use the
XDG `mimeapps.list` (already live, no request).

## 8. Display configuration persistence (A/B) — open

Display changes are applied through `swaymsg output ...` or `wlr-randr` and
last for the session. Request: a persisted `[displays."<connector>"]`
table (`mode = "1920x1080@60"`, `scale`, `transform`, `position = [x, y]`,
`enabled`) applied by the session at login (Sway include or compositor).

## 9. glaze-design: accent-on-panel contrast helper (A) — nice to have

glaze-settings warns when the accent is below 2:1 against `panel`
(`validate::accent_warnings`). A shared helper in `glaze_design::overrides`
(e.g. `accent_visibility(tokens) -> f64`) would keep the threshold in one
place.

## 10. From A to D: Sway draws a title bar over the Settings header bar — open (D)

Observed 2026-11-12 in the live Sway session: a freshly started
`org.glaze.Settings` window gets Sway's title bar ("Settings", with the shell's
min/max/close strip) above its own header bar, i.e. two title bars. Other GTK4
apps (Evince, Text Editor, Calculator) negotiate client-side decorations and
get none. `swaymsg '[app_id=org.glaze.Settings] border none'` hides it, so the
window is asking for (or not refusing) server-side decorations; likely
`set_decorated(false)` or a custom titlebar setup. Please keep GTK's CSD
negotiation (a normal decorated `GtkWindow` with `set_titlebar(header)`); the
shell will not special-case the app id.

## 10. Sensible default apps in provisioning (packaging owner) — open

With no `mimeapps.list` default, Settings > Default apps shows what GIO picks
from Debian's `mimeinfo.cache` (verified in the VM on 2026-11-12, as
"(system choice)"): folders open in **Disk Usage Analyzer**
(`org.gnome.baobab`), text in **LibreOffice Writer**, and PNG images in
**Firefox ESR**. Request: `packaging/kilnlinux/provision/` ships an
`/etc/xdg/mimeapps.list` (or `glaze-mimeapps.list`) with Thunar
(`inode/directory`), GNOME Text Editor (`text/plain`), Loupe (images), Evince
(PDF), Firefox (http/https/html) and Thunderbird (mailto). The user's
`~/.config/mimeapps.list` still wins, and Settings writes only there.
Settings already reads the full XDG order (`$XDG_CONFIG_HOME`,
`$XDG_CONFIG_DIRS`, then `$XDG_DATA_DIRS/applications`, with
`glaze-mimeapps.list` before `mimeapps.list` in each).

## 11. Focus rings in `glaze_design::css` (A) — note

GTK4 sets `:focus-visible` on every **ancestor** of the focused widget, so a
`*:focus-visible` or container-level outline draws rings around the window,
banners and button groups. glaze-settings now uses `*:focus:focus-visible`.
`glaze_design::css` scopes its rule to `button:focus-visible` and named
classes. That is fine today, but please keep it off containers.

## 12. Call `glaze-colour-scheme` when the shell switches the scheme (A) — open

With `appearance.scheme_mode = "auto"`, the shell switches Glaze at 07:00 and
19:00. Settings calls `glaze-colour-scheme light|dark` when it notices the
change, but only while Settings is open. Request: the shell's minute timer
(and session start) runs `glaze-colour-scheme <resolved>` when the resolved
scheme differs from `~/.config/glaze/colour-scheme`, so GTK, Qt and portal apps
follow with Settings closed. It is idempotent and takes about 100 ms.

## 13. Taskbar restore of scratchpad windows (A) — check

The Settings minimise button does `[app_id="org.glaze.Settings" pid=N] move
scratchpad`, the same thing the taskbar does for minimise. Please keep
treating scratchpad windows as minimised task buttons and restoring them with
`scratchpad show` (plus `floating enable` if needed), including windows that
minimised themselves this way.

## 14. Taskbar briefly disappears on every live reload (A) — observed

In live-session grim captures taken about 1 s after a scheme change made from
Settings, the taskbar was missing in one frame and back in the next
(`docs/screens/settings/colour-scheme/dark-thunar.png` has no taskbar;
`dark-calc.png`, taken 1.2 s later, has it). `Shell::reload` rebuilds the
panel windows (`build_panels`). Suggest restyling the existing panels in place
(new CSS plus a relayout) and rebuilding only when edge, monitors or sections
change, so a setting change never makes the bar flicker.
