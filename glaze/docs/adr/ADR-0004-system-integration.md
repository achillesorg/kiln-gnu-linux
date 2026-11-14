# ADR-0004 — System integration: backends, IPC and D-Bus ownership

- **Status:** Accepted
- **Date:** 2026-11-11
- **Deciders:** Glaze lead engineering
- **Supersedes:** none. Relates to ADR-0002 (compositor strategy) and ADR-0005
  (portability and verification).

## Context

The shell must observe **real** window/workspace state, launch **real** apps and
show **real** notifications and tray icons (`GLAZE.md` §4.3). On the authoring
host there is no Wayland, no D-Bus and no compositor, so the integration had to be
split into a portable, testable core and thin platform adapters
(`docs/adr/ADR-0005`). This ADR fixes the three integration seams: the compositor
**backend** abstraction, the local **IPC**, and the **D-Bus** services with their
ownership rules.

## Decision

### 1. Compositor backends behind one trait (`crate glaze-backend`)

The shell and `glaze-service` never touch a compositor directly. They consume
`glaze_common::model::{WindowInfo, WorkspaceInfo}` produced by a `dyn Backend`:

- `ForeignToplevelBackend` — `wlr-foreign-toplevel-management`.
- `SwayBackend` — Sway/i3 IPC (`GET_TREE`).

The trait exposes `name`, `is_connected`, `windows`, `workspaces`,
`activate_window`, `close_window` and `refresh`. Every capability beyond the two
identity methods has a default that returns an honest
`BackendError::Unsupported`, so a backend implements only what it really can.
`BackendError` distinguishes `Unsupported` (cannot do/have that here) from
`NotConnected`, `NotFound`, `Protocol` and `Io`.

**Portable and tested on any host:** i3 IPC framing (`encode`, `decode_header`,
`parse_frame`), `GET_TREE` → model mapping, focus/close command construction, and
the whole `ToplevelRegistry`. **Unix-only:** the socket/wayland transports.

### 2. Why `wlr-foreign-toplevel-management` is preferred over Sway IPC

| | `wlr-foreign-toplevel-management` | Sway IPC |
|---|---|---|
| Standard | Wayland protocol, implemented by Sway, Hyprland, KWin, labwc, niri… | Sway/i3 specific |
| Portability of the backend | Survives a compositor swap; feeds the **native** `glaze-compositor` too | Dies with Sway |
| Data model fit | Title/app_id/`activated`/`minimized`/`maximized` map 1:1 to `WindowInfo` | Superset (tree, rects, layouts) — more than the taskbar needs |
| Security/robustness | A protocol, not a command channel | `RUN_COMMAND` executes commands — more surface than needed |

Sway IPC is kept as a **fallback and diagnostics** path (richer tree, and the
only option when the foreign-toplevel global is not exported). The preferred path
is the standard protocol because it is compositor-agnostic and is the same
mechanism the future Smithay compositor will publish (ADR-0002).

### 3. Local IPC (unchanged transport, backend now attached)

`glaze-service` still speaks newline-delimited JSON over
`$XDG_RUNTIME_DIR/glaze/control.sock` (mode `0600`). `ServiceState` now optionally
holds a `Box<dyn Backend>`:

- **With** a backend: `ListWindows`/`ListWorkspaces` return its real data;
  `ActivateWindow`/`CloseWindow` act via the backend; `Status` reports the backend
  name and live window count.
- **Without** a backend: windows/workspaces are an **empty list** (never
  fabricated) and focus/close return `BackendUnavailable`.

The request **dispatch is portable** and unit-tested on every host; only the
socket serve loop is `#[cfg(unix)]`.

### 4. D-Bus services and the single-owner rule

Two optional session-bus services, both **off by default**
(`--features dbus`, zbus 5):

- **Notifications** — `org.freedesktop.Notifications`.
- **Tray** (StatusNotifier) — `org.kde.StatusNotifierWatcher`.

**Single-owner rule (notifications).** Glaze must never double-claim
`org.freedesktop.Notifications`. Before serving, it requests the name with
`RequestNameFlags::DO_NOT_QUEUE` and consults the *portable, tested*
`notifications::decide_name_claim(already_owned)`. If another daemon
(GNOME Shell, dunst, mako, …) already owns it, Glaze **refuses to start** its own
server and stands down, instead of fighting for the name
(`GLAZE.md` §4.3). The portable decision and the whole notification history ring
(`NotificationHistory`, with `Notify`/`CloseNotification`/`GetCapabilities`/
`GetServerInformation` semantics) are unit-tested on every host; only the zbus
adapter is platform-bound.

The StatusNotifier watcher follows the same rule: if a watcher already runs, Glaze
does not start a second one.

**Dependency policy.** `zbus` is an **optional** dependency behind the `dbus`
feature. It is resolution-tracked but not built unless the feature is enabled, so
the root workspace keeps building on Windows (ADR-0005). The feature is validated
on Fedora only.

## Consequences

- The backend seam is complete and tested; `glaze-service` surfaces real backend
  data or honest `BackendUnavailable` errors — no fabricated windows.
- The **live** `wlr-foreign-toplevel` Wayland client and the live D-Bus adapters
  are **not yet runtime-verified**. They are written as thin adapters over the
  tested models and must be exercised on a Fedora Wayland session. This is
  disclosed, not hidden.
- Adding the native Smithay compositor (ADR-0002 stage 2) only requires a new
  `Backend` impl; no shell or IPC change.
- `packaging/fedora` installs the pieces as an **opt-in** session and never edits
  the login manager, other sessions or boot settings.

## Verified vs unverified (this iteration)

- **Verified on Windows (authoring host):** i3 framing + tree parsing, toplevel
  registry semantics, IPC dispatch with and without a backend, notification/tray
  models and the single-owner decision, argument parsing of the `glaze-service`
  binary. `cargo fmt`, `cargo clippy`, `cargo test` all pass.
- **Unverified (needs Fedora):** the Unix socket transports, the live
  `wlr-foreign-toplevel` client, and the `dbus` feature (zbus adapters).
