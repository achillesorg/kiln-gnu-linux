# glaze-compositor

Glaze's own Wayland compositor (`crates/glaze-compositor`), built on Smithay 0.7
and structured after Smithay's MIT-licensed `anvil` example. Sway stays the
fallback: the Glaze shell only talks standard protocols plus the control
sockets, so it runs on either.

Status and evidence: see "Verification (M3)" below and `PROJECT_STATUS.md`.

## Running it

As the session compositor on a tty (M4, `--backend udev`: DRM/KMS + GBM +
EGL/GLES on llvmpipe, libinput, libseat):

```sh
glaze-session run --backend glaze        # what the login profile runs; falls back to Sway
glaze-compositor --backend udev --spawn foot   # bare, for debugging
```

Nested inside any Wayland (or X11) session (`--backend winit`, the default
when `WAYLAND_DISPLAY`/`DISPLAY` is set):

```sh
glaze-compositor --spawn foot --spawn 'swaybg -c "#D97757"' --spawn glaze-shell
```

| Option | Meaning |
|---|---|
| `--spawn CMD` | run `sh -c CMD` inside the compositor (repeatable). Children get `WAYLAND_DISPLAY`, `GLAZE_COMPOSITOR_SOCKET`, `XDG_CURRENT_DESKTOP=Glaze`; `DISPLAY` and `SWAYSOCK` are removed. |
| `--size WxH` | nested window size (default 1280x800; a tiling parent resizes it anyway) |
| `--reduced-motion` | no animations (also read from `[appearance] reduced_motion` in the Glaze config) |
| `--anim-speed F` | duration multiplier, 1.0 default, 2.0 = twice as slow (also `GLAZE_ANIM_SPEED`) |
| `--dark` | dark frame palette (also `[appearance] scheme = "dark"`) |
| `--tiling` | start with the global layout set to tiling |

| `--backend B` | `udev` (tty session), `winit` (nested) or `auto` |

`RUST_LOG` controls logging (default `info`). **Ctrl+Alt+Backspace** exits;
**Ctrl+Alt+F1..F12** switch VTs on the udev backend. `GLAZE_OUTPUT_MODE=WxH[@Hz]`
picks a connector mode; `XCURSOR_THEME`/`XCURSOR_SIZE` the pointer.

### tty2 test path (M4)

`scripts/vm/install-tty2.sh` adds a getty@tty2 autologin and a tty2 branch to
`~/.bash_profile` that starts `glaze-session run --backend glaze` (private
`XDG_RUNTIME_DIR=/run/user/1000/glaze-tty2`, `dbus-run-session`, binaries from
`~/.local/glaze-m4/bin`) once tty2 becomes the active VT. `scripts/vm/m4-build.sh`
builds and deploys; `scripts/vm/m4-check.sh start status shot NAME ...` drives
it (always ending on tty1); `scripts/vm/chvt.sh N` switches VTs.
`--remove` undoes the getty drop-in.

The VM test rig (`crates/glaze-compositor/scripts/`) builds in `~/glaze-comp`
and runs everything under a private `XDG_RUNTIME_DIR` with a throwaway
headless Sway as the parent, so it never touches the live tty1 session:

```sh
crates/glaze-compositor/scripts/vm-sync-build.sh          # rsync + cargo build in the VM
crates/glaze-compositor/scripts/vm-nested-check.sh         # start parent + compositor + clients, run checks, screenshots
```

## Architecture

| Module | Portable? | Role |
|---|---|---|
| `wm.rs` | yes, unit-tested | The window-management brain: windows, stacking order, focus, workspaces, minimise/maximise/fullscreen, remembered stacking geometry, Alt+Tab, keybinding actions. Uses `glaze-wm` policy. |
| `glaze-wm::layout_mode` | yes, unit-tested | Stacking <-> tiling toggle (`LayoutPolicy`), global mode plus per-workspace overrides, `arrange()` with a dwindle tree (default) or master/stack. |
| `frame.rs` | yes, unit-tested | Paper SSD frames: insets, button rects, hit-testing (title, buttons, resize edges/corners), palette (light/dark from the approved mockup), CPU title-bar painter (ab_glyph text). |
| `anim.rs` | yes, unit-tested | Time-based, interruptible, retargetable tweens; window open/close/minimise/restore/move animations; `MotionConfig` (speed, reduced motion); frame-time statistics. |
| `control.rs` | yes, unit-tested | Control socket protocol (newline JSON, `glaze_common::ipc`). |
| `state.rs` | Linux | Smithay protocol states, `WindowEntry` (window + frame + animation), `apply_layout()` which mirrors `wm` onto surfaces. |
| `handlers/` | Linux | One file per protocol (compositor/commit, xdg-shell, xdg-decoration, layer-shell, seat/selection). |
| `foreign_toplevel.rs` | Linux | `zwlr_foreign_toplevel_manager_v1` v3 **server** (list/activate/minimise/maximise/fullscreen/close, `set_rectangle`). |
| `input.rs` | Linux | Keyboard shortcuts, pointer hit-testing over frames, focus-on-click, move/resize grabs, drag-to-edge snapping. Generic over Smithay's `InputBackend`. |
| `render.rs` | Linux | Builds render elements (layers, windows through their animation transform, frames, closing-window ghosts, client cursor) for the damage tracker; frame callbacks; texture snapshots for close animations. |
| `backend/` | Linux | `BackendData` enum: `winit` (nested, GLES) and `udev` (session: libseat, udev hotplug, DRM/KMS `DrmCompositor` per connector, GBM + EGL + GLES, libinput, vblank pacing, VT switching). |
| `cursor.rs` | Linux | xcursor theme loader for the software pointer (DRM). |
| `bin/glaze-toplevel-ctl.rs` | Linux | Small wlr-foreign-toplevel client used by the scripted checks (`list`, `activate`, `minimize`, `unminimize`, `maximize`, `close`). |

The rule that keeps it simple: **policy changes go through `wm`, then
`State::apply_layout()` mirrors the result onto surfaces** (map/unmap,
configure size and states, keyboard focus, animations, foreign-toplevel
state). Rects in `wm` are outer frame rects; `frame::Insets` converts.

### Protocols served

`wl_compositor`, `wl_subcompositor`, `wl_shm`, `wl_seat` (keyboard, pointer),
`wl_data_device_manager` (clipboard, DnD), `zwp_primary_selection_device_manager_v1`,
`wl_output` + `zxdg_output_manager_v1`, `xdg_wm_base` (toplevels, popups with
grabs and unconstraining), `zxdg_decoration_manager_v1` (forces server-side),
`zwlr_layer_shell_v1`, `wp_viewporter`, `wp_fractional_scale_manager_v1`,
`ext_foreign_toplevel_list_v1`, `zwlr_foreign_toplevel_manager_v1`.

Not yet: screencopy (so `grim` cannot capture *through* the compositor; the
checks capture the parent instead), `xdg_activation`, cursor-shape,
session-lock, idle, input-method/text-input, XWayland, presentation-time.

## Window management

- **Stacking by default** (Win2K): new windows are cascaded around the centre
  of the work area, focused and raised. Click focuses and raises.
- **Frames**: drag the title bar to move; double-click it to maximise; the
  three buttons minimise, maximise/restore and close; drag any edge or corner
  (14 px invisible grab band outside the frame, 22 px corners, with
  matching cursor shapes) to resize. Super+left-drag moves and
  Super+right-drag resizes from anywhere on a window. Clients with their own decorations
  (GTK4) can still move/resize through `xdg_toplevel.move/resize`.
- **Drag to an edge** snaps (left/right half, quarters at corners, top =
  maximise; the taskbar edge does not snap) using `glaze_wm::detect_zone`; a
  translucent paper preview grows out of the window while dragging, and the
  window glides into place on release. Dragging a snapped window off (or
  Super+Down) brings back its free size.
- **Workspaces**: 4, `Super+1..9` switch, `Super+Shift+1..9` move the focused
  window, `Super+Tab`/`Super+Shift+Tab` next/previous.
- **Alt+Tab / Alt+Shift+Tab**: MRU cycling; each press previews (raises and
  focuses), releasing Alt commits.
- **Minimise**: `Super+H`, the frame button, the taskbar (foreign-toplevel
  `set_minimized`) or `xdg_toplevel.set_minimized`. The window really leaves
  the screen; activating it from the taskbar or Alt+Tab restores it.
- Other keys match the Sway session (`sway.conf` + `glaze-snap`): see the
  "Keys" row in `docs/decisions.md` (M4). Compositor bindings
  (`input::comp_binding`) win over `glaze_wm::Keymap::default_glaze()`.
- **Alt+Tab** shows a paper switcher card (MRU list) while Alt is held.

## Tiling toggle

Stacking is the default everywhere. Tiling can be switched **per workspace**
or **globally**:

| Action | Effect |
|---|---|
| `Super+T` | toggle the active workspace (sets an override on it) |
| `Super+Shift+T` | toggle the global mode and clear every per-workspace override |
| control socket `set-window-layout` | either of the above, or set a mode explicitly |

Tiled windows are laid out by a **dwindle** tree (each window takes half of
the remaining space, splitting the longer side first) with an 8 px gap,
inside the work area (layer-shell exclusive zones excluded). Windows keep
their stacking geometry while tiled, so toggling back is lossless. Dragging a
tiled window pulls it out of the tiling (it floats); toggling the workspace
again re-tiles it. Dialogs (toplevels with a parent) always float. The change
animates (each window glides to its new rect).

### Control socket request (for the taskbar's float/tile button)

Socket: `$GLAZE_COMPOSITOR_SOCKET` (exported to children), default
`$XDG_RUNTIME_DIR/glaze/compositor.sock`. Newline-delimited JSON, the same
framing and types as `glaze-service` (`glaze_common::ipc`). The new request is
`Request::SetWindowLayout` (added to `glaze_common::ipc`, clearly marked):

```json
{"kind":"set-window-layout","mode":"toggle","workspace":0}
{"kind":"set-window-layout","mode":"tiling","workspace":null}
```

- `mode`: `"stacking"` (alias `"floating"`), `"tiling"`, or `"toggle"`.
- `workspace`: zero-based index, or `null` for the global mode (clears
  per-workspace overrides).
- Reply: `{"kind":"ok"}` or `{"kind":"error","code":"invalid-request"|"not-found",...}`.

The same socket also answers `hello`, `ping`, `list-windows`,
`list-workspaces` (the shell's `WindowInfo.floating` is `false` for tiled
windows, so the taskbar toggle can show the current state),
`activate-window` and `close-window`. Anything else returns
`invalid-request` ("ask glaze-service"). `glaze-service` answers
`set-window-layout` with `backend-unavailable`.

Example: `printf '%s\n' '{"kind":"set-window-layout","mode":"toggle","workspace":null}' | socat - UNIX-CONNECT:$GLAZE_COMPOSITOR_SOCKET`
(or `python3` / `nc -U`).

## Taskbar data without Sway

`zwlr_foreign_toplevel_manager_v1` (v3) lists every window with title,
app_id and state (activated/maximized/minimized/fullscreen) and accepts
`activate`, `set_minimized`/`unset_minimized`, `set_maximized`/`unset_maximized`,
`set_fullscreen`/`unset_fullscreen`, `close` and `set_rectangle`. The
rectangle (taskbar button, relative to the taskbar's layer surface) becomes
the target of the minimise animation and the source of the restore
animation. `ext_foreign_toplevel_list_v1` is served too (read-only).

## Frames (server-side decorations)

Forced server-side for every client that creates an xdg-decoration object
(foot, Qt, most non-GTK toolkits). GTK4 never asks and keeps its own
headerbar; those windows get no Glaze frame.

Look (colours from `$HOME/fedeos/mockups/taskbar-v2.png`):
title bar 28 px `#F3EFE6` (inactive `#F8F6F0`), 1 px hairline `#CFC6B4` around
the window, 4 px rounded top corners, title 12.5 px in `#2B2926` (inactive
`#8A847A`), 9 px line glyphs stroked 1.25 px in `#3A3733`, button hover
`#EBD9CD`, close hover terracotta `#D97757` with a cream X, a 2 px terracotta
"paper tab" on the focused window's top edge, and a faint 3 px tone-on-tone
offset shadow (warm brown at 14% / 8% alpha). No blur, no gradients, no black
shadows. A dark palette exists (`--dark`). Title text uses the first font
found from `frame::FONT_CANDIDATES` (Inter SemiBold, Noto Sans
SemiBold/Medium/Bold/Regular, DejaVu); in the VM that is Noto Sans Bold.

The title bar is painted on the CPU into a small ARGB buffer only when its
width, title, focus or hover state changes, then uploaded once.

## Animation catalogue (round 3)

All durations scale with `[motion] speed`, `style = snappy`, and the new
`[motion] intensity = "off" | "low" | "normal" | "high"` (off = no motion,
low = 0.6x time and small offsets, high = 1.25x time, larger offsets and a
0.97 -> 1 scale on open). Reduced motion always wins.

| What | Motion |
|---|---|
| Open | rise + fade (spring), full size; High adds a 0.97 -> 1 scale |
| Close | sink + fade from a snapshot of the last frame (230 ms, ease-in-quad: the fade shows from the first frame) |
| Minimise / restore | shrink + fade to / grow from the task button (rect from the shell), unchanged |
| Maximise, snap, tiling toggle, fullscreen | size + position spring on a frozen snapshot of the window (contents never jump); live window takes over at the end |
| Workspace switch | old windows slide out, new ones slide in (full output width, spring) |
| Focus change | title bar crossfade old -> new (accent tab and ink) |
| Alt+Tab | card fades and scales 0.96 -> 1 |
| Menus, toasts, start menu (layer surfaces except the bar) | open: fade + rise 6 px (140 ms); close: the mirror, fade + sink 6 px from a snapshot (120 ms), under the taskbar; reopening fades in again |
| Session lock | lock surface fades in over the paper background |
| Log out | fade to ink, then the compositor exits |

On a software renderer (low-power path, below) the snapshot morph, the
title crossfade, the High open scale and the frame shadow strips are off;
everything else is translate + fade at 1:1.

Not implemented: shake-to-minimise, move/resize settle bounce, unlock fade.

## Animation plumbing

- `anim::Tween`: time-based (`Duration` since compositor start), so a slow
  llvmpipe frame skips ahead instead of slowing motion down; `retarget()`
  continues from the current value with no jump and scales the remaining
  duration by distance.
- `anim::WindowAnim`: open (scale 0.94 -> 1, fade in, 180 ms out-cubic),
  close (reverse, 150 ms, from a texture snapshot of the dying window),
  minimise (fly to the taskbar rect and fade in the last third, 220 ms),
  restore (from the taskbar rect, 220 ms), move (glide between rects, 200 ms,
  used by tiling toggle, snapping, maximise and workspace layout changes).
  A minimise interrupted by a restore reverses from where it is.
- Durations are multiplied by `--anim-speed` and collapse to zero with
  reduced motion (reduced motion always wins).
- Rendering: animated windows are drawn through Smithay's
  `RescaleRenderElement` + `RelocateRenderElement`; frames are solid elements
  with stable ids, so the damage tracker only repaints what moved. A frame is
  rendered only when something is dirty or an animation runs.
- Pacing: nested, frames are paced by a 60 Hz calloop timer (winit runs with
  vsync off). The DRM backend will pace on vblank.
- Frame-time logging: every 5 s, `frame-time (render+submit, CPU wall clock)`
  with frames, fps, min/avg/p95/max ms.

## Session backend (M4)

`backend/udev.rs`: libseat session (seatd on the VM), udev primary GPU,
`DrmDevice` + GBM + EGL on GBM + `GlesRenderer` (llvmpipe through
`kms_swrast`), one `DrmCompositor` per connected connector (preferred mode,
hotplug rescans on udev `changed`), libinput through `LibinputInputBackend`.
Frames render only when dirty or animating and never while a flip is pending
(vblank-paced); after an empty render, frame callbacks come from a one-refresh
timer. VT switch: pause suspends libinput + DRM, resume reactivates, resets
the compositors and rescans. The pointer is composited (no cursor plane).
Pixman fallback: not implemented (render elements are GLES-typed). If EGL/GBM
cannot initialise, the compositor exits before creating its socket with
`DRM: no connected output could be driven (<card>: <EGL error>)` and
`glaze-session` starts Sway (verified with the Mesa driver path broken on
purpose: `docs/screens/M4/egl-failure-fallback.log`).

Rendering goes straight into the scan-out buffer (damage-tracked).
`GLAZE_OFFSCREEN=1` composes in an offscreen texture and copies only the damage
instead; on the VM it was slower (see `docs/decisions.md`).

Output scale: `GLAZE_OUTPUT_SCALE`, else `[appearance] scale` (1, 1.25, 1.5,
2...). The pointer theme comes from `XCURSOR_THEME`/`XCURSOR_SIZE`, else
`~/.config/gtk-3.0/settings.ini` (`gtk-cursor-theme-name`/`-size`), else
Adwaita 24.

Clipboard: `wl_data_device`, primary selection and
`zwlr_data_control_manager_v1` (so `wl-copy`/`wl-paste` and `cliphist` work).
No XWayland: X11-only programs (VLC 3's Qt interface) start without a window
(VLC falls back to its command-line interface); use the Sway session for them.

`glaze-session run --backend glaze` starts the compositor first and waits up to
`GLAZE_COMPOSITOR_TIMEOUT` (20 s) for its Wayland socket (created only after
the backend is up) and 1.5 s more alive; failure is logged with the reason
and Sway starts instead. An abnormal compositor exit during a session writes
`~/.local/state/glaze/compositor-failed`; a login within 10 minutes of that
uses Sway (logged).

### Control socket extensions for the Glaze shell

Besides the `glaze_common::ipc` requests above: `minimize-window {id}`,
`restore-window {id}`, `set-maximized {id, maximized: bool|null}`,
`set-fullscreen`, `move-to-workspace {id, workspace}`, `window-state {id}`,
`get-layout {workspace?}` -> `{"kind":"layout","mode":...}`,
`set-frame-colors {focused_bg, focused_text, focused_border, other_bg,
other_text, other_border}`, `exit`, and `subscribe` (the connection then
receives `{"kind":"event","scope":"window","change":"new"|"change","id"?}`
lines). The shell uses these through `glaze_backend::GlazeBackend`.
