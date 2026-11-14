# Glaze decisions (from the user, 2026-11-11)

Authoritative answers to the questions in `docs/overhaul-plan.md` section 9.
Agents: follow these; ask the user before changing them.

| # | Decision |
|---|---|
| Q1 Compositor | Own Smithay compositor (`glaze-compositor`), Sway kept as fallback. |
| Q2 Toolkit | GTK4 + gtk4-layer-shell, with a `glaze-paper` widget kit (plan default, not contested). |
| Q3 Order of work | Shell redesign on Sway (M0-M2) in parallel with compositor bring-up (M3-M4) (plan default). |
| Q4 Ink style | Ink accents: ink-outline icons and borders; hand-drawn wobble only in wallpapers, illustrations and large icons; geometric small UI. |
| Q5 Window decorations | Force server-side Glaze paper frames where apps accept xdg-decoration (plan default). |
| Q6 Form factor | Desktop first; touch/mobile at M6. |
| Q7 Glaze Classic | Keep as a full alternate token set on the same layout (plan default). |
| Q8 OS name | Full name **Kiln GNU/Linux** (Debian 13 based). Short names: **Kiln** and **KilnLinux**. DE name stays **Glaze**. Use "Kiln GNU/Linux" in formal text (os-release, docs headings, boot/login banner); Kiln or KilnLinux elsewhere. |
| Motion | The stop-motion look lives in the ART (wallpapers, illustrations, shape language). UI motion is **smooth everywhere else** by default. A separate optional **stop-motion mode** with selectable intensities (e.g. off / light / medium / strong; stepped ~8-12 fps with settling jitter) is added **later**, after the core milestones. Reduced-motion always wins. |

## Goal and priorities (user, 2026-11-11)

- End goal: a **daily-drivable desktop environment for the user**.
- **Tiling toggle** is required: stacking Win2K-style windows by default, switchable to tiling (per workspace or global; design in the plan).
- **The taskbar is the user's biggest focus.** It must be excellent first: live windows, pinned apps, grouping, previews, tray, clock, notifications, multi-monitor, keyboard and mouse flows, reliable. Prioritise it in M0-M2 over other shell parts.
- Desktop is finished first; **mobile comes after** the desktop is done (M6, later).

## Taskbar look and requirements (user feedback, 2026-11-11)

- Approved direction: `$HOME/fedeos/mockups/taskbar-v2.svg` (v1 was rejected as too cartoony). Rules: professional, compact. Bar ~40px, buttons ~28px, hairline 1-1.25px strokes in warm grey-brown (not heavy black ink), NO hard black offset shadows (tone-on-tone or very faint 2-3px shadows only), terracotta accent underline for the active window, small tinted pill for window-group count, dot badge on the bell, serif clock, float/tile segmented toggle.
- **Heavy customisability is a core requirement** (not a later nicety): taskbar position (any edge), size/density, button style, grouping and label modes, pinned/running layout, ordering of every section, which tray items and widgets show, clock format, float/tile toggle visibility, colours/accent, corner radius, stroke weight, shadow style/strength, icon style, per-monitor settings, auto-hide, transparency. Every visual token comes from the design system and is user-overridable via config and the settings app, with live preview; shareable theme files.
- **Paper textures are a core requirement:** subtle paper grain/fibre textures on surfaces (taskbar, windows, menus, wallpapers), selectable texture sets and intensity (including none), pre-rendered tileable assets so it stays cheap on software rendering, no blur. Must respect contrast gates and reduced-effects setting.

## Animation priority (user, 2026-11-11)

- **Smooth, flowing animations are a top priority**, not a late polish item: window open/close/minimise/restore, taskbar button reorder/group/hover, menus, previews, workspace and tiling-toggle transitions. Re-plan: bring a basic animation engine (spring/eased, interruptible, retargetable, vsync-paced, damage-tracked) forward from M5 to alongside M1-M3, and design the taskbar and compositor around it from the start.
- Constraint: the dev VM has no GPU acceleration (software rendering), so smoothness must be measured honestly (frame-time logs) and the flat/no-blur style helps. Real hardware later. Reduced-motion setting always wins; animation speed and style are user-customisable.
- Stop-motion mode remains a separate optional mode later (see Motion above); default motion is smooth.

## Preinstalled apps and repositories (user, 2026-11-12)

- Kiln ships a rich set of preinstalled apps and the repos to keep them updated: Debian main + contrib + non-free + non-free-firmware, **Mozilla's official apt repo for regular Firefox (not ESR)**, and Flatpak with the Flathub remote. Source of truth: `packaging/kilnlinux/provision/packages.list` + `provision-apps.sh` (idempotent; verifies the Mozilla key fingerprint).
- Set: Firefox, Thunderbird, LibreOffice, Evince, VLC, mpv, GIMP, Inkscape, Loupe, Thunar + archive/volume plugins, File Roller, GNOME text editor/calculator/system monitor/disk utility, NetworkManager + applet, Blueman, PipeWire + pavucontrol, portals, polkit agent, grim/slurp/wl-clipboard/cliphist, fonts, printing, Transmission, KeePassXC, fastfetch and basic dev tools.
- Later: a graphical software store (GNOME Software or similar) and default-app wiring in Glaze Settings; the start menu categories must show all of these with correct icons.

## Settings app (user, 2026-11-11)

- A full **settings app** is required (`apps/glaze-settings` exists as an early scaffold; never run on screen). It is the front end for the customisation schema: taskbar (edge, size, labels, grouping, section order, tray/widgets, clock), theme (Clay/Classic, light/dark, accent, radius, stroke, shadows, paper texture set + intensity), wallpapers (picker, per-monitor), animation (speed, style, reduced motion, later the stop-motion mode), tiling/stacking defaults and keybindings, displays, sound/network/power basics, default apps, about (Kiln GNU/Linux). Live preview, import/export of theme files, reset-to-default per section, search box.
- Planned as the phase right after the taskbar/design-system work (it depends on that config schema and the `glaze-paper` widget kit). Built in the same flat cut-paper style. Mobile layout comes with the mobile phase.

## Logo and boot (user, 2026-11-11)

- **Logo approved: concept C, nested paper arches** (terracotta outer arch, ink/cream inner arch) with a lowercase "kiln" wordmark built as paths. Source: `$HOME/fedeos/brand/kiln/` (generator `$HOME/fedeos/brand/gen_kiln_logo.py`). Full name Kiln GNU/Linux; wordmark says "kiln". Not yet done: audit and small-size tests, favicon/web icon set, one-colour usage guide, a "Kiln GNU/Linux" full-name lockup.
- **TODO (later): animated logo and animated boot splash.** The arches should animate in (paper layers sliding/settling into place, ember/progress feel), loop gently while booting, then hand off smoothly to the login/desktop. Needs: Plymouth theme (frame sequence or script theme) for boot, plus an SVG/Lottie-style animation source; must be consistent with the smooth-motion rules above and honour reduced-motion. Light and dark variants. Static boot splashes exist now in `brand/kiln/kiln-boot-*.svg`.

## Style (reference: user-supplied flat cut-paper illustration)

Flat terracotta (~#D97757) and cream paper sheets stacked with small hard offsets;
thick hand-drawn ink line art; circles, squares, arches, blobs and other unique
polished shapes; no blur, no gradients, no glossy shadows. Original artwork and
open-licence fonts only. Nothing from Anthropic (illustrations, logo, fonts).
The earlier paper-hills / terracotta-waves wallpapers were liked: keep the palette
and layering, replace blurred shadows with hard offsets, add grain and new shapes.

## Rules in force

- Never commit or push; the user decides. Local work only.
- Progress is measured by VM screenshots plus scripted runtime checks, not test counts.
- Anything outside the repo (e.g. `$HOME/fedeos/run.sh`) needs the user's approval, except where listed under "Approved".

## Approved by the user for M0

- Start M0 (plan section 5.2).
- Add a QMP socket to `$HOME/fedeos/run.sh` for screenshots.
- Rewrite `AGENTS.md`, `GLAZE.md`, `GOAL.md` for the Debian/KilnLinux target (replacing Fedora and Windows-host assumptions).

## Assets and branding defaults (agent C, 2026-11-12; user may override)

- The original `paper-hills` and `terracotta-waves` wallpapers ship **unchanged** (backup + SHA-256 in `$HOME/fedeos/brand/wallpapers-original/`, reproducible with `scripts/gen-wallpapers-legacy.py`); `paper-hills` stays the default. New v2 sets sit alongside: `hills-hard-edge`, `kiln`, `draft`, `margin`, `night-paper` (dark only), `classic-teal`.
- v2 manifest entries point at grain-baked PNGs (1920x1080 / 1080x2340); SVG sources and 1280x800 / 2560x1440 renders are recorded in extra manifest keys the resolver ignores. Schema unchanged.
- Wallpaper ink accents: 1.25-2.25 px at 1080p, low-frequency seeded wobble, round caps; shadows are flat offsets (ink 13 % light / black 30 % dark), never blurred.
- Ink icons: 41 glyphs, 1.5 px default with a 1.25 px variant and a duo (16 % fill) variant; `currentColor` only.
- Kiln brand: added a pixel-snapped 16 px symbol cut, padded the lockup, snapped the k to 45 degrees, added full-name and stacked "Kiln GNU/Linux" lockups and a two-tone favicon/app-icon set (ink tile, terracotta + cream arches).
- Boot: Plymouth script themes `kiln` (dark, default) and `kiln-light`, two-step frame fallbacks `kiln-frames(-light)`; reduced motion via a flag in the script. Not installed, not tested in the VM.
- Fonts: none bundled; OFL candidates listed in `docs/assets.md`.

## Compositor defaults (agent B, 2026-11-12; user may override)

- **Rendering:** nested backend = Smithay winit + GLES on llvmpipe. Smithay's winit backend only supports GLES, so the pixman fallback is deferred to the DRM backend (where `PixmanRenderer` can drive dumb buffers if EGL on `kms_swrast` misbehaves). Nested frames are paced by a 60 Hz timer (winit runs without vsync) and only rendered when something is dirty or animating.
- **Tiling toggle semantics:** one global mode (default stacking) plus per-workspace overrides. `Super+T` toggles the active workspace; `Super+Shift+T` toggles the global mode **and clears all overrides**. Tiling = dwindle tree (longer side split first, 50 %, 8 px gap) inside the work area; master/stack stays available in `glaze-wm`. Windows keep their stacking geometry while tiled (toggle back is lossless). Dragging a tiled window floats it; dialogs (toplevels with a parent) always float.
- **Control socket:** `glaze-compositor` serves its own socket (`$GLAZE_COMPOSITOR_SOCKET`, default `$XDG_RUNTIME_DIR/glaze/compositor.sock`) with the `glaze_common::ipc` framing; the new request is `set-window-layout {mode, workspace}`. `glaze-service` answers it with `backend-unavailable` (one-arm addition so the workspace compiles).
- **Workspaces:** 4 to start. New keys: `Super+Shift+T` (global tiling), `Super+H` (minimise), `Alt+Tab`/`Alt+Shift+Tab` (MRU cycle, commit on Alt release), `Super+Shift+1..9` (move window to workspace). `Ctrl+Alt+Backspace` exits the compositor.
- **Frames:** forced server-side for every client that speaks xdg-decoration; GTK4 keeps its CSD. Title bar 28 px, 1 px `#CFC6B4` hairline, 4 px top radius, 2 px terracotta tab on the focused window, faint 3 px warm offset shadow (14 %/8 % alpha), close hover terracotta. Title font: first of Inter SemiBold / Noto Sans SemiBold, Medium, Bold / DejaVu (Noto Sans Bold in the VM).
- **Animation defaults:** open 180 ms (scale 0.94 + fade), close 150 ms (texture snapshot), minimise/restore 220 ms (to/from the taskbar rect from `set_rectangle`, else bottom centre), move 200 ms. Speed multiplier via `--anim-speed`/`GLAZE_ANIM_SPEED` (no config key yet: `glaze-common` belongs to agent A; suggested key `[appearance] animation_speed`). Reduced motion from `[appearance] reduced_motion` or `--reduced-motion` zeroes all durations.
- **Placement:** new stacking windows cascade (28 px steps) around the centre of the work area; drag-to-edge snaps with `glaze_wm::detect_zone`.

## Agent A defaults (shell, design system v2, taskbar, start menu; 2026-11-12)

Chosen autonomously under the "no questions, pick sensible defaults" brief.
The user can overturn any of these; each is a config value unless noted.

| Topic | Default chosen | Where |
|---|---|---|
| Token home | New crate `glaze-design` (tokens, CSS, theme files, textures, motion maths). `glaze-common::{color, theme}` re-export it so old paths keep working. | `crates/glaze-design` |
| Clay accent | Brand terracotta `#D97757` is the accent *fill* with dark text `#2B1A12` (5.6:1); the old `#B5502C` + white is gone. Hairline `#CFC6B4`, panel `#F5F2EA`, ink `#3A3733`, soft accent `#F2D5C6` (mockup values). | `theme.rs` |
| Clay shadows | Hard 2-3 px warm offsets, blur always 0 (a theme file asking for blur is flattened with a warning), alpha capped at 0.35 at strength 2. | `theme.rs`, `overrides.rs` |
| Fonts | Inter UI and Source Serif 4 display requested first; the VM has neither, so Noto Sans / Noto Serif render (no package installed). | `Typography::clay()` |
| Theme file precedence | built-in < theme file < `[appearance.overrides]` < shortcuts (`accent`, `radius`, `stroke_weight`, `shadow_strength`, `panel.density`, `panel.height`). A theme file's `[meta] base` wins over `appearance.theme`. | `config_check.rs` |
| Bad config values | Per-key recovery: a wrong key is dropped with an error naming its path; the rest applies. A file that does not parse is reported and the running config is kept. | `config_check.rs` |
| Live reload | GIO file monitors (inotify) on `~/.config/glaze/` and the theme dirs; a change rebuilds CSS and taskbars (about 100 ms). | `shell.rs` |
| Textures | Sets `grain` (default), `fibre`, `laid`, `none`; intensity 0.5; tiles generated by `scripts/gen-textures.py`, alpha baked per intensity into `~/.cache/glaze/textures/`. `appearance.reduced_effects` drops textures and shadows. | `texture.rs` |
| Grouping | `always` by default (one button per app with a count pill), `auto` groups only when buttons would not fit, `never` one per window. | `bar.rs` |
| Click rules | Click focuses; click on the active button minimises all its visible windows (Sway scratchpad); click on a fully minimised button restores all; the hover card picks one window. Middle-click opens a new window; right-click: New window, Pin/Unpin, Close (all). | `bar.rs` |
| Pinned launchers | Separate section left of the tasks (mockup); click always launches. When `launcher.pinned` is empty, a file manager, terminal and browser are picked from installed apps (`panel.default_pins`). Pin/unpin rewrites `~/.config/glaze/config.toml` (comments in that file are not preserved). | `bar.rs`, `shell.rs` |
| Float/tile toggle | Per focused workspace by default (`panel.layout_scope = "global"` for all). Tile = `floating disable` on the workspace's windows; new windows on a tile workspace are tiled when Sway reports them (sway.conf still floats everything first, so there is one frame of floating). Restore from minimise follows the workspace mode. | `glaze-backend` (`LayoutMode`, `Backend::set_layout_mode`) |
| Tray | Only items backed by real state: network (sysfs), battery (hidden without one), volume (only when `wpctl` answers), notifications bell (dot when unread), StatusNotifier icons. | `panel_ui.rs` |
| Hover card | Titles, minimised state and close per window; no thumbnails until a screencopy path exists. | `panel_ui.rs` |
| Motion | `motion.style = smooth` (critically damped springs, no overshoot), `snappy` (0.65x), `off`; `motion.speed` 0.25-4. `stop-motion` is accepted but behaves as smooth for now. Reduced motion (config or GTK `gtk-enable-animations`) makes everything instant. | `motion.rs`, `anim.rs` |
| Auto-hide | `always` and `smart` both slide the bar off-edge leaving 3 px; `smart` has no overlap detection yet. | `shell.rs` |
| Start menu | Paper card 420 x 540, 5 pinned columns, anchored to the Start button, power row with inline confirmation for log out / restart / shut down. Recent apps in `~/.local/state/glaze/recent-apps`. | `launcher.rs`, `startmenu.rs` |
| Start button mark | Agent C's ink `start` (Kiln arches) glyph plus serif "Glaze"; the menu header uses `kiln-symbol-small` (copied into `apps/glaze-shell/resources/`). | `panel_ui.rs`, `icons.rs` |
| Live redeploy | `scripts/vm/redeploy.sh` (and `sync-build.sh --install`) restart `glaze-service` and `glaze-shell` inside the running tty1 session via `scripts/vm/restart-live.sh`. | `scripts/vm/` |

## Agent A defaults, polish pass 2 (2026-11-12)

| Topic | Default chosen | Where |
|---|---|---|
| Task button order | Creation order (Sway con ids grow), never stacking order: Sway lists floating windows by stacking, so buttons used to jump on every focus change. | `glaze_shell_core::bar::task_buttons` |
| Group click | A grouped button opens its window chooser (titles, minimised state, close per window, "Minimise all/Restore all", "Close all"); a single-window button keeps focus / minimise / restore. Hover still previews after 450 ms. | `panel_ui.rs` |
| Missed windows | Events stay primary. Every ~3 s the shell compares window ids with `GET_TREE` and fixes and logs any drift (`resync: event stream missed a change`). Sway requests time out after 2 s and reconnect once; a closed event stream re-subscribes. | `shell.rs`, `glaze-backend/sway.rs` |
| Sway title bars | Paper: cream title (`surface_raised`), ink text (Noto Sans Medium 9.5), 1 px frame, terracotta frame only on the focused window, hairline otherwise. Shell control strip uses ink glyphs, terracotta close hover. | `shell.rs::apply_frame_colors`, `frame_color_command` |
| Launch failures | Apps launched from the shell keep stderr in `~/.cache/glaze/apps/<id>.log`; a non-zero exit or signal within 4 s shows a toast with the last stderr line. | `glaze_service::launch::launch_watched`, `toast.rs` |
| Context menus | One shared flat menu (`menu.rs`) for desktop, task buttons and pins: 32 px rows, ink icon column, accel/chevron, hairline separators, header, tinted-accent hover/keyboard selection, cascades in the same popover, type-ahead. Desktop gets `New >` (Folder, Text file in `~/Desktop`) and `Arrange icons >`. | `menu.rs`, `desktop_ui.rs`, `panel_ui.rs` |
| Start word | UI sans (Inter, else Noto Sans) 600, 13 px, tight tracking; `panel.start_label_style = "sans" | "serif"`, `panel.start_label_weight` (300..800). Serif stays for the clock. | `Typography::start_label_*`, `config_check.rs` |
| Dark Clay | Bar `#2B2926`, raised `#33302C`, sunken `#24221F`, hairline `#4A4640`, ink/text `#F3EFE6`; the Start pill is tinted (`accent_soft` + terracotta 1 px inset edge) instead of a bright slab; ink icon stroke 1.5 px (both schemes). | `theme.rs`, `css.rs` |
| Task button states | Idle raised tab; hover one step lighter; active = faint terracotta tint, terracotta-mixed border, semibold label and the 2 px terracotta underline (main cue); minimised = dashed hairline, no fill, dimmed content. Tray well and float/tile toggle share one sunken-well treatment; the Quick Launch grip is a single hairline. | `css.rs` |
| Auto scheme | `appearance.scheme_mode = "auto"`: light 07:00-19:00, dark otherwise, re-checked every minute and applied live. No portal yet. | `config.rs::SchemeMode`, `shell.rs` |
| Live wallpaper | Config reload re-resolves the wallpaper and cross-fades it (450 ms) on the existing background surface. | `desktop_ui::set_wallpaper` |
| Redeploy | `scripts/vm/redeploy.sh --shell-only` skips the glaze-settings build (agent D's app). `scripts/vm/input.sh` drives real pointer/keyboard input (sway seat cursor + wtype). | `scripts/vm/` |

## Agent D defaults (settings app, 2026-11-12)

Chosen autonomously under the "no questions, pick sensible defaults" brief; the user can overturn any of them.

| Topic | Default chosen | Where |
|---|---|---|
| Structure | Windows 2000 Control Panel grouping in a sidebar: Personalise (Appearance, Wallpaper, Animation), Desktop (Taskbar, Start menu, Windows), System (Displays, Sound, Network, Power, Default apps, About). Header: Undo, Reset section, Import theme, Export theme; search in the sidebar. | `apps/glaze-settings/src/catalog.rs`, `ui/mod.rs` |
| Saving | Every control saves at once (no Apply button), except Displays, which has Apply plus a 15 s revert-or-keep check. Writes are `toml_edit` in-place edits, so comments, key order and unknown keys survive, and atomic (temp file + rename). One backup of both files per session and before every destructive action, under `~/.local/state/glaze/settings-backups/` (newest 10 kept). | `store.rs` |
| Keys the shell does not read yet | Saved to the sidecar `~/.config/glaze/settings.toml` and labelled "saved only" in the UI; listed as requests in `docs/settings-needs.md`. | `store.rs`, `docs/settings-needs.md` |
| Live apply | The file is the contract: the shell watches `~/.config/glaze/` and reloads. Settings sends no extra signal. Settings itself watches the same directory, so the window follows changes made elsewhere (shell pin edits, `--set`, an editor). | `ui/mod.rs::watch_config` |
| Single instance | GApplication id `org.glaze.Settings`. A second launch raises the open window (`present()` plus Sway `[app_id] focus`, which also brings it back from the scratchpad). | `ui/mod.rs` |
| Startup | `gtk::init()` runs first; with no display it prints the reason and exits 1. Pages are built the first time they are shown, so the window maps in about 0.8 s on the VM instead of 4 s. | `ui/mod.rs` |
| Window controls | Own minimise/maximise/close buttons (GTK's send xdg requests that Sway ignores for floating windows). On Sway: minimise = `move scratchpad` (the taskbar restores with `scratchpad show`); maximise = fill the workspace's usable area (taskbar excluded) and remember the old geometry; double-click on the title bar toggles maximise. Elsewhere the normal GTK requests. | `swaywin.rs`, `ui/wincontrols.rs` |
| Other apps' dark mode | Whenever Glaze's effective scheme changes (Light/Dark/Automatic, undo, reset, an external edit, the 07:00/19:00 switch while Settings is open), Settings runs `glaze-colour-scheme light|dark` so GTK 3/4, libadwaita, portal and Qt apps follow. Already-open GTK 3 and Qt apps need a restart; the row note says so. `--set/--unset appearance.scheme*` do the same. | `colourscheme.rs`, `ui/mod.rs::sync_system_scheme` |
| Renderer | `GSK_RENDERER=cairo` unless the user set `GSK_RENDERER`. On llvmpipe the window maps in 0.7-1.0 s with cairo against 1.3-1.5 s with GL (measured in the VM). Revisit on real GPUs. | `ui/mod.rs::run` |
| Scriptable | `--get KEY`, `--set KEY VALUE`, `--unset KEY` (same validation, backup and write path as the window), `--check`, `--export-theme`, `--import-theme`, `--reset-section PAGE --yes`, `--find`, `--list`. | `cli.rs` |
| Icons | Ink glyphs are recoloured as plain SVG text (`currentColor` replaced by the theme's ink colour) and loaded as textures, never as GTK symbolic icons. | `ui/mod.rs::render_icon` |
| Motion | Page transitions (crossfade), switch knob and scroll-to-match use the Glaze motion tokens scaled by `motion.speed`. `motion.style = off`, `appearance.reduced_motion` or GTK `gtk-enable-animations = false` make all of them instant. | `style.rs`, `ui/mod.rs` |
| Task button width | When buttons do not fit, labelled buttons first shrink evenly (labels ellipsize) down to 88 px; only then do labels drop (inactive first). | `bar::MIN_LABEL_WIDTH` |
| Menu keyboard focus | While any Glaze menu or the start menu is open, its layer surface takes exclusive keyboard focus; on close focus is released so Sway hands it back to the previous window. (Without this, keys went to a focused foot window.) The desktop surface is now on-demand so its menus get keys too. | `menu::hold_keyboard` |
| Menu placement | Pointer menus open with their left edge at the pointer and are clamped on screen by the shell (Sway does not slide popups). A cascade that would leave the screen drills down in place (parent column hidden, `‹ Title` header, Left/Esc back). | `menu.rs` |
| Animation clock | Springs step on the host's frame clock; when that clock stalls (popovers animated from the bar, idle layer surfaces) a 60 Hz timer steps them instead. Frame logs say how many frames were timer-driven. | `anim.rs` |
| Wallpaper decode | Live wallpaper swaps decode the image on a worker thread and cross-fade when ready (an SVG took ~2 s on the main loop on llvmpipe). | `desktop_ui::set_wallpaper` |

## Window snapping and resizing (user, 2026-11-12)

- Wanted: drag a window to a screen edge or corner to snap it (left/right half, quarters, top edge = maximise, optionally fullscreen), with a translucent paper-style snap preview; large, easy resize hitboxes on all sides and corners; GTK/Qt minimise buttons must work.
- Sway cannot do drag-snap previews or honour client minimise (`xdg_toplevel.set_minimized`). Interim on Sway: 6px borders for resizing, `$mod+m` minimise to scratchpad, keyboard snap bindings (`glaze-snap`). **The real implementation is the Glaze compositor session (M4)**: glaze-wm already has snap logic; add drag-to-edge/corner snap with preview, 12-16px invisible resize margins around frames, and set_minimized support. M4 is the next compositor milestone once taskbar/start/settings are polished.
| System colour scheme | glaze-shell runs `glaze-colour-scheme dark|light` (lead's script: GTK 3/4, libadwaita, portal, Qt) at session start and whenever its effective scheme changes (config edit, `scheme_mode = "auto"` schedule). The shell keeps painting Sway frames from the live tokens (so a custom accent follows) and passes `GLAZE_SWAY_FRAMES=skip`; the script's own frame colours equal the default Clay tokens for when it runs alone. | `shell.rs::apply_system_scheme` |
| Window frames | Paper title (`surface_raised`), ink text, terracotta title outline on the focused window. The lead's 6 px border (resize hitbox) is painted as part of the frame: faint terracotta-tinted paper (`surface_raised` 22 % toward accent) when focused, the bar colour otherwise, so it reads as a paper frame, not a thick line. | `apply_frame_colors`, `glaze-colour-scheme`, `sway.conf` |
| Minimise on Sway (limitation) | Sway ignores `xdg_toplevel.set_minimized`, so a GTK/libadwaita app's own title-bar minimise button does nothing in the Sway session. Reliable paths: click the active task button, task right-click > Minimise, the minimise button per window in the hover card / group chooser, the shell's title-bar strip on server-decorated windows, `Super+M` / `Super+H` / `Super+Down`. Restore: task button, chooser, `Super+Shift+M`. The real fix is the Glaze compositor session. | `panel_ui.rs`, `sway.conf` |
| Snapping (Sway session) | `glaze-snap` (Python, `/usr/local/bin`): halves, quarters, top/bottom, centre, maximise/toggle, restore (geometry remembered in `$XDG_RUNTIME_DIR/glaze-snap/`), restore-or-minimise. Work area = the focused workspace rect from Sway (output minus the taskbar's exclusive zone, so edge/size/auto-hide are followed). Keys: Super+Left/Right halves, Super+Up maximise toggle, Super+Down restore or minimise, Super+Shift+Left/Right top quarters, Super+Ctrl+Left/Right bottom quarters, Super+Shift+Up/Down top/bottom halves, Super+C centre. Task right-click > Snap > ..., and "Maximise / restore" and the title-bar maximise button use the same helper. Drag-to-edge snapping needs the Glaze compositor. | `packaging/kilnlinux/bin/glaze-snap`, `sway.conf` |

## Compositor session defaults (agent B2, M4, 2026-11-12/13; user may override)

Chosen autonomously under the "no questions, pick sensible defaults" brief.

| Topic | Default chosen | Where |
|---|---|---|
| Session backend | `glaze-compositor --backend udev`: libseat (seatd on the VM, the same as Sway), udev primary GPU, DRM/KMS via Smithay `DrmCompositor`, GBM + EGL + GLES (Mesa `kms_swrast` = llvmpipe on virtio-gpu). `--backend auto` picks winit when a parent session exists. | `backend/udev.rs`, `linux.rs` |
| Pixman fallback | Deferred: render elements are GLES-typed; EGL on `kms_swrast` works on the VM. The safety net is glaze-session's Sway fallback. | `docs/compositor.md` |
| Outputs | Every connected connector gets a CRTC and its preferred mode (`GLAZE_OUTPUT_MODE=WxH[@Hz]` overrides), laid out left to right; udev `changed` rescans (hotplug). The window manager's work area is the first output's. | `backend/udev.rs` |
| Cursor | Composited software cursor (no cursor plane: virtio-gpu cursor planes were unreliable for Sway too), xcursor theme from `XCURSOR_THEME`/`XCURSOR_SIZE` (Adwaita, 24), CSS names with X fallbacks; `wp_cursor_shape_v1` served. | `cursor.rs` |
| Pacing | Render only when dirty/animating and no flip pending (vblank-paced); after an empty render, frame callbacks come from a one-refresh timer so idle clients do not spin. | `backend/udev.rs` |
| VT switching | Ctrl+Alt+F1..F12 (XF86Switch_VT_n); pause suspends libinput and DRM, resume reactivates, resets the DRM compositors and rescans connectors. | `input.rs`, `backend/udev.rs` |
| Fallback | `glaze-session run --backend glaze` waits `GLAZE_COMPOSITOR_TIMEOUT` (20 s) for the compositor's Wayland socket (created only after the backend initialised) plus 1.5 s alive; otherwise it logs the reason and starts Sway. A compositor that dies abnormally mid-session writes `~/.local/state/glaze/compositor-failed`; logins within 10 minutes of it use Sway (logged). | `glaze-session` |
| tty2 test path | getty@tty2 autologin drop-in + `~/.bash_profile` tty2 branch, gated by `~/.config/glaze/tty2-session`; starts only once tty2 is the active VT; private `XDG_RUNTIME_DIR` (`/run/user/1000/glaze-tty2`) and `dbus-run-session`, so tty1's sockets, bus names and systemd environment are untouched; test binaries in `~/.local/glaze-m4/bin`. `scripts/vm/chvt.sh N` switches VTs (`kbd`'s chvt is not installed). | `packaging/kilnlinux/bash_profile`, `scripts/vm/install-tty2.sh`, `m4-deploy.sh` |
| Shell channel | The Glaze shell talks to the compositor over its control socket (`glaze_backend::GlazeBackend`), not wlr-foreign-toplevel: the shell's `Backend` trait is synchronous request/reply with stable ids, and the socket also carries workspaces, float/tile state, frame colours and a change stream. wlr-foreign-toplevel (and ext-foreign-toplevel-list) stay served for other tools. `session.compositor = auto` tries the socket first when `GLAZE_COMPOSITOR_SOCKET` is set. | `crates/glaze-backend/src/glaze.rs`, `apps/glaze-shell/src/backend.rs` |
| Control-socket verbs | Compositor-only verbs stay out of `glaze_common::ipc`: `minimize-window`, `restore-window`, `set-maximized` (null toggles), `set-fullscreen`, `move-to-workspace`, `window-state`, `get-layout`, `set-frame-colors`, `exit` (Log out), `subscribe` (event lines `{"kind":"event","scope":"window","change":"new"|"change"}`, sent once per dispatch only when the visible state changed). | `control.rs`, `linux.rs` |
| Frame colours | Built-in Clay light/dark palette from `~/.config/glaze/colour-scheme` (else `[appearance] scheme`/`scheme_mode`), `[appearance] accent` recolours tab, close hover and button tints; both files polled every second. The shell's live tokens (`set-frame-colors`) override until the next config change. | `state.rs::reload_config`, `frame.rs` |
| Motion from config | `[motion] speed` (duration x 1/speed), `style = snappy` (x0.65), `off` or `[appearance] reduced_motion` = instant; `--anim-speed`/`--reduced-motion`/`GLAZE_ANIM_SPEED` win over the config. | `state.rs::reload_config` |
| Decorations | Server-side paper frames forced on every client that speaks xdg-decoration; a client that asks for client-side a second time is considered to insist and gets CSD. | `handlers/decoration.rs` |
| Resize margins | 14 px invisible grab band outside every framed window, 22 px corner zones; cursor shapes follow the edge. | `frame.rs` |
| Drag snap | Left/right edge = half, corners = quarter, top edge = maximise, bottom edge (taskbar) = no snap. Preview: translucent accent-tinted paper sheet with a terracotta hairline, grows out of the window over 160 ms; the drop glides in (move animation). A snapped window remembers its free geometry: dragging it off or Super+Down restores it. | `input.rs`, `render.rs`, `wm.rs` |
| Keys (match sway.conf / glaze-snap) | Super+Left/Right halves, Super+Up maximise toggle, Super+Down restore-or-minimise, Super+Shift+Left/Right top quarters, Super+Ctrl+Left/Right bottom quarters, Super+Shift+Up/Down top/bottom halves, Super+C centre, Super+M minimise, Super+Shift+M restore last minimised, Super+F fullscreen, Super+Shift+Space float/tile one window, Super+Return and Ctrl+Alt+T terminal (`$TERMINAL`, foot), Super+Shift+E exit, Super+Shift+Q close, Super+1..4 / Super+Shift+1..4 workspaces, Super+T / Super+Shift+T tiling, Alt+Tab switcher, Super+left-drag move, Super+right-drag resize. (Super+Shift+Left/Right no longer move windows between workspaces, as in Sway.) | `input.rs::comp_binding` |
| Alt+Tab | Paper card centred on the primary output (cream card, hairline edge, 6 px radius, hard 3 px offset sheet), MRU rows with an app-initial chip; the previewed row is accent-tinted with a terracotta tab; minimised windows are listed and restored on commit. | `frame.rs::paint_switcher` |
| Menus' keyboard | A Top/Overlay layer surface with exclusive keyboard interactivity takes the keyboard when it maps; switching to none returns focus to the focused window. | `handlers/compositor.rs` |
| Log out | The shell's Log out asks glaze-compositor to `exit` over the control socket when `GLAZE_COMPOSITOR_SOCKET` is a socket, else `swaymsg exit`. | `apps/glaze-shell/src/main.rs::LOGOUT_SH` |
| Session choice on tty1 | `~/.bash_profile` reads `GLAZE_BACKEND`, else `~/.config/glaze/session-backend` (`glaze` or `sway`), default **sway**. Glaze is not the default yet (frame times on llvmpipe, see PROJECT_STATUS M4). Rollback: `echo sway > ~/.config/glaze/session-backend` or `~/.bash_profile.glaze-bak`. | `packaging/kilnlinux/bash_profile` |
| Stale sockets | glaze-session treats a `wayland-N` socket re-created after it started waiting as new (a killed compositor leaves its socket file and the next one reuses the name). | `glaze-session::wait_for_new_socket` |
| Render path | Direct, damage-tracked rendering into the scan-out buffer. An offscreen-composition path exists (`GLAZE_OFFSCREEN=1`); A/B on the VM at 1280x800 (Thunar drag, release build): direct avg 5.1-5.7 ms / p95 9.3-9.8 ms, offscreen 7.7 / 13.6 ms, so direct is the default. | `backend/udev.rs` |
| Minimise target | glaze-shell reports each task button's rectangle (relative to the `glaze-panel` layer surface) with `set-minimize-rects` about 0.4 s after the taskbar changes; minimise/restore animate to/from that rectangle, else towards the bottom edge under the window. | `panel_ui.rs::report_button_rects`, `state.rs` |
| Undecorated GTK 3 windows | KDE decoration mode `None` (splash screens, `decorated = FALSE`) gets no Glaze frame. | `handlers/decoration.rs` |
| Clipboard tools | `zwlr_data_control_manager_v1` served (with primary selection) for wl-clipboard and cliphist. | `handlers/seat.rs` |
| HiDPI | Output scale from `GLAZE_OUTPUT_SCALE` or `[appearance] scale` (fractional allowed); title bars are painted at 1x and scaled (slightly soft at 1.5x). | `backend/udev.rs::output_scale` |
| XWayland | Not provided yet (M5 decision). X11-only apps (VLC 3 GUI) need the Sway session. | |

## Agent A, Glaze pass (2026-11-13)

| Topic | Default chosen | Where |
|---|---|---|
| Start button text | "Start" next to the arch mark (user request), `panel.start_label` (empty = mark only), live-reloadable with the rest of `[panel]`. | `config.rs`, `panel_ui.rs` |
| Log Off row | "Log Off <full name>..." only when the account has a GECOS full name that differs from the login; otherwise plain "Log Off...". | `classic_menu.rs` |
| Legacy names | glaze-session copies `~/.config/fede`, `~/.local/share/fede`, `~/.local/state/fede` to the `glaze` names once (old dirs kept) and maps `session-backend = fede` to `glaze`. `glaze-colour-scheme` mirrors the scheme into `~/.config/fede/colour-scheme` while kiln apps built before the rename still read it. | `paths::migrate_legacy_dirs`, `glaze-colour-scheme` |
| Taskbar polish | Clock pill 4 px shorter than controls (no edge contact); warm low-opacity top rule; pinned launchers on a faint paper plate with a terracotta running mark; bar grain at 45 % intensity, half-size tile; tray well and toggle on a darker paper with a firmer hairline. | `css.rs`, `shell.rs` |
| Idle cost | Service accept loop sleeps in poll(2) (500 ms) instead of 20 Hz polling; shell clock touches widgets only when the text changes; tray/notification widgets rebuilt only when the status changes; the title-bar strip is tracked only while shown; unchanged strip positions skip work. | `ipc_server.rs`, `panel_ui.rs`, `shell.rs`, `wincontrols.rs` |
| Start click toggle | A click on Start while the menu is open closes it (the popup's autohide used to close and the click then reopened it). | `classic_menu.rs` |
| Window animations (round 3) | Open and close are a 10 px rise/sink plus fade at full size; moves (snap, tiling, maximise) glide the position on whole pixels while the size lands at once. No scaling, so llvmpipe samples the window 1:1. Minimise and restore are unchanged: shrink and fade toward/from the task button. | `glaze-compositor/src/anim.rs` |
| XWayland | Smithay's XWayland and X11 window manager; `DISPLAY` reaches the shell through `$XDG_RUNTIME_DIR/glaze/xwayland-display`. X11 windows get paper frames unless their Motif hints ask for none. Under Sway, glaze-session asks sway for its (lazy) XWayland `DISPLAY` and passes it on, so VLC 3 opens its GUI with a server-decorated title bar instead of a bare window. | `xwayland.rs`, `glaze-session` |
| Session lock | ext-session-lock; swaylock in Glaze colours (`GLAZE_LOCKER` overrides; `kiln-lock-ui` preferred when installed); a dead locker is replaced after 5 s, Super+L also works while locked. | `handlers/session_lock.rs` |
