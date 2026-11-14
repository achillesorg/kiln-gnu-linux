# ADR-0003 — Native compositor bring-up (Smithay)

- **Status:** Accepted
- **Date:** 2026-11-11
- **Deciders:** Glaze lead engineering
- **Related:** ADR-0001 (architecture), ADR-0002 (staged compositor strategy), `GLAZE.md` §4.1, §4.3, §8 Phase 3

## Context

ADR-0002 chose a staged strategy: Stage 1 runs an existing wlroots compositor and
lets the shell read real window state through a backend adapter; Stage 2 replaces
it with Glaze's own Smithay compositor behind the *same*
`glaze_common::model::{WindowInfo, WorkspaceInfo}` types. This ADR records the
concrete bring-up plan for Stage 2 and the decisions needed to start it without
breaking the rest of the workspace.

Constraints that shaped the decision:

- `AGENTS.md`/`GLAZE.md` §2: the authoring host is Windows 11. Linux-only code
  must stay behind `#[cfg(target_os = "linux")]` so `cargo test --workspace`
  keeps working there.
- `GLAZE.md` §7.3: no pseudocode in runtime files, and no fake "working" claims —
  the milestone list must reflect reality.
- `GLAZE.md` §4.3: a compositor that cannot present, manage focus or show real
  state must not be advertised as a usable session.
- The shell must not be rewritten when the backend changes (ADR-0002).

## Decision 1 — Smithay 0.7 with only the Wayland frontend

Dependencies (all under `[target.'cfg(target_os = "linux")'.dependencies]`):

| Crate | Version resolved | Why |
|---|---|---|
| `smithay` | 0.7.0, `default-features = false`, `features = ["wayland_frontend"]` | Protocol/state machinery; no GPU or device backend yet |
| `calloop` | 0.14.5, `features = ["signals"]` | Event loop; `signals` gives clean SIGINT/SIGTERM shutdown for `glaze-session` |
| `tracing` / `tracing-subscriber` | 0.1.44 / 0.3.23 | Real, filterable runtime logging |
| `wayland-server` | 0.31.14 (via `smithay::reexports`) | `wl_display`, `DisplayHandle`, client data |
| `wayland-protocols` (+`-wlr`, `-misc`) | 0.32.13 / 0.3.12 | Protocol enums (`xdg_toplevel`, layer shell, decoration) |

Deliberately **not** enabled yet: `backend_winit`, `backend_egl`/`renderer_gl`,
`backend_drm`/`backend_gbm`, `backend_udev`, `backend_libinput`,
`backend_session_libseat`, `xwayland`, `renderer_pixman`, `desktop` (Smithay's
`Space`). Consequences of that choice:

- The Fedora build needs no GPU toolchain: only `libxkbcommon-devel` (Smithay's
  input stack has a hard `xkbcommon` dependency) and `pkg-config`.
- `cargo test --workspace` on Windows is unaffected: the crates are target-gated,
  yet `cargo metadata`/`cargo tree` still resolve them everywhere, which is what
  keeps version drift from silently breaking the Linux build.
- Wayland/`wayland-server`/`wayland-protocols` are used through
  `smithay::reexports` rather than as direct dependencies, so there is exactly one
  version of each in the graph.

**Why not enable a nested `winit` backend now?** A nested window would let a
screenshot be taken, but it would also make "the compositor works" mean "a window
opened inside another compositor", and it would pull EGL/GL into the first
milestone. Presentation is milestone 2 (below), where it can be done properly
with damage tracking and frame callbacks. Until then the honest claim is "a real
Wayland server with real protocol state", which is what the code delivers.

## Decision 2 — module split

```
crates/glaze-compositor/src/
├── lib.rs          portable surface: NAME, is_available, ProtocolMilestone, MILESTONES
├── main.rs         binary: platform gate + logging + exit code
├── model_bridge.rs PORTABLE toplevel/workspace bookkeeping -> glaze_common::model
├── linux.rs        Linux entry: env validation (XDG_RUNTIME_DIR) + logging
├── platform.rs     wl_display bring-up + calloop event loop + socket + signals
├── state.rs        State: every Smithay state object, focus/placement policy, delegate_*
└── handlers/
    ├── mod.rs          protocol -> file map and the rationale for the split
    ├── compositor.rs   wl_compositor, wl_subcompositor, wl_shm, wl_buffer
    ├── xdg_shell.rs    xdg_wm_base: toplevels (popups: documented gap)
    ├── layer_shell.rs  zwlr_layer_shell_v1: glaze-shell's own surfaces
    ├── output.rs       wl_output + zxdg_output_manager_v1
    ├── seat.rs         wl_seat/wl_pointer/wl_keyboard/wl_touch focus notifications
    ├── decoration.rs   zxdg_decoration_manager_v1 mode negotiation
    └── foreign_toplevel.rs ext_foreign_toplevel_list_v1
```

Rationale:

- **`platform.rs` vs `linux.rs`.** `linux.rs` answers "may we run, and how do we
  log?"; `platform.rs` answers "what is a Wayland server on this host?" Keeping
  them apart means the socket/loop/dispatch plumbing can be reused unchanged when
  a presentation or device backend is added — only the event sources change.
- **`state.rs` vs `handlers/`.** `state.rs` describes what the compositor *is*
  (owned protocol states, placement policy, model registry); `handlers/` describes
  how it *reacts* per protocol. The alternative — one multi-hundred-line `impl`
  file — cannot be navigated once a dozen protocols exist. Adding a protocol is
  then three mechanical edits: a field in `State`, a `delegate_*` line, a file in
  `handlers/`.
- **`model_bridge.rs` is portable.** Placement, stacking, hit-testing and the
  conversion into `WindowInfo`/`WorkspaceInfo` are pure logic with no Smithay
  types, so they are unit-tested on every host (including Windows) and the Linux
  handlers only translate protocol events into calls. This is what keeps the shell
  backend-agnostic per ADR-0002: the shell consumes the same model whether the
  windows came from Sway or from this compositor.

## Decision 3 — milestone 1 scope, stated honestly

Implemented (real code, no stubs): `wl_display` + listening socket + `calloop`
loop + display dispatch + SIGINT/SIGTERM shutdown + flush tick; a virtual
`wl_output` with mode/scale/geometry plus `zxdg_output_v1`; a `wl_seat` with
pointer and keyboard capabilities; `xdg_wm_base` toplevel map/unmap/title/app_id/
minimize/configure/focus; `zwlr_layer_shell_v1` surfaces; `zxdg_decoration_manager_v1`
negotiation; `ext_foreign_toplevel_list_v1` publication; `wl_shm` with the two
ubiquitous 32-bit formats; buffer lifetime handling in `commit`.

`MILESTONES` therefore reports `InProgress` for XdgShell, LayerShell, Outputs,
Seat, ForeignToplevel and XdgDecoration, and `Planned` for Input and Xwayland.
`Working` is intentionally not used anywhere: the backend has not been *run* on a
Wayland host yet, and a lib test asserts that no milestone claims `Working`.

Explicitly **not** implemented (and why):

| Gap | Reason |
|---|---|
| No pixels are drawn | No presentation backend (`backend_winit`/DRM). `platform::CLEAR_COLOR` records the colour the first backend will clear to. |
| No frame callbacks | Clients that wait for `wl_frame_callback` before drawing would animate against nothing; sending them without presenting is worse than not sending them. |
| No popup placement | Popups need the parent's on-screen geometry, which requires placement + rendering. `new_popup`/`grab`/`reposition_request` log and send `popup_done` so clients are not left blocked. |
| No pointer/keyboard events | No libinput or nested input source. `State::pointer_moved` is the real hit-test seam it will call; `Input` is `Planned`. |
| No interactive move/resize | Driven by pointer events (see above). |
| Client-side decoration is granted | Glaze's target is server-side decoration it draws itself, but advertising `ServerSide` before the frame renderer exists would leave windows with no frame at all. `TARGET_DECORATION_MODE` marks the intended flip. |
| One workspace, always focused | Reporting invented workspaces would be a fabricated state list (`GLAZE.md` §4.3). |
| No clipboard/DnD, no lock, no idle inhibit, no `wlr-foreign-toplevel-management` | Separate protocols with their own milestones; `ext-foreign-toplevel-list-v1` is the one the shell needs. |
| No XWayland | Requires the `xwayland` feature plus a real seat/backend. |

## Decision 4 — deferred behaviour is logged, never faked

Every unimplemented request path either logs why it cannot be honoured
(`warn!`/`debug!`) or is answered with the honest protocol response (e.g.
`popup_done`). No handler silently drops a request and no handler returns a
success it cannot back. This matters because silent drops are exactly how a
compositor "appears" to work while breaking clients.

## Roadmap

Each milestone is a separate vertical slice with its own acceptance gate; later
milestones never require rewriting `model_bridge` or the handler split.

| # | Milestone | Scope | Gate |
|---|---|---|---|
| M2 | Presentation (nested) | Enable `backend_winit` + `renderer_gl`; `Space`/`RenderElement` for shm surfaces; a clear-colour pass using `platform::CLEAR_COLOR`; frame callbacks on presentation; damage tracking; feed winit pointer/keyboard into `State::pointer_moved` and `KeyboardHandle::input`; cursor rendering | `glaze-compositor` opens a nested window, a client's buffer is visibly rendered, clicking focuses the right window, dragging moves it |
| M3 | Window management | Replace the cascade with a real policy (tiling/snapping/floating toggle, workspaces, alt-tab ordering); popup placement via a popup manager; server-side decorations drawn in Glaze Classic/Clay geometry (`TARGET_DECORATION_MODE`) | Task switching, move/resize, multiple workspaces and popups (GTK/KDE menus) work with real apps |
| M4 | Real seat | `backend_udev` + `backend_libinput` + `backend_session_libseat` + DRM/GBM; multi-output, hotplug, per-output scale, mixed DPI, adaptive keyboard layout | Running from a TTY on real hardware on x86_64 and aarch64 |
| M5 | Session integration | `glaze-session` owns the compositor lifetime; portals and `systemd --user` environment; session lock (`ext-session-lock`); idle inhibit; notify/tray integration; XWayland with explicit CSD/SSD handling | User logs into a Glaze Wayland session from the display manager, runs typical apps, exits cleanly |
| M6 | Hardening | DMA-BUF feedback and direct scanout, `presentation-time`, clipboard/DnD (data device + primary selection), `wlr-foreign-toplevel-management` parity, idle/lease protocols, crash resilience | Stability and feature parity with the Stage 1 experience; both architectures tested |

Decorations and XWayland are called out because they are the two places where
promising too much breaks `GLAZE.md` §4.3: rounded/consistent frames are only
possible for windows Glaze decorates, and foreign apps may keep their own CSD.

## Verification

**Verified here (Windows authoring host):**

- `cargo metadata --format-version 1` → exit 0; all Linux-only crates resolve
  (smithay 0.7.0, calloop 0.14.5, wayland-server 0.31.14, wayland-protocols
  0.32.13, xkbcommon 0.8.0).
- `cargo tree -p glaze-compositor --target x86_64-unknown-linux-gnu` → exit 0,
  confirming the resolved dependency set for the Linux target.
- `cargo check -p glaze-compositor --target x86_64-unknown-linux-gnu` → exit 0.
  The `x86_64-unknown-linux-gnu` std is installed on this host, so the entire
  Smithay backend **is type-checked**, not just written.
- `cargo clippy -p glaze-compositor --target x86_64-unknown-linux-gnu --all-targets`
  → exit 0, no warnings.
- `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets`,
  `cargo test --workspace` → exit 0 (portable surface + `model_bridge` tests).

**Not verified, and why:** the compositor has never been *executed*. It has not
been linked or run against a real `wl_display`, and no client has ever connected,
so no runtime claim (socket creation, global advertisement, focus behaviour,
buffer release) is made. Cross-checking against the Linux target proves the code
compiles against Smithay 0.7; it does not prove a session works. A real Fedora
host is still required for `GLAZE.md` §9.

**Fedora verification procedure (next run on a Fedora host):**

```bash
# Build + inspection tools (libxkbcommon is a hard Smithay dependency)
sudo dnf install -y libxkbcommon-devel pkg-config wayland-utils weston
cargo build -p glaze-compositor

# Nested-safe: private runtime dir, never touch the live session
export XDG_RUNTIME_DIR=$(mktemp -d /tmp/glaze-rt.XXXXXX) && chmod 700 "$XDG_RUNTIME_DIR"
RUST_LOG=glaze_compositor=debug ./target/debug/glaze-compositor &
# The compositor logs "listening on the Wayland socket socket=wayland-N"
export WAYLAND_DISPLAY=wayland-1

wayland-info            # must list the globals in State::log_bound_globals
weston-terminal &       # maps a real toplevel inside glaze-compositor
# expect: "toplevel mapped" with title/app_id, and one foreign-toplevel handle
kill -TERM %1           # must log "glaze-compositor stopped" and exit 0
```

Acceptance for M1 on Fedora: `wayland-info` lists `wl_compositor`,
`wl_subcompositor`, `wl_shm`, `wl_output`, `zxdg_output_manager_v1`, `wl_seat`,
`xdg_wm_base`, `zxdg_decoration_manager_v1`, `zwlr_layer_shell_v1` and
`ext_foreign_toplevel_list_v1`; a client maps a toplevel and appears in the
foreign-toplevel list; SIGTERM exits 0. Anything short of that is a bug in M1 and
must not be reported as complete.

## Consequences

- Glaze owns a real Smithay compositor that the shell can already observe through
  the same model types as Stage 1, with no shell changes.
- The portable half (placement, stacking, hit-testing, model conversion) is
  tested on every host, so the risky Linux half stays small and readable.
- Nothing user-visible works yet: no pixels, no input, no popups, one workspace.
  `glaze-compositor` must not be offered as a login session before M5.
- The remaining work is additive: a presentation backend, then WM policy, then a
  device backend. No ADR-level change is expected for M2/M3.

## Alternatives rejected

- **Raw `wayland-server` with hand-written protocol objects.** Reimplements the
  state machines (surface roles, transactions, configure bookkeeping) that
  Smithay already provides, for no gain; more code, more bugs, same outcome.
- **Linking wlroots/Sway as a library.** Not a native Rust stack, forces the shell
  to depend on wlroots IPC forever, and `GLAZE.md` §4.1 prefers owning the
  compositor.
- **Making `glaze-shell`'s panel a privileged special case instead of a
  layer-shell client.** Would couple shell and compositor and break the
  "independently replaceable" rule in §4.1. `zwlr_layer_shell_v1` keeps the shell
  a normal client.
- **Shipping the current code as "the compositor is done".** Forbidden by
  §4.3/§7.2/§7.5 and by `MILESTONES`, which deliberately uses no `Working` state.
