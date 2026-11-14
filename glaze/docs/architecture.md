# Glaze architecture

Glaze is a native Fedora Wayland desktop environment. This page is the short,
current map of the system. Decisions live in `docs/adr/`.

## Component graph

```text
                 +-----------------------------+
                 |   glaze-session (supervisor) |  sets XDG_* env, starts & watches
                 +--------------+--------------+
                                | starts
             +------------------+-------------------+
             |                                      |
    +--------v---------+                 +-----------v-----------+
    |  compositor      |                 |  glaze-shell (GTK4)    |
    |  (Sway now,      |<--- window/ --> |  panel, launcher,      |
    |   Smithay later) |     workspace   |  tray, clock, desktop  |
    +------------------+     state       +-----------+-----------+
                                                      | IPC (unix socket, JSON)
                                          +-----------v-----------+
                                          |   glaze-service        |
                                          | registry, search,     |
                                          | launch, notifications |
                                          +-----------+-----------+
                                                      |
                                          +-----------v-----------+
                                          |   glaze-common         |
                                          | tokens, config, model |
                                          +-----------------------+
```

## Crates

| Crate | Portability | Responsibility |
|---|---|---|
| `glaze-common` | portable | theme tokens + CSS, configuration, models, IPC types, XDG paths |
| `glaze-service` | core portable, IPC/backends Linux | `.desktop` registry, search ranking, safe launch, control socket |
| `glaze-session` | portable core | session environment, component supervisor |
| `glaze-compositor` | Linux | native Smithay compositor (staged; see ADR-0002) |
| `apps/glaze-shell` | Linux/GTK4 | panel, start button, launcher, taskbar, tray, clock, desktop |
| `apps/glaze-settings` | Linux/GTK4 | appearance / panel / wallpaper settings |

## Data flow for launching an app

1. User types in the launcher (shell).
2. Shell asks `glaze-service` (`SearchApps`) over the control socket.
3. `glaze-service::search` ranks **real registry entries**; no synthetic results.
4. On Enter the shell sends `LaunchApp { id }`.
5. `glaze-service` looks the id up, builds argv via `glaze-service::desktop`
   (field codes expanded, **never** a shell), and spawns it detached.
6. Window state returns via the compositor backend and updates the taskbar.

## Theme single-source

`glaze-common::theme::ThemeTokens` is the only place colour and metric values
exist. `to_gtk_css()` emits `@define-color glaze_*` roles plus component rules
for the panel, launcher, menu, task buttons, tray and desktop. The shell and the
settings app load that string into a `GtkCssProvider`. Tests assert every role is
defined, no raw hex leaks into component CSS, and shell text meets WCAG AA.

## Security

- `.desktop` data is parsed and tokenised in `glaze-service`; it is never passed
  to `sh -c`. There is a test that asserts `evil ; rm -rf /` becomes inert argv.
- The control socket lives in `$XDG_RUNTIME_DIR/glaze/control.sock`, mode `0600`.
- The install/session path never edits a login manager or the bootloader; the
  nested developer mode runs inside a window.
