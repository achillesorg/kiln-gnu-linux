# Glaze overhaul plan

- **Status:** Proposal (planning only, nothing implemented by this document)
- **Date:** 2026-11-11
- **Scope:** the "full overhaul" the user asked for: a real compositor, a
  redesigned shell in a flat cut-paper look, light and dark themes, bundled
  wallpapers, desktop and touch form factors, and Glaze Classic (Win2K
  structure) as a heritage theme.
- **Target platform (new):** Debian 13 arm64 hobby OS in QEMU (HVF on an Apple
  Silicon host). `GLAZE.md`, `AGENTS.md`, `GOAL.md` and `PROJECT_STATUS.md` still
  describe Fedora and a Windows authoring host. Both are out of date and need
  an ADR (see M0).

Every claim below is marked with how it was checked. **[src]** means I read it
in the repo source. **[vm]** means a read-only command in the VM on 2026-11-11.
**[host]** means a command run on the macOS host. **[unverified]** means
background knowledge I did not check here, to be confirmed before relying on it.

---

## 1. Assessment: what is real, what is scaffold

### 1.1 Evidence collected

| Check | Result |
|---|---|
| `git log` [host] | One commit, `68a631c Initial import`. Everything since is uncommitted. |
| `git status` [host] | 9 modified files, 6 untracked (theme palette, form factor/mobile, wallpapers, session fixes, install fix). These are unreviewed edits from a previous agent. |
| `cargo test --workspace` on macOS with those edits [host] | **619 passed, 0 failed.** |
| VM OS [vm] | `/etc/os-release` says `PRETTY_NAME="KilnLinux (Debian 13)"`, hostname `kilnlinux`, kernel 6.12 arm64, 4 vCPU, 6 GB RAM. The name already in use is neither MorphLinux nor ClayOS. |
| VM GPU [vm] | `virtio-gpu-pci` without virgl. Sway logs `DRI2: failed to create screen` and falls back to `kms_swrast`, so all rendering is software (llvmpipe). `qemu-system-aarch64 -device help` [host] lists no `virtio-gpu-gl` device for this build. |
| VM input [vm] | USB keyboard and USB tablet only. **No touch device.** QEMU 11.0.3 on the host does provide `virtio-multitouch-pci` [host]. |
| Running session [vm] | `glaze-session run --backend sway` runs `sway --unsupported-gpu`, `swaybg`, `swaybar -b bar-0`, `glaze-service --serve` and `glaze-shell`. Launched from `~/.profile` via `dbus-run-session`, with no sway config of its own: `~/.config/sway` is missing, so sway uses `/etc/sway/config`, and its `bar {}` block (line 219) is the stray bar. `~/.config/glaze/sway-session.conf` exists but nothing passes it to sway with `-c`. |
| Shell environment [vm] | `glaze-shell` (pid 824) has `WAYLAND_DISPLAY=wayland-1` but **no `SWAYSOCK`**, and `XDG_SESSION_TYPE=tty`. `shell.log` says `Sway IPC backend unavailable: ... SWAYSOCK is not set` and `foreign-toplevel backend unavailable: ... not linked in this build`. **The taskbar can never show windows today.** |
| Screenshot via `grim` [vm] | Sway's stock tiling layout, the default swaybar at the top, a flat light panel at the bottom with only "Glaze" and a clock. There are no task buttons even though two foot windows are open. The panel colours look like the *old* blue-grey palette, so the installed shell binary appears to predate the uncommitted theme edit (inferred from pixels, not checked against a build id). |
| systemd user bus [vm] | `session.log`: `Activated service 'org.freedesktop.systemd1' failed`. `dbus-update-activation-environment --systemd` fails, so portals and activation env will not work properly under the current login path. |
| `glaze-compositor` headless smoke run [vm] | Run for 3 s with a private `XDG_RUNTIME_DIR`. It binds `wl_compositor, wl_subcompositor, wl_shm, wl_output, zxdg_output_manager_v1, wl_seat, xdg_wm_base, zxdg_decoration_manager_v1, zwlr_layer_shell_v1, ext_foreign_toplevel_list_v1` and logs `glaze-compositor ready (no presentation backend: nothing is drawn)`. It stops cleanly on SIGTERM. **This is the first recorded run of it.** |
| Packages available [vm, `apt-cache policy`] | Installed: GTK 4.18.6, gtk4-layer-shell 1.0.4, sway 1.10.1, libinput 1.28, libseat 0.9.1, mesa 25.0.7, libgbm/libudev/libxkbcommon/pixman dev headers, librsvg2-common, grim 1.4.0, Noto Sans/Serif. Available but not installed: `fonts-inter` 4.1, `squeekboard` 1.43.1, `phosh` 0.46, `labwc` 0.8.3, `wayfire` 0.9.0, `resvg` 0.45.1, `libdisplay-info-dev`. Not found: `niri`, `fonts-ibm-plex`, `fonts-adobe-sourceserif4`. |

### 1.2 Real (works or is exercised at runtime)

- **App registry, search and launch** (`glaze-service`): parses real `.desktop`
  files, shell-free `Exec` tokenisation, serves the control socket. [vm] The
  service is listening at `/run/user/1000/glaze/control.sock`, and `shell.log`
  shows real launches (`launched Foot (foot)`, `launched Vim (vim)`).
- **GTK4 layer-shell panel** (`apps/glaze-shell`): renders a bottom
  exclusive-zone panel with a launcher button and a clock on Sway. [vm screenshot]
- **Theme token → GTK CSS pipeline** (`glaze-common::theme`) with WCAG contrast
  tests (`glaze-a11y`). [src, host tests]
- **Session supervisor** (`glaze-session`): starts and watches children. With the
  uncommitted diff it passes `--serve` and waits for a new `wayland-*` socket
  before starting later components, and it has tests for both. [src] The running
  VM session shows `glaze-service --serve`, so the installed `glaze-session`
  already contains that part. [vm]

### 1.3 Scaffold (compiles and is unit-tested, but does nothing at runtime)

- **`glaze-compositor`**: Smithay 0.7 with only `wayland_frontend`
  (`crates/glaze-compositor/Cargo.toml`). It has no renderer, no
  winit/DRM/udev/libinput/libseat backend, no `Space`, and no input source
  (`handlers/seat.rs`: "No device feeds this seat yet"). It is a protocol
  server that draws nothing (confirmed by the [vm] run above). About 2.3 k
  lines, of which `model_bridge.rs`/`wm_actions.rs` (policy glue) are the
  reusable part.
- **Window-management policy** (`glaze-wm`: snapping, tiling, alt-tab,
  workspaces, keymap): about 2 k lines of tested pure logic. At runtime only the
  keymap is used, and only as GTK shortcuts on the panel. Sway ignores
  everything else.
- **Taskbar/tray/notification models** (`glaze-shell-core`, about 4 k lines):
  well tested, but starved of data at runtime. The Sway backend fails on
  `SWAYSOCK`, the wlr-foreign-toplevel client is "not linked", and the D-Bus
  notification/SNI adapters sit behind an off-by-default `dbus` feature that
  `PROJECT_STATUS.md` says has never talked to a bus. I did not establish
  whether the VM build enabled it.
- **Crates with no runtime consumer** [src, grep of `crate::` paths]:
  `glaze-state` (none), `glaze-power` (none), `glaze-cache` (integration tests
  only), `glaze-a11y` (report binary and tests only).
- **Desktop icons, context menus, mobile layout, wallpaper resolver**
  (`glaze-desktop`, `mobile_ui.rs`, `form_factor.rs`, `wallpaper.rs`): code
  exists, but none of it has been seen on screen. The default config disables
  the desktop surface (`desktop surface: disabled` in `shell.log`).
- **Packaging**: `packaging/fedora/*.spec` targets Fedora. The OS is Debian, so
  it is dead weight until it is replaced by `.deb` or the custom image build.

**Honest ratio.** The repo has about 30 k lines of Rust and 619 tests, but the
runtime product is a GTK panel with a launcher on top of stock Sway. Test count
has been used as a proxy for progress. The overhaul should measure progress by
**screenshots and runtime checks in the VM**.

### 1.4 Review of the uncommitted edits (keep, fix or discard)

| Edit | Verdict |
|---|---|
| `glaze-session`: `--serve`, wait for Wayland socket, export `WAYLAND_DISPLAY` | **Keep.** Correct and tested. Still missing: export `SWAYSOCK` (glob `$XDG_RUNTIME_DIR/sway-ipc.$UID.<pid>.sock` for the sway child's pid), set `XDG_SESSION_TYPE=wayland`, and pass a Glaze-owned sway config with `-c`. |
| `install-session.sh`: `mkdir -p` the manifest dir, install wallpapers | **Keep.** Fixes the "fails if `~/.local/share/glaze` is missing" bug. |
| `theme.rs` cream/terracotta palette | **Keep the palette, change the depth.** It still uses blurred shadows (`0 4px 18px`, `0 3px 12px` at `theme.rs:388-396`), which contradicts the reference look (§3.1). Replace them with hard offset "paper sheet" shadows. |
| `wallpaper.rs`, `assets/wallpapers/*.svg`, `gen-wallpapers.py` | **Keep. These are the visual baseline the user likes.** Keep the palette, the layering and the generator, and don't edit or delete them during planning. Evolve them in M1 (§7.1). Two things conflict with the cut-paper reference and need changing: soft blurred shadows between layers (6 `filter`/blur references in `paper-hills-light-desktop.svg`), and the dark "terracotta waves" set has no terracotta (rendered thumbnail on the host). |
| `form_factor.rs`, `mobile.rs`, `mobile_ui.rs` | **Keep as a starting point.** The pure logic is fine. The GTK mobile UI is unseen, and gestures belong in the compositor (§6). |

---

## 2. Compositor decision

### 2.1 Requirements that drive the choice

1. **Win2K stacking windows by default**, with title bars that Glaze draws
   (paper style, min/max/close), not a tiling layout.
2. **Paper-style window frames**: hard offset sheet shadows, ink edge, light and
   dark, Classic variant.
3. **Motion** (open/close/minimize/overview) with reduced-motion support.
4. **Touch**: edge-swipe gestures, rotation, on-screen keyboard protocols.
5. **Must run on llvmpipe** in the VM, since there is no GPU acceleration.
6. Rust, licence compatible with MIT/Apache, agent-maintainable.

### 2.2 Options

| Option | Fit | Cost | Notes |
|---|---|---|---|
| **A. Keep Sway** (configure floating, own config, hide bar) | Low. No themeable title bars beyond colours and border width, no rounded or paper frames, no animations, no touchscreen edge gestures (`bindgesture` is touchpad-only [unverified]). | Very low | Fine as a **dev fallback and test host**, not as the product. |
| **B. Switch to labwc** (stacking, Openbox-style, in Debian 0.8.3 [vm]) | Medium. Stacking matches Win2K, and Openbox-style themes can colour title bars. Animations and gestures are out of reach without patching C. | Low | Fastest path to "looks like a desktop". The ceiling is low, and it is not Rust. |
| **C. Fork niri or cosmic-comp** (Rust/Smithay) | Medium. Mature and fast, but niri's scrolling-tiling paradigm and cosmic's protocol stack fight the Win2K model. Both are GPL-3.0 [unverified], which would force Glaze off MIT/Apache. niri is not packaged in Debian 13 [vm]. | Medium entry, high divergence cost | Better used as **read-only reference code** than forked. |
| **D. Real Smithay compositor (`glaze-compositor`)**, built on the structure of Smithay's MIT-licensed `anvil` example | High. Glaze owns frames, motion, gestures and touch. Smithay ships the winit, udev/DRM/GBM, libinput, libseat, GLES and pixman pieces needed [unverified for 0.7 feature names; check at M3]. | High: the largest single workstream | The existing crate already has protocol handlers and the policy bridge. It needs backends, rendering, a `Space`/stacking model, input routing and SSD drawing. |

### 2.3 Recommendation: **D, staged, with Sway kept as fallback**

- **M0–M2** run the shell on Sway, made decent with a Glaze sway config:
  floating by default, no bar, Glaze colours, wallpaper. The shell restyle can
  ship and be screenshotted immediately.
- The shell talks to the compositor **only through standard protocols** plus
  the existing Glaze control socket: `wlr-layer-shell`,
  `wlr-foreign-toplevel-management` (or `ext-foreign-toplevel-list` plus a Glaze
  activation request), `ext-session-lock`, `ext-workspace`. The same shell then
  runs on Sway and on `glaze-compositor`, so the compositor can be swapped in
  without touching UI code (this preserves ADR-0002).
- **M3–M5** make `glaze-compositor` real: first nested (winit inside Sway in the
  VM), then on tty1 via DRM/libseat. `glaze-session` falls back to Sway if the
  compositor fails to produce a socket.
- Rendering uses the **GLES renderer on llvmpipe**. The flat cut-paper look is
  cheap: solid fills plus an offset solid rectangle, no Gaussian blur passes.
  That is a performance argument *for* this art direction in a GPU-less VM.
  Keep a pixman fallback in mind if EGL on `kms_swrast` misbehaves under
  Smithay [unverified].

Rejected alternatives and why: A has too low a ceiling to be the product. B is
a dead end for gestures and motion. C brings a licence change and paradigm
mismatch.

---

## 3. Shell UI toolkit decision

### 3.1 The visual target (from the user's reference image)

The reference is a flat terracotta field (about `#D97757`) with **two off-white
paper sheets stacked with a small hard offset**, **thick black hand-drawn ink
line art** with rounded caps and slightly wobbly strokes, flat fills, no blur,
no gradient, no glossy shadow, and lots of empty space. Sheet corners are
nearly square.

"Clay" in Glaze therefore means **cut paper plus ink**, not inflated 3D. Design
rules that follow:

| Concern | Rule |
|---|---|
| Depth | **Hard offset sheet**, e.g. `box-shadow: 4px 4px 0 0 @glaze_sheet_under` (zero blur). At most one sheet under a surface, and only on floating things: menus, launcher, windows, notifications. The panel is flat, with a 1 px ink or rule line. |
| Edges | 1–2 px ink border (`#1F1E1C`-ish in light, warm cream in dark) on raised paper. Radius small, **4–8 px**, not 12–20: paper, not pillows. Classic keeps 2–3 px with bevels. |
| Colour | Cream paper `#FAF9F5`/`#F3EFE6`, ink `#1F1E1C`/`#2B2926`, terracotta `#D97757` as **field and accent fill** (large areas: start button, selected item, wallpaper) and a darker `#B5502C` where white text must pass WCAG AA (already in the uncommitted palette). Dark scheme: warm charcoal paper, cream ink, the same terracotta. |
| Type | Serif display (headings, start menu title, clock, lock screen) plus sans UI (OFL fonts, §7.3). |
| Illustration | Hand-drawn ink strokes, organic wobble, rounded caps, flat fills. Used in wallpapers, empty states, onboarding and the lock screen, **not** on every control. |
| Icons | Ink-outline glyphs with a consistent ~1.75 px stroke at 16/24 px, rounded caps, optional flat terracotta or cream fill. Hand-drawn wobble only at large sizes (48 px and up); small sizes stay geometric for legibility. |
| Shapes | Geometric, distinctive cut-outs: circles, squares, arches, blobs, odd notched or bitten shapes. Pieces are layered with slight offsets and small rotations (±0.5–2°), have hard edges and subtle paper grain, and use no blur or gradients. In the UI this shows in restrained places only: the start button as a terracotta circle or arch, the selected item as a slightly rotated paper tab, notification cards as offset sheets, empty-state art. Controls stay axis-aligned and legible. |
| Motion | **Stop-motion** (see §3.3). Paper pieces step into place rather than glide. No elastic bounce, no blur, no cross-fade smear. |

### 3.3 Stop-motion motion

The cut-paper reference implies animation like stop-motion paper puppetry:
discrete poses held for a few frames, not 60 fps easing.

- **Stepped timeline.** Transitions are sampled at an **animation frame rate of
  about 8–12 fps** (default 12) while the screen still presents at the display
  rate. A 160 ms menu open is about 2 held poses, and a 250 ms window open is 3.
  Easing is **off by default**: poses are linear interpolation of the key
  poses, quantised to the step rate.
- **Per-pose jitter.** Each held pose may add a tiny deterministic offset or
  rotation (≤1 px, ≤0.5°, seeded per surface so it is repeatable in tests). This
  gives the hand-placed feel. It settles to exactly zero at rest, so nothing is
  ever drawn crooked when idle.
- **Where it applies.**
  - Window open: sheet drops in from a 3–4 px offset.
  - Window close/minimise: sheet lifts and slides toward its task button in 3 poses.
  - Menus and launcher: unfold from the invoking button in 2–3 poses.
  - Hover/press: sheet offset 2→4 px in a single step.
  - Workspace switch: a paper slide in 3–4 poses.
  - Mobile: home swipe and quick-settings sheet. While a finger is down, motion
    **tracks the finger continuously**, and only the release animation is stepped.
- **Implementation.** One `StepClock` in `glaze-design` (step rate, pose count,
  jitter seed), used by both the compositor (window transitions, rendered per
  pose with damage only on pose changes, which is also cheaper on llvmpipe) and
  the shell (GTK: a `glib` timeout per pose that swaps CSS classes or widget
  transforms. CSS `steps()` timing on transitions is an option, but whether GTK
  4.18 CSS supports `steps()` is [unverified]).
- **Settings.** `motion.style = stop_motion | smooth | off`,
  `motion.step_fps` (8–24), `motion.jitter` on/off. **Reduced motion** (Glaze
  setting or the GTK/portal preference) forces `off`: instant state changes,
  with at most one 1-step cross-cut for orientation.
- **Tests.** Pure unit tests on `StepClock`: pose count for a duration and
  rate, zero jitter at rest, determinism per seed, reduced-motion yields one
  pose. A VM frame-sequence capture per transition in M5.

**Brand caution.** The reference is an Anthropic illustration. Glaze must use
**original artwork and open fonts** in this *style*. It must not reuse
Anthropic's illustrations, logo, or proprietary typefaces, and nothing should
suggest Glaze is an Anthropic product.

### 3.2 Can each toolkit render this crisply?

| Toolkit | Hard offset sheets | Ink outlines / wobbly strokes | Touch gestures | Layer-shell | VM rendering | Verdict |
|---|---|---|---|---|---|---|
| **GTK4 4.18** (installed [vm]) | Yes: CSS `box-shadow` with 0 blur and `border` are crisp. | Yes: SVG icons via librsvg (installed [vm]), and custom widgets can stroke `GskPath` with round caps (GTK ≥ 4.14 path API [unverified gtk4-rs feature flag, likely `v4_14`]). | `GtkGestureSwipe`/`Drag`/`LongPress`; Phosh is a GTK shell on phones [unverified detail]. | gtk4-layer-shell 1.0.4 installed and **working** [vm]. | Cairo/GL renderers work on llvmpipe (panel renders today [vm]). | **Recommended.** |
| iced (+ third-party layer-shell crate) | Yes, custom styling in Rust. | Via `canvas` paths. | Basic touch; gestures are mostly DIY. | Third-party crate, version churn risk. | tiny-skia software fallback. | Viable, but would rewrite all existing GTK work. Accessibility is weaker [unverified]. |
| Slint | Yes. | Paths are supported. | Good touch story. | Not first-party; needs a custom platform. | Software renderer available. | Licence and layer-shell friction. |
| Draw in the compositor | Full control. | Full control. | Native. | n/a | Same renderer. | Use **only** for window frames, gesture-driven transitions and the overview. Text layout and a11y from scratch is too much for panels and menus. |

**Decision.** GTK4 plus gtk4-layer-shell for all shell surfaces (panel, start
menu, launcher, desktop, notifications, quick settings, mobile home, lock
screen). `glaze-compositor` draws window frames and transition effects itself,
using the same tokens. Avoid libadwaita in the shell: its styling would fight
the paper look. Add a GTK widget kit crate, `glaze-paper` (`PaperSheet`,
`InkButton`, `InkIcon`, `InkRule`, `SerifHeading`), so the look is built once.

---

## 4. Target architecture and crate layout

```text
glaze-session (supervisor; picks backend; fallback glaze -> sway)
   |-- glaze-compositor (Smithay)  or  sway + Glaze config (fallback/dev)
   |       owns: windows, stacking, SSD paper frames, input, touch gestures,
   |             outputs/rotation, lock (ext-session-lock), screencopy
   |-- glaze-service  (registry/search/launch, notifications, SNI watcher,
   |                  power/session actions, settings broker)  [D-Bus + control socket]
   `-- glaze-shell (GTK4)  desktop layout | mobile layout, selected at runtime
           talks: layer-shell, foreign-toplevel, ext-workspace, control socket
```

| Crate | Change |
|---|---|
| `glaze-common` | Keep: config, IPC types, paths. **Move theme out.** |
| `glaze-design` (new, from `glaze-common::theme` + `color` + `glaze-a11y` audit) | One token source: palettes (Clay light/dark, Classic light/dark), metrics, typography, sheet-offset tokens. Emits GTK CSS **and** compositor frame params. Holds the a11y contrast gate. |
| `glaze-paper` (new, `apps/` side, GTK) | Paper widget kit plus ink icon loader. |
| `apps/glaze-shell` | Split into `desktop/` (panel, start menu, taskbar, tray, desktop icons) and `mobile/` (home grid, status bar, quick settings, gesture bar), sharing `glaze-shell-core`. |
| `glaze-shell-core` | Keep the models, drop dead paths. Add a real wlr-foreign-toplevel **client** on its own Wayland connection. |
| `glaze-compositor` | Grow into the real compositor: `backend/{winit,udev}`, `render/` (frames, sheet shadows, transitions), `shell/` (stacking `Space`, focus, SSD), `input/` (keyboard, pointer, touch, gestures), `protocols/`. |
| `glaze-wm` | **Merge into `glaze-compositor`** as its portable `policy` module (snap, stacking, alt-tab, workspaces, **gesture recogniser**), still testable on the host. |
| `glaze-service` | Absorb `glaze-power`. Turn the `dbus` feature **on by default** on Linux. |
| `glaze-state`, `glaze-cache` | Delete, or fold into `glaze-service` when something actually consumes them. |
| `glaze-backend` | Keep the Sway adapter for the fallback session. Move the foreign-toplevel client into `glaze-shell-core`. |
| `glaze-i18n`, `glaze-desktop`, `glaze-cli`, `glaze-session` | Keep. |
| `assets/` | `wallpapers/` (sources plus rendered PNGs), `icons/ink/`, `fonts/` (OFL plus licence files), `illustrations/`. |
| `packaging/` | Replace `fedora/` with `debian/` (or the OS image recipe). |
| `tests/vm/` (new) | Scripts that drive the VM and capture screenshots (§5.1). |

An ADR is required for each of: the Debian pivot, the compositor choice,
GTK-only shell plus compositor-drawn frames, and the crate merges/deletions.
`AGENTS.md` requires ADRs for architecture changes.

---

## 5. Milestones and done-criteria

### 5.1 Verification harness (built in M0, used by every milestone)

- `scripts/vm/sync-build.sh`: rsync the tree to the VM and run
  `cargo build --release` there (the VM copy is not a git repo today [vm]).
- `scripts/vm/shot.sh NAME`: capture with `grim` inside the session when the
  compositor supports screencopy. Otherwise use **QEMU `screendump`** through a
  QMP socket, which works regardless of compositor. That needs `-qmp
  unix:...,server,nowait` added to `$HOME/fedeos/run.sh`, a file outside
  the repo, so the user must approve the change.
- Screenshots are committed under `docs/screens/<milestone>/` with the command
  that produced them. "Done" means a screenshot exists **and** a scripted check
  passed: `swaymsg -t get_tree` or `glaze status` JSON, exit codes, logs.
- Standing gates: `cargo test --workspace`, clippy `-D warnings`, fmt, the
  a11y contrast report, **and** a VM build of the GTK apps (real link, not the
  pkg-config stub).

### 5.2 Milestones

| # | Milestone | Done when (all verifiable in the VM) |
|---|---|---|
| **M0** | **Honest baseline and session fixes.** Review and keep the edits per §1.4. `glaze-session` exports `SWAYSOCK` and `XDG_SESSION_TYPE=wayland` and starts sway with a Glaze config (`-c`): no `bar {}`, floating by default, Glaze border colours, no swaybg. Fix the systemd user bus env (or drop the `--systemd` call). ADR-0006 (Debian pivot, docs rewrite of Fedora/Windows assumptions). Harness §5.1. | After a reboot and autologin: `pgrep swaybar` is empty. The `glaze-shell` environ contains `SWAYSOCK`. Open `foot`, then `glaze status` reports `windows>=1` and the screenshot shows a task button. Tests are green. |
| **M1** | **Design system v2 (cut paper plus ink).** `glaze-design` crate; hard offset sheet shadows; ink borders; OFL fonts installed by `install-session.sh`; `glaze-paper` widgets; first 24 ink icons; Classic re-derived. `glaze-shell --gallery` renders every component in each theme and scheme. Wallpaper generator evolved per §7.1 (hard offsets, grain, shapes, seeds), with the baseline kept alongside. `StepClock` crate API and unit tests. | Four gallery screenshots (Clay light/dark, Classic light/dark). A CSS-token test asserts **no blur radius > 0** in Clay shadows. The a11y report shows 0 failures. A font check confirms the serif and sans families resolve (`fc-match` in the VM). |
| **M2** | **Desktop shell redesign on Sway.** Win2K structure: start menu (categories, search, recent, power), taskbar with live windows (wlr-foreign-toplevel client; click focuses, minimises, restores), tray (SNI over D-Bus), notification popups plus centre, quick settings, clock/calendar with serif clock, desktop icons, wallpaper on the desktop layer. | Screenshots: desktop idle, start menu open, two windows plus taskbar, notification (`notify-send` succeeds and is shown), tray icon from a real SNI app. Scripted: clicking a task button changes the `focused` flag in `swaymsg -t get_tree`. |
| **M3** | **Compositor renders, nested.** Smithay winit backend inside the Sway session; GLES on llvmpipe; xdg-shell stacking `Space`; layer-shell; keyboard and pointer; screencopy. | `glaze-compositor --nested` opens a window inside Sway. `foot` and `glaze-shell` run inside it (`WAYLAND_DISPLAY` points at it). Screenshot taken *through* the compositor's screencopy (`grim` 1.4 needs `wlr-screencopy` [unverified which protocol grim 1.4 supports; check in M3]). Frame-time log shows under 16 ms at 1280×800 on llvmpipe, or the measured number is recorded honestly. |
| **M4** | **Compositor as the session.** udev/DRM/GBM, libinput, libseat on tty1. Glaze paper SSD frames (title, min/max/close, focus state, sheet shadow), force-SSD policy via xdg-decoration. Foreign-toplevel management, workspaces, alt-tab, snapping (from `glaze-wm` policy). `glaze-session --backend glaze` with fallback to Sway. | Autologin boots into `--backend glaze`. Screenshot shows paper frames on foot and a GTK app. Killing the compositor leaves a usable tty and the log says why. On the next login with the compositor binary removed, the session falls back to Sway (logged). |
| **M5** | **Motion and polish.** Stop-motion `StepClock` transitions (§3.3): open/close/minimise-to-taskbar, menu-from-affordance, reduced-motion switch, HiDPI scale 1/1.25/1.5/2, XWayland via `xwayland-satellite` or Smithay XWayland (decide in an ADR). | Short screen recording or frame sequence of each transition. A reduced-motion toggle test shows 0 ms transitions. Screenshots at each scale. |
| **M6** | **Touch and mobile** (§6). | Touch VM profile boots into the mobile layout. Gesture traces pass the recogniser tests. Screenshots: home grid, app open, quick settings pulled down, keyboard shown on a text field, rotated output. |
| **M7** | **Lock, settings, packaging.** `ext-session-lock` lock screen (serif clock, illustration); settings app restyled with `glaze-paper`, including wallpaper picker and form-factor switch; `.deb` packages or an OS image recipe. | The lock screen can't be bypassed by killing the shell: kill `glaze-shell` while locked and the session stays locked (compositor-enforced). The `.deb` installs on a clean VM snapshot. |

Rough sizing, relative only: M0 small, M1 medium, M2 large, M3 large, M4 very
large, M5 medium, M6 large, M7 medium. M3–M4 can run **in parallel** with M1–M2
because the shell only touches standard protocols.

---

## 6. Mobile and touch strategy

- **One shell, two layouts.** `FormFactor::resolve` (exists [src]) picks
  desktop or mobile from output size, orientation and seat capabilities, and
  can be overridden by config or `GLAZE_FORM_FACTOR`. Switching at runtime (a
  convertible docking a keyboard) is a later goal; restart-to-switch is
  acceptable first.
- **The compositor owns gestures.** Edge swipes must start from screen edges,
  where a client cannot see them. A portable `GestureRecognizer` in the
  compositor policy module turns touch traces into `HomeSwipe` (bottom edge
  up), `AppSwitcher` (bottom up and hold), `QuickSettings` (top edge down) and
  `Back` (left/right edge, optional). Policy actions go to the shell over the
  control socket (or a small `glaze-shell-v1` Wayland protocol if latency
  demands it). Recogniser unit tests replay recorded traces on the host.
- **Under Sway (M2), touch is limited** to in-surface GTK gestures. No
  edge gestures until M4/M6. This is stated, not hidden.
- **Mobile UI:** status bar (layer-shell top), home app grid with 48 px or
  larger targets (`MIN_TOUCH_TARGET_PX = 48` exists [src]), bottom gesture
  handle, fullscreen-maximised windows only, quick settings sheet as a paper
  card sliding down, notifications in the same sheet.
- **On-screen keyboard:** `squeekboard` (Debian 1.43.1 [vm]), which needs
  `input-method-v2`, `virtual-keyboard-v1` and `text-input-v3` in the
  compositor [unverified exact protocol set; check squeekboard docs at M6].
  Restyle it with its CSS theme to paper/ink, if its theming allows [unverified].
- **Rotation:** output transform in the compositor, with the shell
  re-laying out on `wl_output` geometry change.
- **Test rig:** a second QEMU profile (`run-touch.sh`, outside the repo, needs
  user approval) with `-device virtio-multitouch-pci` and a portrait virtio-gpu
  mode. Whether `virtio-gpu-pci` `xres`/`yres` sets the initial mode is
  [unverified]. Scripted touch injection: QMP `input-send-event` multitouch
  support is [unverified]. Fallback is a dev-only injection command on the
  compositor's control socket, compiled only in debug builds.
- **Not in scope** until the user asks: real phone hardware, telephony,
  modem, sensors.

---

## 7. Wallpapers, icons, fonts and illustrations

### 7.1 Wallpapers

**Baseline (keep).** `assets/wallpapers/` has paper-hills and terracotta-waves,
each in light/dark × desktop (1920×1080) and mobile (1080×2340), with
`manifest.toml`. They come from the deterministic `scripts/gen-wallpapers.py`
and are resolved by `wallpaper.rs`. The user likes them. They stay as shipped
until the evolved generator produces replacements the user approves. Old
outputs are kept side by side, not overwritten, until then.

**Evolution toward the stop-motion cut-paper reference:**

1. **Shadows: blur to hard offset edges.** Replace each blurred inter-layer
   shadow with a **duplicate of the layer's path, filled with a darker tone of
   the layer below and offset by 3–6 px** (down-right, consistent light
   direction), drawn under the layer. Optionally add a 1 px lighter "cut edge"
   highlight on the top contour. No `<filter>`, no `feGaussianBlur`, no
   gradients. This keeps the hills' palette and layering exactly and makes
   every edge hard.
2. **Paper grain.** One small tileable grain texture (generated noise, 2–4 %
   luminance variation) multiplied over the whole image at render time, the
   same tile for all sets. Generated by the pipeline, not a photographic scan,
   so the licence stays clean. Because SVG noise would need a filter, grain is
   applied in the **PNG render step**, not in the SVG source.
3. **Shape language.** Add geometric, distinctive cut-outs to the hills and
   waves:
   - circles (sun/moon discs, a few dots);
   - squares and rectangles, slightly rotated;
   - arches (half-pill shapes, like doorways);
   - soft blobs (low-frequency closed curves);
   - "odd" cut-outs (a circle with a bite, a notched square, a stepped edge).

   Each piece has a small rotation (±0.5–3°) and offset, as if placed by hand
   for a stop-motion frame. Clouds become blob or pill cut-outs with the same
   hard offset. Optional **ink line accents** (one or two wobbly strokes with
   round caps) depending on Q4.
4. **Composition rules.** Keep 40–60 % calm empty field where desktop icons or
   the mobile grid sit (top-left on desktop, centre on mobile). At most 7
   layers. The terracotta `#D97757` family is present in **every** variant,
   including dark (fixes the dark terracotta-waves set).
5. **Variants via seeds.** The generator takes `--set`, `--scheme`,
   `--orientation` and `--seed`. Each set ships about 3 curated seeds, which
   vary piece placement, rotation and shape mix. The manifest records the
   seed, so any output is reproducible bit-for-bit.

**Pipeline decision.** Procedural first, hand-authored optional:

| Approach | Use |
|---|---|
| **Procedural generator** (recommended) | Default for all shipped sets: deterministic, consistent shape language, cheap to produce light/dark × desktop/mobile × seeds, testable. Port `gen-wallpapers.py` to a Rust bin (`tools/glaze-wallgen`, reusing `glaze-design` palette tokens so wallpapers and UI never drift). Keep the Python script until the Rust port reproduces its current outputs. |
| Hand-authored SVG | Allowed for one or two hero pieces (for example an ink illustration wallpaper). Must follow the same rules (no filters, palette tokens), checked by the same lint. |

- Sets (proposal): **Paper Hills** (evolved baseline), **Terracotta Waves**
  (evolved baseline, with terracotta in dark), **Kiln** (circles and arches on a
  terracotta field), **Cut-outs** (squares, blobs and odd shapes on cream),
  **Night Paper** (dark field, cream pieces, terracotta disc). Plus **Classic**:
  an original flat heritage colour with grain, for the Classic theme.
- Sizes: render PNG for 1280×800 (the VM's current mode [vm]), 1920×1080,
  2560×1440 and 1080×2340 (mobile). The shell uses cover-crop for anything
  else. Render with the `resvg` crate (or the Debian `resvg` 0.45 binary
  [vm]) plus the grain pass, at build or install time. SVG stays the source.
- Licence: the repo's MIT OR Apache-2.0 (as the current manifest says [src]),
  or CC0 if the user prefers.
- Done-check (M1):
  - a contact sheet PNG of all sets × schemes × orientations × seeds;
  - a test that every manifest entry exists and its seed reproduces the file
    hash;
  - a lint that sources contain no `<filter>`, `feGaussianBlur` or gradient;
  - a VM screenshot of the desktop with the new default wallpaper beside the
    current baseline, for the user to approve.

### 7.2 Icons

- Ink-outline set: about 24 core shell glyphs (start, search, apps, files,
  terminal, settings, power, network, volume, battery, bell, close, minimise,
  maximise, …) as SVG with stroke ~1.75 px at 24 px and round caps/joins,
  `currentColor` so they recolour per scheme.
- App icons: use the system theme (Adwaita/hicolor) for third-party apps.
  Optionally draw ink-style icons for the bundled Glaze apps only. Don't promise
  a full app icon theme.

### 7.3 Fonts (OFL, bundled or packaged)

- UI sans: **Inter** (`fonts-inter` 4.1 in Debian [vm]).
- Display serif: an OFL serif such as **Source Serif 4**, **Fraunces** or
  **Newsreader**, bundled in `assets/fonts/` with its licence. No Debian
  package was found for Source Serif 4 [vm].
- Fallback: Noto Sans and Noto Serif (already installed [vm]).

### 7.4 Illustrations

A small set of hand-drawn ink spot illustrations for empty states (no
notifications, no search results), first-run and the lock screen. Original SVG,
same stroke rules as the wallpapers.

---

## 8. Risks

| Risk | Impact | Mitigation |
|---|---|---|
| Compositor scope (M3–M4) is far larger than the panel work so far. | Months of work. A half-done compositor is worse than Sway. | Sway fallback stays first-class. Standard protocols keep the shell portable. Use `anvil` as structure. Milestone gates are screenshots, not test counts. |
| No GPU in the VM: llvmpipe frame times. | Janky motion. | The flat paper style avoids blur. Measure in M3 and record honestly. Damage tracking from day one. |
| Smithay API churn (0.7 → newer). | Breakage during the build. | Pin; upgrade only at milestone boundaries; read the vendored source rather than memory. |
| Hand-drawn style hurts legibility (wobbly icons at 16 px, thick borders). | Ugly or inaccessible UI. | Wobble only at large sizes. Contrast gate on every token. Classic theme as a crisp alternative. |
| Look too close to Anthropic's brand. | Trademark or impersonation concerns. | Original art, OFL fonts, own name and mark. Never use Anthropic logos or illustrations. |
| Agent drift into model-only work (the 619-tests-without-runtime pattern). | No visible progress. | Every milestone's "done" requires a VM screenshot plus a scripted runtime check. |
| Docs say Fedora/Windows while the OS is Debian. | Agents follow stale instructions. | ADR-0006 and a rewrite of `AGENTS.md`/`GLAZE.md`/`GOAL.md` in M0, with user approval since those are the user's instructions. |
| Login path (`~/.profile` + `dbus-run-session`) breaks systemd user env and portals. | Flatpak/portal apps and notifications misbehave. | M0 decides between a proper `systemd --user` session and documenting the limitation. |
| Uncommitted work in a repo with one commit, no remote. | Easy to lose. | The user should commit the reviewed M0 baseline (agents are told never to commit). |

---

## 9. Decisions for the user

(★ = recommended)

**Q1. Compositor strategy?**
- ★ A. Own Smithay compositor built in parallel with a shell redesign on Sway, Sway kept as fallback: full control of frames, motion and touch, with visible progress early; the largest effort.
- B. Stay on Sway with a Glaze config: cheapest, but no paper window frames, no motion, no edge gestures.
- C. Switch to labwc: stacking Win2K feel quickly; C codebase, low ceiling for motion and gestures.
- D. Fork niri or cosmic-comp: mature base; GPL-3.0 and a tiling/scrolling paradigm that fights Win2K.

**Q2. Shell UI toolkit?**
- ★ A. GTK4 + gtk4-layer-shell (current), with a `glaze-paper` widget kit: already runs in the VM, crisp CSS hard shadows and SVG ink icons; CSS has limits for custom strokes.
- B. iced: Rust-native styling; rewrites all UI and relies on a third-party layer-shell crate.
- C. Slint: good touch and declarative UI; layer-shell and licence friction.
- D. Draw the whole shell inside the compositor: total control; text, a11y and widgets from scratch.

**Q3. How strong should the stop-motion motion be?**
- ★ A. Medium: 12 fps stepped poses, tiny jitter that settles to zero, finger-tracking stays smooth: clearly "paper puppetry" without feeling laggy.
- B. Strong: 8 fps, visible jitter, stepped hover too: most characterful; can read as stutter or low performance, harder on long sessions.
- C. Subtle: smooth eased motion by default, stop-motion only on window open/close and in wallpapers: safest and most conventional; loses most of the signature feel.
- D. Off by default (instant), stop-motion opt-in: calmest and fastest; the style lives only in static art.

**Q4. How literal should the hand-drawn ink style be?**
- ★ A. Ink accents: ink-outline icons and borders, hand-drawn wobble only in wallpapers, illustrations and large icons, with geometric small UI: keeps legibility, still reads as the reference.
- B. Fully hand-drawn: wobbly borders, button outlines and frames everywhere: most distinctive; hurts legibility and density, costly to render and to theme.
- C. Paper only, no ink: flat cream/terracotta with hard offset sheets and clean lines: simplest and calmest; loses the reference's character.

**Q5. Window decorations?**
- ★ A. Force server-side Glaze paper frames for all apps that accept xdg-decoration: consistent Win2K-style title bars; GTK/libadwaita apps that insist on CSD keep their own look.
- B. Respect client-side decorations: least work; inconsistent look.
- C. Hybrid: paper frames only for non-GTK apps: fewer double title bars; two visual languages.

**Q6. Form-factor priority?**
- ★ A. Desktop first, touch/mobile at M6: the Win2K heritage lands first; mobile waits.
- B. Mobile first: touch shell sooner; postpones the desktop the project was built around.
- C. Both in lockstep: parity throughout; roughly doubles UI work per milestone.

**Q7. Glaze Classic (Win2K heritage) scope?**
- ★ A. Keep as a full alternate token set (crisp bevels, grey/teal) on the same layout: cheap once tokens are the single source; doubles the visual QA matrix.
- B. Drop Classic until after M5: less QA; loses the heritage story for now.
- C. Classic only as a layout preset (Win2K structure with paper paint): one look to polish; no real heritage visual.

**Q8. OS name (VM `/etc/os-release` currently says "KilnLinux")?**
- ★ A. KilnLinux: already deployed, and a kiln fires clay, which fits the terracotta/paper theme.
- B. ClayOS: matches the theme directly; "clay" now means paper and ink, not 3D clay.
- C. MorphLinux: neutral and flexible; no tie to the visual identity.
