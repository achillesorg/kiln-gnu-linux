# Glaze — project status

_Last updated: 2026-11-12 (shell pass by agent A; M0 baseline below)_

**Target (since M0, ADR-0006):** KilnLinux = Debian 13 arm64 in a QEMU VM
(HVF, `virtio-gpu-pci` without virgl, so llvmpipe) on an Apple Silicon macOS
host. Fedora and the Windows authoring host are no longer targets; the tables
further down that name them are **historical** (iterations 1-3) and kept as a
record. Plan: `docs/overhaul-plan.md`; decisions: `docs/decisions.md`.

## Compositor as the session (M4, agent B2, 2026-11-12/13): works on tty2, NOT yet the default

Glaze compositor (`crates/glaze-compositor`) has a DRM/KMS session backend and
runs as the compositor of a tty. Tested on **tty2** with real QEMU input (QMP
tablet + keyboard); **tty1 stays Sway** (the login profile now reads
`~/.config/glaze/session-backend`, default `sway`). Release build, llvmpipe,
output Virtual-1 1792x1344 (1280x800 before the VM restart). Evidence:
`docs/screens/M4/`, `~/session-tty2.log`. Defaults: `docs/decisions.md`
("Compositor session defaults (agent B2, M4)").

| Check | How | Result |
|---|---|---|
| DRM backend | tty2, udev backend | seatd session, card0, GBM + EGL `PLATFORM_GBM_KHR`, GLES "llvmpipe (LLVM 19.1.7)" (`drm-first.png`) |
| VT switch | `scripts/vm/chvt.sh 2`/`1`, dozens of round trips | `session paused`/`resumed` logged, both sides redraw, no crash |
| Session | `glaze-session run --backend glaze` | compositor ready, glaze-shell on the `glaze-compositor` backend, wallpaper + taskbar (`session-3.png`, `rel-two-windows.png`) |
| GTK 3 paper frames | Thunar | `ssd=true` via org_kde_kwin_server_decoration; paper title bar with min/max/close (`rel-two-windows.png`) |
| GTK own minimise | glaze-settings' minimise button | `minimised` (xdg set_minimized), dimmed task button kept, restore from the taskbar works (`step2-gtk-minimised.png`) |
| Drag snap | drag Thunar's title to the left edge, then to the top-right corner (hold) | translucent paper preview with terracotta hairline (`snap-preview-left.png`, `snap-preview-quarter.png`), `drag snap zone=LeftHalf/TopRight`, window fills the half/quarter (`snap-settled-left.png`, `snap-settled-quarter.png`); first run showed the old size coming back (client commit raced the configure), fixed |
| Resize margin | pointer 8 px outside foot's right edge, drag 160 px | `ew-resize` cursor (`resize-margin-cursor.png`), window widened (`resize-margin-done.png`) |
| Alt+Tab | hold Alt, Tab twice | paper switcher card with MRU rows (`alttab-card.png`), `CycleWindows` x2 |
| Keys | Super+Left / Up / Down / Down / M / Shift+M | left half, maximised above the taskbar, restore, minimise, restore last (`key-super-left.png`, `key-super-up.png`, `key-super-m.png`; log `key binding action=...`) |
| Tiling toggle from the taskbar | click the tile, then the float button | `set-window-layout tiling` then `stacking` over the control socket; tiles side by side and back (`taskbar-tile.png`, `taskbar-float.png`) |
| Crash -> Sway | `kill -9 glaze-compositor` on tty2 | marker written; next login: `Glaze compositor unavailable: it failed less than 10 minutes ago ...`, `FALLING BACK TO SWAY`, Sway session (`crash-fallback.log`, `crash-fallback-sway.png`) |
| No socket -> Sway | VM starved by host swap | `no Wayland socket within 40s ... FALLING BACK TO SWAY` (logged) |
| 10-minute soak | `scripts/vm/m4-soak.sh 10`: 84 real-input actions (snaps, Alt+Tab, minimise/restore, tiling toggle) with foot, Thunar, Settings | same compositor pid start/end, 0 `ERROR`/`panicked` lines (`soak-end.png`) |

**Frame times, round 2 (2026-11-13, calm host, display fixed at 1280x800,
release build, llvmpipe; render + queue CPU wall clock per frame, from the
compositor's 5 s `frame-time` lines; `scripts/vm/m4-perf.sh`):**

| Scenario | fps | avg | p95 | flip-to-flip |
|---|---|---|---|---|
| Idle with pointer motion | 60-107 | 1.3-1.9 ms | 4-7 ms | 7-9 ms |
| Dragging Thunar (8 s circles) | 53-66 | 5.1-5.7 ms | 9.2-9.8 ms | 14-16 ms |
| Same, offscreen composition (`GLAZE_OFFSCREEN=1`) | 47-60 | 7.7 ms | 13.6 ms | 16.5 ms |
| Open + close foot x4 (client start-up competing for the 4 vCPUs) | 7-19 | 13-31 ms | 51-95 ms | 16-18 ms |
| 10-min soak, 5 apps incl. Firefox, snaps/tiling/minimise every 6 s | 2,645 frames | 15.6 ms | median 73 ms, 90th pct 181 ms (per 5 s window) | 14 ms |

The first round's seconds-long frames were host starvation (UTM VM at 97 %
CPU and full macOS swap; display 1792x1344): `perf` then showed 74 % of the
time in llvmpipe's JIT rasteriser and 18 % in kernel page faults. Drags and
snaps are now under the 33 ms target; open/close and whole-desktop relayouts
with several large clients are not.

Round-2 fixes: snap no longer reverts to the old size (configure in flight);
**compositor deadlock on window close fixed** (the close-animation snapshot
re-locked a surface inside a surface-tree walk; LibreOffice's splash closing
froze the session); glaze-session treats a re-created `wayland-N` socket as new
(stale socket from a killed compositor caused false Sway fallbacks); GTK 3
undecorated windows (splash) get no frame; task-button rectangles from the
shell drive minimise/restore (`set-minimize-rects`, log `minimise animation
... button=true`); `zwlr_data_control_manager_v1` (wl-copy/wl-paste, primary
selection: `clipboard.log`); output scale from `GLAZE_OUTPUT_SCALE` /
`[appearance] scale` (`hidpi-1.5.png`); cursor theme also from GTK
`settings.ini`; clean exit with the EGL error when GBM/EGL cannot start.

| More checks | Result |
|---|---|
| Missing compositor binary | `glaze-compositor is not installed (not on PATH)`, `FALLING BACK TO SWAY` (`missing-binary-fallback.log`) |
| EGL/GBM broken (Mesa driver path removed for the compositor only) | compositor exits 1 before its socket, Sway starts (`egl-failure-fallback.log`) |
| Daily use on tty2 | Firefox, Kiln Files/Text/Calculator, LibreOffice Writer, foot, Thunar, Settings together (`daily-apps2.png`); workspaces Super+2/Super+1 (`ws2.png`); start menu (`startmenu3.png`); Log Off from the start menu ends the session cleanly (exit 0, no crash marker, `logoff-confirm.png`) |
| Second 10-min soak (release, 5 apps) | 92 actions, same compositor pid, 0 ERROR/panic |

Not verified / known issues: **no XWayland** (VLC 3's GUI needs X11 and falls
back to its CLI); no lock screen (`loginctl lock-session` has no listener);
Shut down/Restart from the start menu not clicked (they run `systemctl
poweroff/reboot`, same as the Sway session, and would stop the shared VM);
multi-output and hotplug only unit-tested (`next_output_x`) because the VM has
one virtual head; title bars are painted at 1x and look soft at 1.5x; the
first click on the Start button after a session start can be missed until the
pointer moves (seen once); LibreOffice Writer opens wider than the screen
(client-chosen size); open/close and multi-window relayout frame times above.

**Default session: NOT switched (tty1 stays Sway).** Remaining blockers:
open/close and relayout p95 above 33 ms, no XWayland for X11-only apps, no
lock screen. To use Glaze on tty1: copy `~/.local/glaze-m4/bin/*` into
`~/.local/bin`, then `echo glaze > ~/.config/glaze/session-backend` (next
login). Rollback: `echo sway > ~/.config/glaze/session-backend`, or
`GLAZE_BACKEND=sway`, or `cp ~/.bash_profile.glaze-bak ~/.bash_profile`.

## Shell, Glaze pass (agent A, 2026-11-13)

- Live tty1 session migrated: `glaze-session run --backend sway`, `glaze-service`, `glaze-shell` (FeDe binaries, `fede-*.desktop`, `/usr/local/bin/fede-*` removed; `~/.config|.local/share|.local/state/fede` merged into `glaze` once, marker `.migrated-from-fede`). `scripts/vm/migrate-glaze.sh` does the system part.
- Start button reads "Start" (`panel.start_label`, default "Start"); Log Off row shows "Log Off..." (GECOS name equals the login). Screens: `docs/screens/shell/g03-start-menu.png`, `g04-bar-dark-2x.png`, `g05-bar-light-2x.png`.
- Idle cost (VM idle, `scripts/vm/perf-shell.sh 30`): glaze-shell 0.63 % of one core, 8.4 wake-ups/s, RSS 198 MiB; glaze-service 0.20 %, 2.5 wake-ups/s, 2 MiB. Before (FeDe build, VM under build load, so not a clean comparison): shell 1.53 %, 263 MiB; service 1.66 %. Shell start to first frame: 969 ms (idle VM).
- Not verified: start-menu open latency on an idle VM (416 ms measured only under load 23); kiln-terminal/kiln-files still read `~/.config/fede/colour-scheme` (mirrored by `glaze-colour-scheme` until they are rebuilt).

## Shell: taskbar, start menu, menus (agent A, 2026-11-12)

Live tty1 Sway session in the VM, 1280x800 scale 1 (the settings agent had
switched the output to 1920x1440 @1.75 for a while; screenshots before
`v3-*` are at that size). Driven with real input: `scripts/vm/input.sh`
(sway seat cursor + wtype) and `scripts/vm/qmp_pointer.py` (QEMU USB tablet).
Evidence: `docs/screens/shell/`, review sheets
`$HOME/fedeos/mockups/taskbar-v3-{light,dark}.png` (real captures).

| Check | How | Result |
|---|---|---|
| Every Sway window gets a task button | `swaymsg -t get_tree` vs bar; `org.glaze.Settings`, Thunar, Text Editor, VLC (XWayland class), Evince, Calculator, foot x2 | all shown with names/icons (`s09-shrink.png`); a missed event is now repaired by a 3 s resync that logs drift |
| Click focus / minimise / restore | 3 clicks on the Thunar button, tree after each | `focused=True` -> `scratchpad=True` -> `focused=True` |
| Middle-click new window | mclick Thunar | thunar windows 1 -> 2 |
| Group chooser | click "Foot 2" | chooser with both windows, Minimise all / Close all (`v3-*-group-chooser.png`) |
| Context menu + cascade + Esc | right-click task, hover "Move to workspace", Esc, Esc | `s14`/`s15`/`s16-esc*`, `v3-*-task-menu.png` |
| Desktop menu | right-click desktop, `Arrange icons >` | flat menu, cascade or drill-down near the edge (`v3-dark-desktop-menu.png`) |
| Start menu | click Start, type "text", Down, Esc, Esc (foot focused) | results filter, selection moves, Esc clears then closes; foot received no keys (`v3-kbd-{a,b,c}.png`) |
| Programs cascade | hover Programs, category | `s12-programs-cascade-dark.png` |
| Title-bar controls (Sway) | click min / max / close on Thunar's title strip | scratchpad / fills 0,0 1280x766 (bar is 34 px) and back / window gone |
| Float/tile toggle | click tile, float | 8 floating_con -> 8 con -> 8 floating_con (`v3-tiled.png`) |
| Snapping keys | `glaze-snap` via Super+arrows on Thunar | left 0,0 640x766; right 640,0; Up maximise and back; Shift+Left 0,0 640x383; Ctrl+Right 640,383; Down restores 136,67 1008x613 |
| Border resize (6 px band) | `scripts/vm/test-resize.sh 31`: real USB-tablet drags via QMP on all 4 edges and 4 corners | all 8 change the geometry as expected (e.g. left 1008->1028 wide, top-left 1114x708 -> 1134x728 at 36,9); needed `floating_maximum_size 0 x 0` (the old 1000x640 clamp blocked enlarging); new windows are fitted into the work area by the shell instead |
| Snap menu | task right-click > Snap > Left half on Thunar | 0,0 640x766 (`v3-dark-snap-cascade.png`) |
| Frames follow scheme | `swaymsg reload` | shell logs `sway reloaded its config; repainting window frames` (dark frames restored) |
| Launch failure toast | test entry exiting 101 with a GTK panic line | toast "Crash Test closed right after starting / It exited with status 101: GTK has not been initialized" (`v3-toast-launch-failure.png`) |
| Live wallpaper / scheme | config `scheme` light <-> dark | `wallpaper (live): ...paper-hills-light...` then `...dark...`, CSS and bars rebuilt, no restart |
| Animation frame times | `[glaze-shell] anim ...` log lines | start menu 37-136 fps, menus 40-88 fps, taskbar 14-54 fps under load (some frames timer-driven where the frame clock stalls); honest numbers in the log |
| Host gates | `cargo test` (glaze-common, -design, -shell-core, -backend, -service, -a11y, -session, integration), clippy `-D warnings`, fmt; a11y report | pass; a11y 44/44 contrast incl. the new dark palette; glaze-shell clippy clean on the VM |

Not verified / known issues: GTK apps' own minimise buttons do nothing on
Sway (it ignores `set_minimized`; use the bar, chooser, Super+M); drag-to-edge
snapping needs the Glaze compositor; no window thumbnails in the hover card;
portal colour-scheme not read for `scheme_mode = auto`; desktop icons are not
rebuilt on live reload; live reload with a wallpaper change takes ~0.5-2 s
(mostly theme/CSS rebuild on llvmpipe); the first popup after a shell restart
may need the pointer to move first (Wayland enter).

## M0 status: done-criteria met in the VM (2026-11-11)

Run after a full VM reboot and tty1 autologin (no manual steps in the VM):

| Check | Command | Result |
|---|---|---|
| No stray bar | `pgrep swaybar`; `pgrep swaybg` | **empty (rc=1)** for both |
| Sway uses Glaze config | `pgrep -a sway` | `sway --unsupported-gpu -c /home/fede/.config/glaze/sway.conf` |
| Shell env | `tr '\0' '\n' </proc/$(pgrep -x glaze-shell)/environ` | `SWAYSOCK=/run/user/1000/sway-ipc.1000.692.sock`, `WAYLAND_DISPLAY=wayland-1`, `XDG_SESSION_TYPE=wayland`, `DBUS_SESSION_BUS_ADDRESS=unix:path=/run/user/1000/bus` |
| Activation env | `session.log`; `systemctl --user show-environment` | `activation environment updated: WAYLAND_DISPLAY=wayland-1 SWAYSOCK=... XDG_CURRENT_DESKTOP=Glaze XDG_SESSION_TYPE=wayland XDG_SESSION_DESKTOP=glaze`; the systemd1 activation error is gone (0 matches); systemd user env has all four vars |
| Windows reach the service | `swaymsg exec foot; glaze status` | before: `windows : 0`; after: `backend : sway-ipc`, `windows : 1` |
| Window is floating | `swaymsg -t get_tree` | `floating_con foot 'foot' focused=True {x:290,y:127,w:700,h:496}` |
| **Taskbar shows the window live** | `session.log` + screenshot | `[glaze-shell] taskbar: 1 live window(s), 1 button(s)`; `docs/screens/M0/11-foot-task-button.png` shows a focused terracotta "foot" button. Killing foot: `0 live window(s), 0 button(s)`, `12-idle-after-foot-closed.png` |
| Host gates (macOS arm64) | `cargo fmt --all -- --check`; `cargo clippy --workspace --all-targets -- -D warnings`; `cargo test --workspace` | **clean; 0 warnings; 626 passed, 0 failed** |
| GTK apps, real build (VM arm64) | `scripts/build.sh --release`; `cargo clippy --release --all-targets -- -D warnings` in `apps/glaze-shell` and `apps/glaze-settings` | **builds and links; clippy exit 0 for both** |

Why the taskbar never showed windows before M0 (all fixed): (1) sway was a
child of `glaze-session`, which never exported `SWAYSOCK`, so the shell and
service had no backend; (2) the shell read windows **once** at startup and
never again; (3) `panel_ui::build` drew the taskbar strip from the first zone
only (`Pinned`), so running windows in the `Task` zone were dropped even when
known; (4) GTK's Adwaita button background and backdrop label colour overrode
Glaze CSS, so a task label was white on white (`02-foot-taskbar.png`).

M0 changes: `glaze-session` (SWAYSOCK discovery, `-c ~/.config/glaze/sway.conf`,
explicit session env for children, `dbus-update-activation-environment`),
`packaging/sway/sway.conf`, `packaging/kilnlinux/bash_profile` (no
`dbus-run-session`), live taskbar poller (750 ms `GET_TREE`) and single strip
in `glaze-shell`, `glaze-shell-core::view::taskbar_entries`, `glaze status` asks
the service for backend and window count, `background-image: none` and label
colour inheritance for panel buttons, harness `scripts/vm/{sync-build,shot,install-login}.sh`
plus `qmp_screendump.py`, QMP socket in `$HOME/fedeos/run.sh`, ADR-0006,
and the Debian/macOS rewrite of `AGENTS.md`, `GLAZE.md`, `GOAL.md`,
`START_PROMPT.md`.

Not verified in M0: clicking a task button (focus/minimise) — the click path
exists (`activate_window` over Sway IPC) but nobody clicked it; the dark
scheme and Classic theme on screen; x86_64; the `.desktop` session entry under
a display manager (the VM has none).

Known issues found in M0 (not fixed): QMP screendump can return a frame a few
seconds old (`shot.sh` now waits 2 s); a stale `session.json` made session
restore launch `glaze-settings`, which exited at once and stayed a zombie
(`glaze_service::launch` drops the `Child` without reaping); the floating-size
clamp in `sway.conf` is tuned for the 1280x800 VM output.

## Historical: verification on the Windows host (iterations 1-3)

| What | Target / env | Command | Result |
|---|---|---|---|
| Portable workspace tests | Windows 11 x86_64, rustc 1.99.0 | `cargo test --workspace` | **PASS — 589 tests, 0 failed** |
| Lint | Windows x86_64 | `cargo clippy --workspace --all-targets` | **PASS — 0 warnings** |
| Format | Windows x86_64 | `cargo fmt --all -- --check` | **PASS — clean** |
| Extended gate | Windows x86_64 | `scripts/test.sh` (fmt + clippy + tests + a11y + cross-target checks) | **All steps pass**; the script now also runs `glaze-a11y-report` and the x86_64/aarch64 linux compile checks |
| Accessibility audit | Windows x86_64 | `cargo run -p glaze-a11y --bin glaze-a11y-report` | **PASS — 28/28 contrast, 12/12 focus** |
| **Linux x86_64 compile** | `x86_64-unknown-linux-gnu` | `cargo check --workspace --all-targets --target x86_64-unknown-linux-gnu` | **PASS** |
| **Linux x86_64 lint** | `x86_64-unknown-linux-gnu` | `cargo clippy --workspace --all-targets --target x86_64-unknown-linux-gnu -- -D warnings` | **PASS — 0 warnings** |
| **Linux aarch64 compile** | `aarch64-unknown-linux-gnu` | `cargo check --workspace --all-targets --target aarch64-unknown-linux-gnu` | **PASS** |
| Smithay compositor backend | `x86_64-unknown-linux-gnu` | `cargo check -p glaze-compositor --target x86_64-unknown-linux-gnu` | **PASS — compiles vs Smithay 0.7** |
| **GTK apps type-check** | `x86_64-unknown-linux-gnu` + `aarch64-unknown-linux-gnu` | `scripts/typecheck-apps.sh` (stub `pkg-config`; `cargo check` + `clippy -D warnings`) | **PASS — 0 errors, 0 warnings, both apps, both targets** |
| GTK runtime | Fedora/Wayland | `cd apps/glaze-shell && cargo build && ./glaze-shell` | **NOT RUN — real GTK4/layer-shell/Wayland needed to *run*; compiles clean** |
| Compositor runtime | Fedora/Wayland | `glaze-compositor` | **NOT RUN — never executed** (`run()` validates `XDG_RUNTIME_DIR` then hands to the Smithay backend; no display has ever actually been created) |
| Fedora x86_64 / aarch64 runtime | Fedora / Asahi | `scripts/run-nested.sh` | **NOT RUN — no Fedora host here** |

The Linux/aarch64 rows are **compile/lint only** (no linking, no execution): they
prove the Unix-only code compiles, not that it runs. No Linux *runtime* claim is
made anywhere in this document.

## Test inventory (Windows x86_64) - 589 total

| Crate | Tests | Crate | Tests |
|---|---|---|---|
| `glaze-common` | 26 | `glaze-state` | 26 |
| `glaze-service` | 66 | `glaze-i18n` | 33 |
| `glaze-backend` | 22 | `glaze-cache` | 20 |
| `glaze-compositor` | 28 | `glaze-a11y` | 18 |
| `glaze-shell-core` | 118 | `glaze-desktop` | 58 |
| `glaze-session` | 13 | `glaze-wm` | 61 |
| `glaze-cli` | 24 | `glaze-power` | 36 |
| `glaze-integration-tests` | 40 | **Total** | **589** |

## Working features (verified on this host, with evidence)

**Design & configuration**
- Theme tokens `Glaze Clay` (default) / `Glaze Classic`, light + dark → GTK4 CSS
  (`glaze-common::theme`); colour maths with WCAG (`glaze-common::color`).
- Versioned configuration + migration (`glaze-common::config`).
- Accessibility audit computing real ratios from the live palette
  (`glaze-a11y`, `glaze-a11y-report`) — **0 failures**.

**Application launching**
- Real XDG `.desktop` registry (`Hidden`/`NoDisplay`/`OnlyShowIn`/`TryExec`/dedupe).
- Desktop-entry parser + **shell-free** `Exec` tokenisation with field codes
  (a security test proves `; | > $(...)` backticks stay inert argv).
- Deterministic launcher search ranking; safe detached launch.
- `glaze` CLI: `apps/search/launch/exec-line/status/tray/notifications/config
  get|set/ipc`. `tray` and `notifications` send the `ListTray`/`ListNotifications`
  requests and render with the new table helpers; off Unix they report the honest
  "control socket is Unix-only" error.
- **Notification list UI** — the panel badge is now a `MenuButton` whose popover
  lists the notification history (newest first, summary + sending app, unread
  marked) from `StatusSnapshot.notifications`, with a **Mark all read** action
  that issues `MarkNotificationsRead` (a fresh socket connection per click, so it
  cannot go stale). Empty history says so instead of showing a blank popover.
- **Adversarial edge-case suite** — `crates/glaze-integration-tests/tests/edge_cases.rs`
  (15 tests) probes degenerate inputs across the portable models: 0/1-window
  tiling and gaps larger than the screen, non-numeric master ratios, 0×0 and
  negative work areas for every snap zone, empty/duplicate/malformed keybindings,
  a 0- or negative-area icon grid, regex metacharacters as a search query, empty
  registry and `limit = 0`, launcher navigation on an empty list, `Exec` with
  stray `%`/single quotes/escaped quotes/lone backslash, out-of-range clock parts,
  every locale key, every theme × scheme, and a window with no `app_id`. **All
  pass** — no defects found by this sweep.
- **Tray / notification status feed** — `glaze-common::ipc` gained `ListTray`,
  `ListNotifications`, `MarkNotificationsRead` and `Response::Tray` /
  `Response::Notifications { items, unread_ids, dnd }` with wire types
  (`TrayItemInfo`, `NotificationInfo`). The service's `NotificationServer` now
  tracks **read state** (`unread_ids`/`mark_read`/`mark_all_read`;
  re-notification marks unread again), `TrayRegistry::upsert` is the watcher's
  path, and `glaze-shell-core::status_feed::poll_status` turns both replies into
  the shell's `TrayModel` + `NotificationBadge` — **round-trip tested through the
  real `IpcServer::handle`** via `InProcessClient` (8 tests, including a
  `TrayWatcher` signal → shell-model test). The shell polls it every 2 s and
  refreshes the tray/badge widgets in place (`panel_ui::apply_status`).
- **`glaze-service::tray_watcher`** — the portable StatusNotifier signal state
  machine: `SniSignal` (RegisterItem/UnregisterItem/NewTitle/NewIcon/NewStatus/
  NewCategory/RegisterHost/UnregisterHost) applied to a `TrayRegistry`, with
  `WatcherOutcome` (Registered/Removed/Updated/HostRegistered/HostRemoved/
  Ignored). A metadata signal for an unknown id is **ignored, never invented**;
  updates mutate in place so icon order never jumps (10 tests). `ServiceState`
  owns the watcher and exposes `apply_tray_signal`.
  `tray_watcher::item_signal` maps the **item-side** signals
  (`NewTitle`/`NewIcon`/`NewStatus`/`NewCategory`) onto watcher signals, with an
  end-to-end test that a signal string updates a registered item. `NewToolTip` is
  deliberately **unmapped** (it carries a structured tuple; using only its title
  would be lossy). The D-Bus adapter now **subscribes**: `RegisterStatusNotifierItem`
  spawns a per-item listener on `org.kde.StatusNotifierItem` and feeds every
  signal through `item_signal` into the shared watcher, so a live item's icon
  stays current. Path-style registrations are now resolved to the caller's
  **unique name** from the message header (per spec) instead of being used
  verbatim. Compile-verified for both Linux targets; never run.
- **StatusNotifier wire parsing fixed** — `TrayStatus`/`TrayCategory::from_wire`
  are now case-insensitive: the spec sends PascalCase (`"NeedsAttention"`) but
  some AppIndicator implementations send lower-case, and the strict match
  silently mapped a real attention request to `Active` (found by the new tests).
- **`glaze-compositor::input`** — `InputRouter`, the seam the Smithay input
  backend will call: it holds `Keymap::default_glaze()` + a tiling `Layout` and
  applies matched keys through `wm_actions`, returning `None` for unbound keys so
  events are forwarded rather than swallowed (5 tests, incl. that the keymap
  matches the shell's). `handlers/seat.rs` documents the call site.
- **D-Bus adapters (compile-verified, never run)** — `glaze-service --features
  dbus` compiles and lints clean for `x86_64-unknown-linux-gnu` **and**
  `aarch64-unknown-linux-gnu`, **including the binary**. Real defect fixed: the
  two exported objects (`org.freedesktop.Notifications`,
  `org.kde.StatusNotifierWatcher`) previously held *private copies* of the
  notification/tray models, so anything arriving on the bus would never have
  reached `ListNotifications`/`ListTray`. They now share the **same**
  `Arc<Mutex<ServiceState>>` the IPC socket serves, the `Notify` `urgency` hint
  is mapped (spec default on any read failure, never dropped), and
  `glaze-service --serve --features dbus` actually claims and serves both names
  (keeping the connections alive for the serve loop).
- Registry cache with mtime invalidation and corrupt-cache fallback.
- Launcher state: recents + frecency `0.5^(age/half_life)+ln(count)`.
- Localisation: locale detection/chain + en/de/es/fr catalogues.

**Shell behaviour (GTK-free, tested)**
- Launcher state machine (open/close/query/select/escape, wrapping navigation).
- Clock formatting (12/24 h, seconds, date tokens), panel layout in §6.1 order.
- Taskbar merging: pinned-vs-running/focused with unmistakable flags.
- Tray model + overflow, notification badge (`99+` cap, DND), honest backend
  summary, and a `ControlClient` with an in-process impl (testable) + Unix-socket impl.

**Desktop & window management**
- Desktop surface: icon grid, real-entry icons + launch argv, single/ctrl/shift
  selection, desktop/icon context menus, persistent shortcuts.
- Window management: snapping zones + corner detection, workspace model that
  never invents workspaces, MasterStack/floating tiling with exact coverage,
  keybinding parse/match + default Glaze keymap, alt-tab MRU switcher.
- Session actions: 7 power/session actions with a real confirmation state
  machine for destructive ones and shell-free argv (`systemctl`/`loginctl`).

**Compositor & integration**
- `glaze-backend`: Sway/i3 IPC framing + `GET_TREE` parsing + foreign-toplevel
  registry; honest `Unsupported` for the un-wired live client.
- `glaze-compositor`: Smithay 0.7 bring-up (display, calloop loop, socket, seat,
  output, xdg-shell/layer-shell/decoration/foreign-toplevel handlers) — compiles,
  **never executed**; no milestone marked `Working`. Plus `wm_actions`, which
  applies `glaze-wm`'s snapping/tiling/keymap policy to the live toplevel set
  (8 tests assert real geometry).
- `glaze-service`: portable IPC dispatch, notifications + tray models with the
  single-owner rule for `org.freedesktop.Notifications`, off-by-default `dbus` feature.

**GTK apps (type-checked for x86_64 + aarch64; never run)**
- `apps/glaze-shell`: layer-shell panel rendered from the tested `PanelView`
  (taskbar, tray overflow, badge, clock), real launcher, i18n labels, compositor
  backend attach; a **desktop surface** (wallpaper + `glaze-desktop` icons via the
  shared grid, with `glaze-desktop` context menus) on a layer-shell background
  window; and `glaze-wm`'s keymap installed as panel keybindings.
- `apps/glaze-settings` (agent D, rewritten 2026-11-12; details `docs/settings.md`):
  12 pages in Control Panel categories (Appearance, Wallpaper, Animation,
  Taskbar, Start menu, Windows, Displays, Sound, Network, Power, Default apps,
  About), search with jump-to-setting, undo, reset section, theme import and
  export, and `--get/--set/--unset` on the command line. **Runs in the live VM
  session**: Start menu, then Glaze Settings, opens the window; the old binary
  panicked at startup ("GTK has not been initialized"). Evidence in
  `docs/screens/settings/`: every page in light, dark and Classic (`pages/`),
  live QMP shots (`live-*.png`), driven interactions (`interactions/`).
  Verified in the VM: smoke test `apps/glaze-settings/scripts/vm-smoke.sh` (window
  maps, process stays alive). Real pointer and keyboard input in a headless Sway
  (`scripts/vm-input.sh` plus the `tools/vptr` virtual pointer): wallpaper pick
  writes `desktop.wallpaper_set`, Undo, search "clock" then Enter jumps to the
  row, Dark restyles the window live, an invalid accent hex shows an inline
  error and is not written, Reset section confirms and makes a backup, a broken
  `config.toml` shows the read-only banner. Live session: clicking Kiln in
  Settings changes the desktop wallpaper with no restart (shell log
  `wallpaper (live): .../kiln-dark-landscape-1920x1080.png`), and Undo restores
  paper-hills. A second launch raises the open window, also from the
  scratchpad. With no display it exits 1 with a message. Own window controls,
  because Sway ignores GTK minimise and maximise: verified live, minimise goes
  to the scratchpad, the taskbar button restores (shell log `task
  app:org.glaze.Settings: Restore`), maximise fills 1280x766 above the taskbar
  and restore brings it back, close exits (`live-08..12b`). Colour scheme
  switches other apps through `glaze-colour-scheme`: verified live, Light/Dark
  from Settings flips gsettings and `~/.config/glaze/colour-scheme`, Calculator
  (GTK 4) follows live, already-open Thunar (GTK 3) and VLC (Qt) need a restart
  or the next login (`colour-scheme/`). Gates: 58 portable tests pass and fmt is
  clean (macOS);
  clippy `-D warnings` exits 0 (VM, GTK). Not verified: Displays Apply (not
  pressed in the live session), Sound (no PipeWire in the headless test),
  GUI file dialogs for import/export (the CLI equivalents are tested). Keys
  without a schema home are kept in `settings.toml` and marked "saved only";
  the requests are in `docs/settings-needs.md`.

**Docs, assets, packaging, CI**
- `docs/architecture.md`, `docs/visual-spec.md`, `docs/testing.md`,
  `docs/user-guide.md`, `docs/contributing.md`, 6 man pages, ADR-0001…0005.
- `themes/*.toml` (byte-exact vs `theme.rs`), original `assets/` mark + 8 SVG icons.
- `packaging/fedora/{glaze.spec,glaze-session.desktop}` (opt-in; never replaces the
  user's session), bash+zsh completions, `portable`+`fedora` CI workflow.
- **Session restore** — `glaze-session::restore` (`RunningApps` + `restore_plan`):
  persists the last running desktop ids to `$XDG_STATE_HOME/glaze/session.json` and
  plans **only applications that are still installed** (unknown ids dropped,
  duplicates collapsed, capped at 8, order preserved); a missing/corrupt file
  yields an empty plan so login is never blocked. The shell honours
  `session.restore_apps` on start and records the running set on exit — **only
  when a compositor backend reported windows**, so it never invents a list
  (7 tests).
- **Session install / uninstall (no root)** — `scripts/install-session.sh`
  installs the binaries, the `Glaze` Wayland session entry (absolute `Exec`), a
  Settings app entry, themes, assets, man pages and completions under `~/.local`
  (`--prefix` to change), writing a **manifest** of every file; `--uninstall`
  refuses to run without one, so it cannot delete anything it did not install. It
  never edits the login manager, the default session or the bootloader. Verified
  here: `bash -n` clean, `--help` correct, and both safety paths behave (no
  manifest → refuses; missing binaries → refuses with build guidance, writes
  nothing).
- `glaze-session`: Wayland session env + component supervisor.

## Real defects found and fixed during integration

1. **WCAG AA palette gap** — `danger`/`success` failed AA on raised surfaces
   (worst: Classic-dark `danger` 2.33:1). Fixed in `glaze-common::theme`
   (Clay-light darkened, Classic-dark lightened). The audit is now a hard gate.
2. **`ipc_server` borrow error** — `let guard` used as `&mut`; invisible because
   the module was `#[cfg(unix)]`.
3. **`dbus.rs` wrong zbus API** — `RequestNameFlags::DO_NOT_QUEUE` →
   `DoNotQueue.into()` (verified against vendored zbus 5.19).
4. **`glaze-service` bin missing `Backend` trait import** (Unix-only path).
5. **`fn_to_numeric_cast` lints** in signal handlers — replaced with a true
   `extern "C" fn(i32)` pointer type.

Defects 1–5 were all caught by **cross-checking the Linux target from this
Windows host** — the mechanism ADR-0005 prescribes.

## Open items / blockers (updated in M0)

- **Taskbar for daily use** (the user's top priority) still lacks: click
  behaviour verified on screen (focus, minimise-to-taskbar, restore; Sway has
  no real minimise, so this needs scratchpad tricks or the Glaze compositor),
  pinned apps in the default config, window grouping UI (groups exist in
  `glaze-shell-core::tasks` but a group shows only one button and focuses the
  first window), previews, per-output taskbars (multi-monitor), app icons for
  windows whose `app_id` is not a `.desktop` id (foot falls back to a generic
  icon), an event-driven update instead of 750 ms polling, the tray (no SNI
  watcher running) and notifications (D-Bus server never exercised).
- **Compositor is a protocol server only** — no renderer, no input (M3/M4).
- **Never run against a live bus:** the D-Bus notification server, SNI watcher
  and item subscription (`--features dbus`).
- **Tiling toggle** (required by `docs/decisions.md`): only per window
  (`$mod+Shift+space`) today.
- `packaging/fedora/` is legacy; Debian packaging is M7.

## Next 3 actions

1. M1 design system v2 (`docs/overhaul-plan.md` §5.2): hard offset sheet
   shadows, ink borders, OFL fonts, `glaze-paper` widgets, gallery screenshots.
2. M2 taskbar work: verify clicks with `swaymsg -t get_tree` focus checks,
   minimise/restore, grouping menu, pinned defaults, Sway IPC event subscription.
3. Fix the `launch` zombie (reap or double-fork) and the stale session-restore
   record.

## Architecture decisions

- `docs/adr/ADR-0001-architecture.md` — language, portability boundary, layout.
- `docs/adr/ADR-0002-compositor-strategy.md` — staged Sway → Smithay strategy.
- `docs/adr/ADR-0003-native-compositor.md` — native compositor roadmap (M2–M6).
- `docs/adr/ADR-0004-system-integration.md` — D-Bus/IPC/backend architecture.
- `docs/adr/ADR-0005-portability-and-verification.md` — what is verified where.

## Parallel workstreams — 15 complete

Wave 1 (11): Shell · Compositor · Design/settings · System integration · QA ·
CLI · Launcher state · Localisation · Registry cache · Accessibility ·
Docs/man/completions.
Wave 2 (4): Window management (`glaze-wm`) · Desktop surface (`glaze-desktop`) ·
Session actions (`glaze-power`) · Shell taskbar/tray integration.

## Recent changes

- 2026-11-11 (iter 14) — **Notification list UI + adversarial edge-case suite.**
  `StatusSnapshot` now carries the notification history; the panel badge is a
  `MenuButton` popover listing notifications (unread marked) with a working
  "Mark all read" (`MarkNotificationsRead`). Added a 15-test edge-case "bug hunt"
  across the portable models (degenerate geometry, malformed bindings, regex-like
  queries, odd `Exec` lines, out-of-range clock parts) — all pass, no defects
  found. Also fixed two real code defects found by the toolchain: a `&Vec::clone`
  and a variable shadowing bug in `status_feed::snapshot_from` that had silently
  fed notification items into the tray conversion. Gate: workspace **589 tests**,
  fmt/clippy clean, Linux clippy clean, aarch64 compiles, `--features dbus` clean,
  both GTK apps clean for x86_64 **and** aarch64.
- 2026-11-11 (iter 13) — **Session restore + a real no-root install path.**
  Implemented `glaze-session::restore` (7 tests) and wired it into the shell, so
  `session.restore_apps` — which the settings app already exposed but nothing
  honoured — now records the running set on exit and replays only still-installed
  apps on login (never inventing a list when no backend reported windows).
  Turned `install-session.sh` into a real user-level install/uninstall with an
  exact-removal manifest, and completed `glaze.spec` (the `glaze` CLI, man pages,
  completions, Settings app entry, desktop-database `%post`/`%postun`). Gate:
  workspace **573 tests**, fmt/clippy clean, Linux clippy clean, aarch64 compiles,
  `--features dbus` clean, both GTK apps clean for x86_64 **and** aarch64, all
  shell scripts pass `bash -n`.
- 2026-11-11 (iter 12) — **D-Bus item-signal subscription.** Registering a
  StatusNotifierItem now spawns a per-item listener
  (`zbus::blocking::Proxy::receive_all_signals` on `org.kde.StatusNotifierItem`)
  that maps each signal through `tray_watcher::item_signal` into the shared
  `ServiceState`, so a live item's title/icon/status/category stay current
  instead of freezing at registration. `RegisterStatusNotifierItem` also takes
  `#[zbus(header)]` now, so path-style registrations resolve to the caller's
  unique name as the spec requires. Compile-verified (lib + bin + tests) for
  x86_64 **and** aarch64 with `--features dbus`; never run. Gate: workspace
  **566 tests**, fmt/clippy clean, Linux clippy clean, aarch64 compiles, both GTK
  apps clean for x86_64 **and** aarch64.
- 2026-11-11 (iter 11) — **Item-side SNI signal mapping + settings parity.**
  `tray_watcher::item_signal` maps `NewTitle`/`NewIcon`/`NewStatus`/`NewCategory`
  onto watcher signals (with an end-to-end test), and `NewToolTip` is
  deliberately unmapped as lossy. Found and fixed a real wire-parsing bug:
  `TrayStatus`/`TrayCategory::from_wire` were case-sensitive against the spec's
  PascalCase, so a lower-case `needsattention` from a real item was silently
  treated as `Active` (the attention highlight would never have shown). Completed
  settings parity with the panel **date format** field. Gate: workspace
  **566 tests**, clippy/fmt clean, Linux clippy clean, aarch64 compiles,
  `glaze-service --features dbus` clippy-clean, both GTK apps clean for x86_64
  **and** aarch64.
- 2026-11-11 (iter 10) — **StatusNotifier watcher + compositor input seam.**
  Added `glaze-service::tray_watcher` (portable SNI state machine → `TrayRegistry`,
  7 tests; unknown-id updates ignored, in-place updates so order never jumps) and
  wired it into `ServiceState` (`apply_tray_signal`, `tray_mut`); the
  shell's status feed now has an end-to-end test from a watcher signal to the
  shell model. Added `glaze-compositor::input::InputRouter` (5 tests) — the seam
  the Smithay input backend will call to apply the shared keymap through
  `wm_actions`, returning `None` for unbound keys. **Fixed a real D-Bus defect**:
  the exported Notifications/Watcher objects now share the service state the IPC
  serves (they had private copies, so bus events could never reach the shell),
  mapped the `urgency` hint, and wired the adapters into
  `glaze-service --serve --features dbus` so the feature is no longer dead code.
  Also fixed a split doc comment and a `BTreeMap`-ordering assumption found by
  the tests.
  Gate: workspace **563 tests**, clippy/fmt clean, Linux clippy clean, aarch64
  compiles, `glaze-service --features dbus` (lib **and** bin) clippy-clean for
  both targets, and both GTK apps `check` + `clippy -D warnings` clean for
  x86_64 **and** aarch64.
- 2026-11-11 (iter 9) — **Tray/notification feed over IPC + CLI subcommands.**
  Added `ListTray`/`ListNotifications`/`MarkNotificationsRead` and the
  `Tray`/`Notifications` replies with stable wire types; the service now tracks
  notification read state and exposes tray upsert; `glaze-shell-core::status_feed`
  converts both replies into the shell's tray model and badge and is round-trip
  tested against the real `IpcServer::handle`; the shell polls every 2 s and
  updates the tray/badge widgets in place; `glaze-cli` gained `glaze tray` and
  `glaze notifications`. Verified: workspace **550 tests**, clippy/fmt clean,
  Linux clippy clean, aarch64 compiles, and both GTK apps `check` + `clippy -D
  warnings` clean for x86_64 **and** aarch64.
- 2026-11-11 (iter 8) — **Compositor-side action application + settings parity.**
  New `crates/glaze-compositor/src/wm_actions.rs` (portable, 8 tests) applies
  `glaze-wm` policy to the live toplevel set: `Snap`/`ToggleMaximize` use the
  shared `snap_zone` geometry, `ToggleTiling` flips tiling and re-tiles, and
  workspace actions return an explicit `Ignored { "single workspace in
  milestone 1" }` rather than moving windows the model cannot publish; `resolve`
  maps keys through `Keymap::default_glaze`. Added registry `set_placement` /
  `set_floating`. `glaze-settings` gained a **Launcher & Session** page (search
  limit, recent apps, pinned-app editor, compositor backend, restore/debug,
  desktop icons) for full `GlazeConfig` parity; `switch_row` is now shared with
  the panel page. Verified: workspace **542 tests**, clippy/fmt clean, Linux
  clippy clean, aarch64 compiles, and both GTK apps `clippy -D warnings` clean
  for x86_64 **and** aarch64.
- 2026-11-11 (iter 7) — **`glaze-desktop` and `glaze-wm` are now bound into the
  GTK layer.** New `apps/glaze-shell/src/desktop_ui.rs` renders a real desktop
  surface: wallpaper (`GtkPicture`, cover) or the theme canvas, icons from
  `glaze_desktop::icons::build_icons` positioned by `grid::grid_positions`, a
  layer-shell **background** window (with a windowed fallback), double-click
  activation via `resolve_target` (shell-free argv), and a right-click context
  menu built from `context_menu::{desktop_menu,icon_menu}` with `glaze-i18n`
  labels and danger styling. New `keybindings.rs` maps GDK key/modifier events to
  `glaze_wm::keybindings::default_glaze()` and installs the keymap on the panel,
  reporting honestly that system-wide shortcuts and window moves belong to the
  compositor. Verified: both apps `cargo check` + `clippy -D warnings` clean for
  x86_64 **and** aarch64 (stub `pkg-config`); workspace **534 tests**, clippy/fmt
  clean, Linux clippy clean, aarch64 compiles.
- 2026-11-11 (iter 6) — **`apps/glaze-shell` now renders from the tested view
  model.** `panel_ui` no longer calls `layout()`/`merge_tasks()` itself: it
  iterates `PanelView.items` and maps `TaskView` → taskbar buttons (pinned /
  running / focused classes), tray overflow and badge from the view, and label
  visibility via `TaskView::show_label_at`. `main.rs` builds one `PanelView` via
  `build_panel_view`. `TaskView` gained `window_ids` (so click-to-focus still
  targets a real window) and `show_label_at`. Verified: shell `cargo check` +
  `clippy -D warnings` clean for x86_64 **and** aarch64 (stub `pkg-config`);
  workspace **534 tests**, clippy/fmt clean, Linux clippy clean, aarch64 compiles.
- 2026-11-11 (iter 5) — **GTK apps now compile-verified.** Since `cargo check`
  never links, a stub `pkg-config` (`scripts/dev/pkg-config-stub` +
  `scripts/typecheck-apps.sh`) lets the compiler type-check both apps for
  x86_64 **and** aarch64 linux. They now pass `cargo check` and
  `cargo clippy -D warnings` with **0 errors / 0 warnings**, which surfaced and
  fixed **9 real defects** that would have broken the Fedora build:
  `glib::source::unix_signal_add_local` (removed in glib 0.22), two borrow/move
  bugs in the shell launcher, a missing `v4_12` feature for
  `CssProvider::load_from_string`, `toggle()` returning a row but used as a
  `Switch`, a move-while-borrowed bug in the settings wallpaper page, and 7
  unused imports. Documented in `docs/testing.md` §1.1.
- 2026-11-11 (iter 4) - Closed the wave-2 integration gaps: added the 11 missing
  `menu.*` keys to `glaze-i18n` (en/de/es/fr) plus a stronger test that reads each
  locale file directly; added `glaze-shell-core::view`, a render-ready panel view
  model composing layout + taskbar merge + tray + badge + clock (15 tests);
  aligned both GTK apps on **one** gtk-rs generation (gtk4 0.11 / glib 0.22 /
  gtk4-layer-shell 0.8), verified against the vendored crate sources; and
  extended the standing gate (`scripts/test.sh`, CI) with the a11y report and the
  x86_64/aarch64 cross-target checks. **533 tests**, clippy/fmt clean, a11y 0%.
- 2026-11-11 (iter 3) — Merged wave 2. Workspace harness grew to **517 tests**
  across 15 crates; added the Linux x86_64/aarch64 compile + lint cross-checks to
  the standing gate. `glaze-shell-core` reached 93 tests (taskbar/tray/badge/
  status/control-client); new `glaze-wm`, `glaze-desktop`, `glaze-power`.
- 2026-11-11 (iter 2) — Merged wave 1; cross-checks found and fixed 5 real
  defects (1 palette, 3 Linux-only compile errors, 1 lint class); 311 tests.
- 2026-11-10 (iter 1) — Portable workspace, `glaze-common` contract, registry/
  search/launch, session supervisor, compositor crate, README + ADRs.
