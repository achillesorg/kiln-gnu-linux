# Glaze

A native Fedora Wayland desktop environment for `x86_64` and `aarch64`.
Windows 2000 desktop structure, rebuilt as a modern restrained claymorphic UI.

Rust core, GTK4 shell (`gtk4-layer-shell`), staged compositor (Sway → Smithay).
No Electron, no HTML, no theme packs.

## Layout

```
crates/glaze-common/       theme tokens, config, models, IPC protocol
crates/glaze-service/      app registry, search, launch, notifications/tray, control socket
crates/glaze-session/      session environment, component supervisor, session restore
crates/glaze-compositor/   native Smithay compositor + window-management policy
crates/glaze-backend/      compositor backends (Sway IPC, wlr-foreign-toplevel)
crates/glaze-shell-core/   GTK-free shell logic (launcher, panel, taskbar, tray, status feed)
crates/glaze-cli/          the `glaze` CLI
crates/glaze-desktop/      desktop icons, grid, context menus
crates/glaze-wm/           snapping, tiling, workspaces, keybindings
crates/glaze-state/        recents and frecency
crates/glaze-cache/        registry cache
crates/glaze-i18n/         locale detection and catalogues
crates/glaze-a11y/         accessibility descriptors and contrast audit
crates/glaze-power/        session/power actions with confirmation
apps/glaze-shell/          the panel (own workspace: needs system GTK4)
apps/glaze-settings/       settings app (own workspace)
packaging/fedora/         RPM spec and the opt-in Wayland session entry
scripts/                  bootstrap, build, test, run-nested, install-session
docs/                     architecture, visual spec, testing, man pages, ADRs
```

## Build and run

```bash
./scripts/bootstrap-fedora.sh   # build deps (prints what it installs)
./scripts/build.sh              # workspace + GTK apps
./scripts/test.sh               # fmt, clippy, tests, a11y audit, cross-target checks
./scripts/run-nested.sh         # panel + launcher in a nested window
```

Portable crates build and test anywhere: `cargo test --workspace`.

## Status

Verified so far is **compile-time only** — unit tests, `cargo check`/`clippy` for
`x86_64-unknown-linux-gnu` and `aarch64-unknown-linux-gnu`, and type-checking of
the GTK apps. The shell, settings app and compositor have **not been run**;
there is no runtime evidence yet for Wayland/layer-shell, Sway IPC, D-Bus
tray/notifications, portals or XWayland. See `PROJECT_STATUS.md` for the exact
list of what passes, what is unverified and why.

## Docs

`GLAZE.md` (specification) · `GOAL.md` · `AGENTS.md` · `docs/architecture.md` ·
`docs/visual-spec.md` · `docs/testing.md` · `docs/adr/`

## License

MIT OR Apache-2.0. Original artwork only; Windows 2000 is a structural
reference, not an asset source.
