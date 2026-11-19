# Kiln convergence: one OS, phone shell and desktop shell (design)

Status: design, not implemented. Written 2026-11-18. Scope: the DeX-style
convergence for Kiln GNU/Linux. One Kiln image runs TilesDE on the phone
screen. Plugging the phone into a monitor, keyboard and mouse switches to the
Glaze desktop. Apps, files, settings and running sessions survive the switch.

Inputs read: `glaze/AGENTS.md`, `glaze/docs/decisions.md` (Q6: desktop first,
mobile at M6), `fedeos/HANDOFF.md`, `tilesde/docs/mobile-DECISIONS.md`,
`tilesde/docs/mobile-needs-from-fede.md`, the source of
`glaze-compositor/src/{backend/udev.rs,input.rs,state.rs,control.rs}`,
`tilesde/compositor/src/{main.rs,gesture.rs,protocol.rs}`,
`glaze-shell-core/src/form_factor.rs`, `fedeos/run.sh`. Sources for external
facts are listed at the end. Claims marked **(verify)** are not confirmed
yet and must be checked in the named stage before anything depends on them.

Rule from `AGENTS.md`: any architecture change needs an ADR. Section 2 is
written so it can become `docs/adr/ADR-00xx-convergence-single-compositor.md`
without changes.

---

## 0. Summary of decisions

| # | Decision | Section |
|---|---|---|
| D1 | **One compositor, two shells.** `glaze-compositor` runs the whole session and switches between a *Handheld* policy and a *Desktop* policy at runtime. TilesDE (Qt/QML) and the Glaze shell (GTK4 + layer-shell) are both ordinary clients of that compositor. No logout and no session switch. | 2 |
| D2 | Dock detection lives **inside the compositor**, which already owns udev, DRM and libinput. The policy is a pure Rust crate `glaze-dock` (state machine, no Smithay types) that is unit-tested on macOS, like the TilesDE gesture recogniser. | 1 |
| D3 | Default docked layout = **external display shows the desktop; the phone screen becomes a touchpad plus keyboard pad**. When a physical mouse is present the phone screen turns off (external-only mode, which Phosh lacks). Mirroring is a manual option only. | 2, 5 |
| D4 | Apps are **re-configured, never restarted**. On a switch every `xdg_toplevel` gets a new configure (size, states, `configure_bounds`) and a new output with its scale. Apps with a minimum size larger than the phone get compositor-side compat scaling. | 3 |
| D5 | Form-factor hints come from data that already exists: the desktop-entry key `X-Purism-FormFactor` (used by Phosh), AppStream `<requires>/<recommends>` `display_length` and `control`, then a Kiln override file. | 3 |
| D6 | Phone-as-touchpad is **implemented in the compositor**. Touches on the internal output that hit no client input region become relative pointer motion. The pad visuals and buttons are a TilesDE layer-shell page. | 4 |
| D7 | On-screen keyboard = **squeekboard** (packaged in Debian; uses `input-method-v2` + `virtual-keyboard-v1`). The compositor adds `text-input-v3`, `input-method-v2`, `virtual-keyboard-v1`; Smithay 0.7 has all three. | 4 |
| D8 | `tilesde/compositor` stops being a separate compositor. Its gesture recogniser and JSON protocol move into a crate `glaze-touch` used by `glaze-compositor`. | 6 |
| D9 | Bring-up order: aarch64 QEMU dual-head VM, then an x86 touch convertible with HDMI (uses the existing x86 pipeline), then a phone. Phone for phone-mode work: OnePlus 6/6T (best mainline/Mobian support, cheap used). Phone for real docking: **Fairphone 5** (mainline DisplayPort over USB-C, Mobian Trixie image with limited support). | 7 |
| D10 | The prototype runs entirely in the existing aarch64 VM: `virtio-gpu-pci,max_outputs=2`, `virtio-multitouch-pci` for touch, QMP `device_add`/`device_del` of `usb-kbd`/`usb-mouse` for dock input hotplug, sysfs connector `status` writes for display hotplug, QMP `screendump ... head` for evidence. | 8 |

---

## 1. Trigger detection

### Decision

The compositor turns raw device events into one **dock state**. A pure state
machine (`glaze-dock`) receives `Fact`s and returns a `DockState` plus actions.

Facts and where they come from:

| Fact | Source | Notes |
|---|---|---|
| `DisplayConnected{connector, edid_id}` / `DisplayGone` | udev `drm` subsystem `change` event with `HOTPLUG=1` (and `CONNECTOR=` on kernels that send it), then a connector rescan | `glaze-compositor/src/backend/udev.rs` already handles `UdevEvent::Changed` with `scan_connectors()`. The rescan must now emit a fact instead of only laying outputs out left to right. |
| `DpAltMode{port, active}` | udev `typec` subsystem: `/sys/class/typec/portN-partner/` and its altmode with SVID `0xff01` (DisplayPort) | Hint only. Used to show "Display detected, waiting for signal" early. The DRM connector is the ground truth. |
| `KeyboardAdded/Removed{id}` | libinput `DeviceAdded`/`DeviceRemoved` with keyboard capability | Ignore built-in keys: devices whose udev parent is not USB/Bluetooth, or names matching `gpio-keys`, `pm8941_pwrkey`, `*-resin`, `Power Button`. Built-in list is per device in `/usr/share/kiln/devices/<compatible>.toml`. |
| `PointerAdded/Removed{id, kind}` | libinput pointer capability (`ID_INPUT_MOUSE`, `ID_INPUT_TOUCHPAD`) | |
| `TabletModeSwitch(on/off)` | libinput switch event `SW_TABLET_MODE` | Convertibles (stage H1). Tablet mode on means Handheld policy on the internal panel. |
| `DockSwitch(on/off)` | evdev `SW_DOCK` via libinput where present | Rare on phones; supported because it is free. |
| `LidSwitch` | libinput `SW_LID` | Laptops/convertibles only. |
| `UserOverride(mode)` | control socket / quick setting | Always wins until the hardware set changes. |

logind: the compositor does **not** read `org.freedesktop.login1.Manager.Docked`.
It sets policy itself and takes a logind `handle-lid-switch` inhibitor while
docked, so closing a convertible's lid with an external display does not
suspend.

State machine:

```
Handheld ──display connected──▶ Prompt (first time for this EDID) ──accept──▶ Docked
   ▲                                    └──"phone only"──▶ HandheldPlusMirror(off)
   │                                                        (external stays dark)
   └──────── display gone (debounced 1500 ms) ◀──────────── Docked
Handheld + keyboard only  ──▶ Handheld, OSK suppressed   (no profile switch)
Docked + mouse present    ──▶ Docked, internal panel off (external-only)
Docked + no mouse         ──▶ Docked, internal panel = touchpad pad
```

- Debounce: connect 750 ms (DP alt-mode link training and dock firmware can
  flap), disconnect 1500 ms. Phosh's reported spurious docked mode is the
  lesson here.
- The choice per display is remembered by a hash of EDID manufacturer,
  product and serial in `~/.config/kiln/dock.toml`. Unknown displays prompt
  once (like DeX's "Start DeX?").
- A manual override (quick-settings toggle in both shells, plus
  `glaze dock set handheld|docked|auto`) always exists. Phosh users report the
  docked toggle greyed out with no override; Kiln must not repeat that.

### Components / files

- New crate `glaze/crates/glaze-dock/` (`state.rs`, `facts.rs`, `edid.rs`, tests). No Smithay dependency.
- `glaze-compositor/src/backend/udev.rs`: emit facts from connector rescans; add a `typec` udev monitor.
- `glaze-compositor/src/input.rs`: emit facts from libinput device add/remove and switch events.
- `glaze-compositor/src/control.rs`: `dock-state`, `dock-set`, and a push event `{"event":"dock","state":...}`.
- `/usr/share/kiln/devices/*.toml`: built-in input names, internal connector name, touch device to output mapping.

### Work breakdown

| Task | Size |
|---|---|
| `glaze-dock` state machine with debounce, override, EDID memory, 20+ unit tests | M |
| Connector rescan emits facts; EDID parse (manufacturer/product/serial) | S |
| libinput device classification + device TOML loader | S |
| typec monitor (hint only) | S |
| Control socket messages + `glaze dock` CLI subcommand | S |

### Risks

- Phone kernels report USB hubs, docks and charger pads in odd ways; a dock
  with a built-in keyboard controller may appear before the display.
  Mitigation: the display is the only fact that starts Docked.
- EDID absent (some HDMI adapters, virtio-gpu without `edid=on`): fall back to
  connector name + mode list hash.

### Acceptance tests

1. Unit: a sequence `KeyboardAdded, DisplayConnected, (600 ms) DisplayGone, (200 ms) DisplayConnected` stays Handheld until 750 ms after the last connect, then goes Docked exactly once.
2. VM: hot-adding `usb-kbd` via QMP leaves the profile Handheld and logs `osk=suppressed`.
3. VM: forcing the second connector on yields `{"event":"dock","state":"prompt"}` on first sight and `docked` directly after the display was accepted once.

---

## 2. Switching design

### Decision

Three options were considered:

| Option | What happens on dock | Apps survive? | Verdict |
|---|---|---|---|
| A. Session switch (log out of a TilesDE session, log in to a Glaze session, or two compositors switched by VT) | New compositor, new Wayland display | **No.** Every Wayland client dies with its compositor. | Rejected. Breaks the core promise. |
| B. One compositor, two shells, profile switch | Compositor changes window policy; Glaze shell maps on the external output; TilesDE switches its internal-output page to the pad | Yes. Clients keep their connection; they only get new configures. | **Chosen.** |
| C. Same as B but the phone screen keeps the full phone UI (two independent desktops) | Two shells active at the same time | Yes | Kept as a later mode ("Dual"). Doubles input-focus and app-placement complexity; DeX dropped a similar dual mode in practice. |

Profile semantics inside `glaze-compositor`:

- **Handheld**: one output (internal) gets the window manager. Policy
  `SingleAppFullscreen`: each new toplevel is maximised to the internal
  output's usable area (the area minus the TilesDE status bar and nav bar
  exclusive zones), exactly one app visible, shell gestures from the edge
  recogniser. This is what `tilesde/compositor` does today.
- **Desktop**: the external output gets the existing `glaze-wm` stacking/tiling
  policy (snap, Alt+Tab, workspaces). The internal output gets only the pad
  layer surface (or is powered off).
- The policy is a trait (`trait WindowPolicy`) that wraps today's
  `wm::WindowManager` for Desktop and a new `HandheldPolicy`. This is change 2
  of `tilesde/docs/mobile-needs-from-fede.md`.

Output roles: each output gets a role `Primary | Pad | Mirror | Off`. Today
`udev.rs` says "the window manager works on the first (primary) output; the
others show layers". That rule becomes "the window manager works on the
output whose role is Primary", which moves on dock and undock.

Shell lifecycle:

- TilesDE runs for the whole session (it owns lock screen, notifications
  centre and the pad page). On dock it receives `{"event":"profile","name":"desktop"}`
  and shows the pad page on the internal output.
- The Glaze shell (`glaze-desktop`, panels, start menu) starts on the first
  dock and stays alive for 10 minutes after undock with its surfaces
  unmapped, so a quick re-dock is instant. After that it exits (RAM on a
  6–8 GB phone). `glaze-session` supervises both.
- The session lock is one lock (`ext-session-lock`, already in
  `glaze-compositor/src/handlers/session_lock.rs`) with one lock surface per
  output; TilesDE's lock screen is the lock client in both profiles.

Switch sequence (dock), target under 1.5 s from debounced fact to first
desktop frame:

1. Freeze: compositor stops committing frames on all outputs for at most 300 ms (no half-relaid layout on screen).
2. Configure the new output (mode, scale 1.0 default, transform normal).
3. Move every toplevel to the external output, compute desktop geometry (section 3), send configures.
4. Send `profile` event to shells; start or map the Glaze shell.
5. Switch the internal output to role Pad (or Off when a mouse is present).
6. Unfreeze when every toplevel has acked its configure or after 300 ms, whichever comes first; late clients render scaled until they commit.

Undock is the reverse; the window that had focus becomes the visible
Handheld app.

### Components / files

- `glaze-compositor/src/wm.rs`, `state.rs`: `WindowPolicy` trait, output roles, profile switch transaction.
- New `glaze-compositor/src/policy/handheld.rs`.
- `glaze-session/src/`: supervise TilesDE + Glaze shell, lazy start, warm hold.
- `tilesde/shell/qml/`: `DockPad.qml` page, handle `profile` event.
- ADR `glaze/docs/adr/ADR-00xx-convergence-single-compositor.md`.

### Work breakdown

| Task | Size |
|---|---|
| ADR | S |
| `WindowPolicy` trait around existing WM, no behaviour change on desktop (all existing tests green) | M |
| `HandheldPolicy` (port of TilesDE compositor model) | M |
| Output roles + moving the WM's primary output at runtime | M |
| Switch transaction with freeze/unfreeze and configure-ack tracking | L |
| Shell lifecycle in `glaze-session` | M |

### Risks

- `glaze-compositor` is desktop-first and mobile is milestone M6 in
  `docs/decisions.md`. This plan must not slow M0–M5; every task above is
  additive behind the trait and the default config stays Desktop.
- GTK shell plus Qt shell both resident costs RAM. Measure on the phone; if
  above budget, drop the warm hold.
- Smithay output removal while clients hold `wl_output` references: Lomiri's
  2025 roadmap lists a shell crash on external display unplug as a fix item.
  The 50-cycle unplug soak (section 8) is mandatory before any device build.

### Acceptance tests

1. With profile Desktop and a single output, `cargo test --workspace` and the existing VM screenshot checks give the same results as before the trait.
2. VM: dock while `foot` and `glaze-files` run: both PIDs unchanged after the switch, both windows listed on the external output by `glaze status` / control `windows`.
3. Debounced-fact-to-first-frame time logged; median of 20 docks under 1.5 s in the VM.

---

## 3. Keeping apps alive across the switch

### Decision

Same Wayland connection, new configure. Per toplevel the compositor stores
two remembered geometries, one per profile, keyed by `app_id` plus the
`xdg_toplevel_tag_v1` tag when the client sets one (wayland-protocols 1.43
added that protocol), else by `app_id` and window index.

On Handheld → Desktop:
- unset `fullscreen`/`maximized` unless the remembered desktop state had them;
- send `configure_bounds` (xdg-shell v4, wayland-protocols 1.25) = external usable area;
- send `configure` with the remembered desktop size, or a cascade default of 60 % of the work area;
- `wl_surface.leave(internal)`, `wl_surface.enter(external)`; scale changes come through `preferred_buffer_scale` (wl_surface v6) and `wp_fractional_scale_v1`, both of which the compositor already binds or Smithay offers.

On Desktop → Handheld:
- send `maximized` + all four `tiled_*` states, size = internal usable area, `configure_bounds` the same;
- apps not visible get the xdg `suspended` state (wayland-protocols 1.32) so they stop rendering;
- if the client's `min_size` exceeds the phone area: **compat scaling**. The compositor configures the client at its min size and draws it scaled down to fit (letterboxed), with a one-time hint "This app is not made for phones". Same for apps flagged desktop-only.

Form-factor hints, in priority order:

1. Kiln override `/usr/share/kiln/form-factor.d/*.toml` and `~/.config/kiln/form-factor.toml` (`[org.gnome.Nautilus] handheld = "adaptive"`).
2. Desktop entry `X-Purism-FormFactor=Workstation;Mobile;` (the key Phosh already uses, present in many GNOME and Mobian-patched apps).
3. AppStream metainfo `<requires>`/`<recommends>` `<display_length>` and `<control>touch</control>`.
4. Default: GTK4/libadwaita and Qt Quick apps adaptive; X11 (XWayland) apps desktop-only with compat scaling.

The TilesDE All Apps list greys out (not hides) desktop-only apps in Handheld.

XWayland apps: the X11 window manager part (`glaze-compositor/src/xwayland.rs`)
sends `ConfigureNotify` with the new size; X11 apps that ignore it keep their
size and get compat scaling.

### Components / files

- `glaze-compositor/src/state.rs`: per-window `ProfileGeometry { handheld, desktop }`.
- `glaze-compositor/src/handlers/xdg_shell.rs`: bounds, suspended, tag.
- New `glaze-compositor/src/compat_scale.rs` (render-time scale element).
- `glaze-shell-core/src/form_factor.rs`: extend `FormFactor` with `AppFit { Adaptive, DesktopOnly, HandheldOnly }` and the hint resolver.

### Work breakdown

| Task | Size |
|---|---|
| Per-profile geometry memory + re-configure on switch | M |
| `configure_bounds`, `suspended`, `xdg_toplevel_tag_v1` | S |
| Compat scaling render path + input coordinate transform | M |
| Hint resolver (desktop entry, AppStream, override) with tests | S |
| XWayland re-configure | S |

### Risks

- Apps that recreate their whole widget tree on resize (some Electron apps) flicker or lose scroll position; acceptable, they are not restarted.
- Apps that cache output scale at start (older toolkits) render blurry after the move until restarted; compat scaling covers the visual case.

### Acceptance tests

1. VM: `foot -e sh -c 'echo $$ >/tmp/foot.pid; exec cat'`, type `hello`, dock, type `world`, undock: same PID, terminal shows both words, window size differs per profile and matches the remembered geometry on the second dock.
2. A test client with `min_size` 1024x768 on an 800x1280 phone output is drawn letterboxed; a tap at its centre arrives at client coordinates (512, 384) ± 1.
3. Hidden Handheld apps report the `suspended` state (`WAYLAND_DEBUG=1` log of a GTK4 client).

---

## 4. Input model

### Decision

| Situation | Phone screen | Cursor | Text entry |
|---|---|---|---|
| Handheld | touch to apps, edge gestures to shell (`glaze-touch` recogniser, 28 px band) | hidden | squeekboard on text-input focus |
| Handheld + keyboard | as above | hidden (shown if a mouse is also added) | physical keyboard; OSK suppressed |
| Docked, no mouse | **pad**: whole screen is a touchpad; pad page has left/right buttons, keyboard button, a small notification strip | external output | squeekboard shown **on the phone screen** (not the external display) when a text field on the external output has focus or the keyboard button is tapped |
| Docked + mouse | off (or pad if user prefers) | external | physical keyboard, OSK on phone screen only on request |

Pad gestures (compositor-side, in `glaze-touch`):

- 1-finger move: relative pointer motion with an acceleration curve
  (libinput's "adaptive" profile shape as reference).
- 1-finger tap: left click; 2-finger tap: right click; 2-finger move: scroll
  (`wl_pointer.axis` with `axis_source = finger`); 3-finger swipe up/down:
  task view / show desktop; tap-and-drag: drag lock.
- Pinch on pad: forwarded as `zwp_pointer_gesture_pinch_v1` to the client under
  the cursor (the compositor already imports the gesture event types in
  `input.rs`).

Routing rule: a touch on the internal output first goes through the edge
recogniser, then the hit test. If it hits a client surface's input region
(the pad's buttons, squeekboard) it goes to that client as `wl_touch`.
Otherwise in Desktop profile it becomes pad input. The pad page therefore has
an input region only on its buttons.

Touch-to-output mapping: libinput touch devices are mapped to the internal
output by the device TOML (`touch_output = "DSI-1"`), never "first output",
or touches land on the external display after docking.

### Components / files

- New crate `glaze/crates/glaze-touch/` from `tilesde/compositor/src/gesture.rs` + `protocol.rs` (moved, tests kept), plus `pad.rs`.
- `glaze-compositor/src/input.rs`: `seat.add_touch()`, `InputEvent::Touch*` handling (zero touch code today), routing rule above.
- `glaze-compositor/src/state.rs`: `TextInputManagerState`, `InputMethodManagerState`, `VirtualKeyboardManagerState` (Smithay 0.7).
- `tilesde/shell/qml/DockPad.qml`.
- Package dependency: `squeekboard`.

### Work breakdown

| Task | Size |
|---|---|
| Move gesture recogniser into `glaze-touch` (keep its 6 tests) | S |
| Touch seat capability, touch routing, output mapping | M |
| Pad: motion, taps, scroll, drag lock, 3-finger gestures, unit tests on synthetic tracks | M |
| text-input / input-method / virtual-keyboard globals; squeekboard placement on the internal output in Desktop profile | M |
| OSK suppression when a physical keyboard is present | S |

### Risks

- squeekboard positions itself as a bottom layer-shell surface on "the" output; placing it on the internal output while text focus is on the external one needs the compositor to pick the output for the input-method popup surface. **(verify)** squeekboard behaviour with two outputs in stage P5.
- Pad latency: compositor-side processing keeps it in-process; target under 16 ms from touch event to cursor frame.

### Acceptance tests

1. VM with `virtio-multitouch-pci`: a scripted 1-finger swipe of 200 px on the pad moves the cursor on head 1 (pointer position read from control `state`) by an amount within the acceleration curve's expected range.
2. 2-finger tap on pad opens the Glaze context menu under the cursor (screendump head 1).
3. Focus a `foot` window on head 1, tap the pad's keyboard button, type via squeekboard on head 0: text appears in `foot`.
4. Hot-add `usb-mouse`: pad goes off (head 0 blank or DPMS off), cursor still works.

---

## 5. Power and thermal

### Decision

- Docked + mouse: internal panel **DPMS off** (external-only). Docked pad
  mode: pad dims to 10 % brightness after 10 s without a touch, wakes on touch.
- External output default mode capped at **1920x1080@60** on phone-class
  hardware, even if the monitor offers 4K. User can raise it in Settings.
- Idle in Docked: blank external after the desktop idle timeout; no
  auto-suspend while external power is present (USB-C power role sink via
  `/sys/class/typec/portN/power_role`, or `power_supply` online), else the
  normal phone suspend policy.
- Thermal: `glaze-power` reads `/sys/class/thermal/thermal_zone*/temp`. Above
  the device TOML's `warm` threshold it requests `power-saver` from
  power-profiles-daemon, disables shell animations (`reduceMotion`) and limits
  the external output to 30 Hz frame callbacks; above `hot` it shows a
  notification. Values per device, not global.
- Rotation (Handheld only): `iio-sensor-proxy` (Debian package) over D-Bus
  `net.hadess.SensorProxy` drives the internal output transform. Rotation is
  ignored while Docked.

### Components / files

- `glaze-power/src/`: thermal and power-source watcher, D-Bus client for power-profiles-daemon.
- `glaze-compositor`: DPMS per output, frame-callback throttling per output, output transform (change 3 of `mobile-needs-from-fede.md`).

### Work breakdown

| Task | Size |
|---|---|
| Per-output DPMS + pad dimming | S |
| External mode cap + Settings override | S |
| Thermal/power watcher + profile requests | M |
| Rotation via iio-sensor-proxy + input transform | M |

### Risks

- Phone GPUs (Adreno 630 on SDM845, Adreno 643 on QCM6490) are fine at
  1080p for a 2D desktop; 4K external composition is the thermal risk, hence
  the cap.
- No battery or thermal sensors in the VM: these paths are verified only on hardware (H1, H2).

### Acceptance tests

1. VM: with `usb-mouse` present while docked, `cat /sys/class/drm/card0-Virtual-1/dpms` reads `Off`.
2. Hardware H2: one hour docked with a browser video at 1080p, the device stays below its `hot` threshold, or throttling is logged before it.

---

## 6. What exists today and what is missing

| Area | Exists | Missing |
|---|---|---|
| `glaze-compositor` (Smithay 0.7) | udev/DRM backend, multi-connector scan, hotplug rescan on udev `Changed`; layer-shell, session-lock, viewporter, fractional-scale, xdg-output, foreign-toplevel, cursor-shape, XWayland; pointer gestures types imported; control socket | **No touch at all** (no `Touch` in `input.rs`). WM only on first output. No output roles, no policy trait, no output transform, no text-input/input-method/virtual-keyboard, no DPMS per output, no dock facts. |
| `tilesde/compositor` (Smithay 0.7, ~700 lines + libs) | winit-nested prototype, single-app fullscreen model, wl_touch routing, edge-gesture recogniser (6 tests), JSON protocol `touch/swipe/rotate/spawn/state`, rotation event | No DRM backend, no session lock, not used by anything on hardware. Its useful parts are the recogniser, the protocol and the handheld policy logic: these move into `glaze-touch` and `policy/handheld.rs`. |
| TilesDE shell (Qt 6.8 / QML) | Start tiles, All Apps, lock, Action Centre, switcher, nav bar, status bar; control socket; mocked services | Is an xdg fullscreen toplevel, not layer-shell (LayerShellQt not installed); mocked services; no pad page; no real app launching or window list from the compositor. |
| Glaze shell (GTK4 + gtk4-layer-shell) | Desktop panels, start, settings; `glaze-shell-core::FormFactor {Desktop, Mobile}` and unseen `mobile_ui.rs` | Shell must handle appearing and disappearing outputs and being started lazily. The GTK mobile UI is superseded by TilesDE: `FormFactor::Mobile` should mean "TilesDE owns the screen". |
| FlowDE mobile shell | Ancestor of TilesDE (same code), backed up | Nothing to take beyond what TilesDE has. |
| Settings | Glaze config schema (`glaze-common/src/config.rs`), TilesDE `Theme.qml` tokens | One shared appearance source (accent, dark mode, wallpaper, text scale) read by both shells. Use the XDG settings portal keys `org.freedesktop.appearance` `color-scheme` and `accent-color` as the shared surface, backed by one Kiln file. Form-factor-specific state (tile layout vs taskbar pins) stays separate. |
| Files | Same `$HOME`, same user | Nothing needed: one session, one user. |

### Work breakdown (shared settings)

| Task | Size |
|---|---|
| Kiln appearance store + portal settings backend values | M |
| TilesDE reads accent/dark mode/text scale from it instead of `Theme.qml` defaults | S |

### Acceptance tests

1. Changing accent in Glaze Settings while docked, then undocking: TilesDE tiles use the new accent without a restart.

---

## 7. Per-device bring-up path

### Decision

| Stage | Hardware | Why | Kernel / base |
|---|---|---|---|
| V (now) | aarch64 QEMU VM on the Mac (`fedeos/run.sh`, HVF) | Already the dev runtime; dual-head and touch are emulable | Debian 13 kernel 6.12 |
| H1 | Any x86_64 2-in-1 or touch laptop with HDMI and an `SW_TABLET_MODE` switch | Real DRM hotplug, real touch, real GPU, uses the existing x86 pipeline (`stage-kiln/build`); cheapest real test of the whole switch | Debian 13 stock kernel |
| H2a | **OnePlus 6/6T** (SDM845), used | Best-supported Mobian device with fully working stable image; cheap; good for Handheld profile, touch, OSK, power, rotation | Mobian Trixie kernel (6.12 LTS) and device packages |
| H2b | **Fairphone 5** (QCM6490) | DisplayPort over USB-C has mainline patches (Feb–Mar 2025, Luca Weiss) and DP audio patches merged in 2025; Mobian Trixie ships an image with limited support | Mobian/postmarketOS mainline kernel **(verify)** DP alt mode in the kernel Kiln ships |

Notes:
- OnePlus 6/6T reportedly lack DisplayPort alt mode in hardware (community
  report only). **(verify)** on the postmarketOS device wiki before buying it
  for docking; if true, H2a is Handheld-only, and docking there would need a
  USB DisplayLink/GUD adapter, which is out of scope.
- Other Mobian Trixie devices (PinePhone, PinePhone Pro, PineTab, Librem 5,
  Pixel 3a, Pocophone F1) are possible later targets. SHIFT6mq has DP alt mode
  in hardware but limited Mobian support.
- Kiln reuses Mobian's device packages (kernel, firmware, tweaks) from its apt
  repository rather than porting kernels. This is a supply-chain dependency;
  pin and mirror it like the other Kiln repositories.
- Droidian (Halium, Android vendor kernel) is rejected: it ties Kiln to
  Android BSP kernels and hwcomposer, and gains nothing for DP alt mode.

Lessons from existing projects:
- **Phosh**: automatic docked detection was reported as spurious and
  without override; no external-only mode. → debounce, manual override,
  external-only by default when a mouse is present.
- **Lomiri / Ubuntu Touch**: years of desktop mode work; 2025 roadmap still
  lists multi-monitor greeter and a crash on external display unplug. →
  unplug soak test before any device build; lock screen on all outputs.
- **Phosh form factor key** `X-Purism-FormFactor` already tags many apps. → reuse it.
- **DeX**: non-resizable Android apps must restart or be letterboxed. Wayland
  toplevels are resizable by protocol, so Kiln letterboxes only by min size.

### Work breakdown

| Task | Size |
|---|---|
| H1: device TOML for one convertible, x86 image with TilesDE + Glaze + squeekboard | M |
| H2a: arm64 image flavour with Mobian device packages for SDM845, boot to TilesDE | L |
| H2b: Fairphone 5 image, DP alt-mode docking test | L |

### Risks

- Mobian lists Fairphone 5 under limited support (audio/other features
  missing in 2025). Docking works only where the kernel has the DP series.
- Phone purchases need the owner's decision; this doc only recommends.

### Acceptance tests

1. H1: fold to tablet mode → Handheld; unfold with HDMI monitor → Docked; 20 cycles, no client dies.
2. H2a: boots to TilesDE lock screen, touch and OSK work, suspend/resume 10 times.
3. H2b: USB-C dock with HDMI + USB keyboard + mouse → Glaze desktop on the monitor within 3 s; unplug returns to TilesDE with the same apps.

---

## 8. Staged prototype plan in the aarch64 QEMU VM

All stages run in the existing VM. `fedeos/run.sh` is outside the repo and
needs the owner's approval to change, so the dual-head launcher is a new
versioned script `glaze/scripts/vm/run-dual.sh` that adds or replaces only:

```
-device virtio-gpu-pci,max_outputs=2,xres=1280,yres=800
-device virtio-multitouch-pci          # QEMU >= 8.1; touch device for the internal output
-device qemu-xhci                       # keyboard/mouse are hot-added per test
```

Head 0 (`Virtual-1`) plays the phone: 1280x800 with output transform 90°
gives an 800x1280 portrait panel. Head 1 (`Virtual-2`) plays the monitor.

Hotplug simulation:
- Display: `echo on|off|detect > /sys/class/drm/card0-Virtual-2/status` (the
  connector `status` attribute accepts writes since 2015; root needed, done
  only by the test script via `$HOME/fedeos/vm`). **(verify P0)** that
  virtio-gpu emits a hotplug uevent on the forced change; fallback is the
  compositor test command `{"cmd":"sim-display","connector":"Virtual-2","connected":bool}`
  which feeds the same `DisplayConnected` fact.
- Input: QMP `device_add usb-kbd,id=dockkbd,bus=xhci.0` / `device_del dockkbd`,
  same with `usb-mouse,id=dockmouse`. These are real udev/libinput hotplugs.
- Evidence: QMP `screendump` with `device` and `head` arguments per head,
  saved under `glaze/docs/screens/convergence/`. `scripts/vm/shot.sh` gets a head parameter.

| Stage | Goal | Size | Acceptance test (scripted, evidence = screendumps + control-socket JSON + logs) |
|---|---|---|---|
| P0 | Dual-head VM boots; guest sees two connectors; hotplug sim works | S | `modetest -c` (or `/sys/class/drm/card0-Virtual-*/status`) lists two connectors; after `echo off` then `echo on` on Virtual-2, `udevadm monitor -k` shows a `drm` change event, or the fallback command is chosen and recorded in this doc. |
| P1 | `glaze-compositor` places the WM on the role-Primary output; roles switchable from control socket | M | `{"kind":"set-output-role","output":"Virtual-2","role":"primary"}` moves a running `foot` to head 1; screendump head 1 shows it, head 0 shows only layers. Existing desktop VM checks unchanged. |
| P2 | Touch in `glaze-compositor` + `glaze-touch` recogniser | M | `virtio-multitouch-pci` touch on head 0 reaches a GTK4 test app (`WAYLAND_DEBUG` shows `wl_touch.down`); bottom-edge fast swipe emits `{"event":"gesture","name":"home"}`; the 6 moved recogniser tests pass on macOS. |
| P3 | Handheld policy; TilesDE runs on `glaze-compositor` on head 0 rotated to portrait | L | Boot into Handheld: screendump head 0 shows TilesDE Start at 800x1280; launching `foot` from a tile makes it fullscreen; home gesture returns to Start; `foot` PID alive. |
| P4 | Dock state machine + live profile switch | L | Script: start `foot` (PID file) and a GTK4 app in Handheld; `echo on` Virtual-2; within 1.5 s control `state` reports `profile=desktop`, both windows on Virtual-2, PIDs unchanged, Glaze taskbar on head 1, pad on head 0; `echo off` returns both apps to head 0 Handheld. |
| P5 | Pad + OSK | M | Section 4 tests 1–4 pass in the VM. |
| P6 | Form-factor hints, compat scaling, shared appearance | M | Section 3 tests 1–3 and section 6 test 1 pass. |
| P7 | Soak | S | 50 dock/undock cycles with 5 clients (foot, GTK4, Qt6, XWayland `xterm`, a client with `min_size` 1024x768): zero compositor crashes, zero client disconnects (`ss -x` count of Wayland socket connections constant), RSS growth of the compositor under 10 MB. |

Order rule from `AGENTS.md`: each stage is a vertical slice that ends with
VM evidence and an update to `PROJECT_STATUS.md`; no stage is marked done
from unit tests alone.

### Risks (prototype)

- VM performance: llvmpipe rendering with two heads doubles CPU load; the VM
  already freezes when Mac swap fills. Keep heads at 1280x800 and run one soak at a time.
- `virtio-multitouch-pci` availability depends on the Homebrew QEMU build; check with `qemu-system-aarch64 -device help | grep multitouch` in P0. Fallback: `virtio-tablet-pci` plus the existing compositor `touch`/`swipe` JSON commands from `tilesde/compositor/src/protocol.rs`.
- Cocoa display shows one head at a time; screendumps per head are the evidence, not the window.

---

## 9. Overall order and sizing

1. P0, ADR, `glaze-dock` crate (S+S+M) — can start now, does not touch desktop behaviour.
2. P1 output roles + policy trait (M+M).
3. P2 touch (M+M).
4. P3 Handheld policy + TilesDE on the real compositor (L) — this is also TilesDE's ranked next step 8 in `HANDOFF.md`.
5. P4 profile switch (L+L).
6. P5 pad and OSK, P6 hints/compat/appearance (M each).
7. P7 soak (S), then H1 (M), H2a (L), H2b (L).

Desktop milestones M0–M5 keep priority per `docs/decisions.md`; steps 1–2
are safe to interleave because they are additive and keep Desktop the default.

---

## Sources

- Phosh docked mode reports: https://forums.puri.sm/t/questions-about-using-phosh-in-docked-mode-with-an-external-monitor/15831 , https://source.puri.sm/Librem5/phosh/-/issues/421 , https://forum.pine64.org/showthread.php?tid=15770 , external-only request https://gitlab.com/postmarketOS/pmaports/-/issues/858
- Mobian accessories/docking: https://wiki.debian.org/Mobian/HardwareAccessories
- Mobian Trixie devices: https://wiki.debian.org/Mobian/Devices , https://linuxiac.com/mobian-trixie-officially-released-with-broader-device-support/ , https://theregister.com/2025/10/21/mobian_trixie
- Fairphone 5 DP over USB-C: https://patchew.org/linux/20250226-fp5-pmic-glink-dp-v1-0-e6661d38652c@fairphone.com , https://patchew.org/linux/20250312-fp5-pmic-glink-dp-v2-0-a55927749d77%40fairphone.com , DP sound https://patchew.org/linux/20250507-fp5-dp-sound-v4-0-4098e918a29e@fairphone.com , FOSDEM 2026 update https://fosdem.org/2026/events/attachments/ZNAAZX-mainline_kernel_for_fairphones_-_2026_update/slides/266938/fosdem_20_rsemaak.pdf
- OnePlus 6 DP alt mode report: https://feddit.it/post/8373855
- Lomiri roadmap: https://ubports.com/lomiri-roadmap
- DRM connector status write: https://github.molgen.mpg.de/mariux64/linux/commit/c484f02d0f02fbbfc6decc945a69aae011041a27
- xdg-shell suspended (wayland-protocols 1.32): https://gitlab.freedesktop.org/wayland/wayland-protocols/-/merge_requests/201 ; configure_bounds (1.25): https://lists.freedesktop.org/archives/wayland-devel/2022-January/042102.html ; xdg-toplevel-tag (1.43): https://lists.freedesktop.org/archives/wayland-devel/2025-April/044134.html
