# glaze-shell

The Glaze panel — a bottom (or configured-edge) Wayland taskbar with a real
application launcher, taskbar, tray zone, notification badge and clock, written
in Rust with **GTK4 + gtk4-layer-shell**.

This is the first runnable Glaze shell milestone (`GLAZE.md` §5 P0). It is its own
Cargo workspace (see the `[workspace]` table in `Cargo.toml`) because GTK4 only
builds on Linux/Wayland, so it is deliberately excluded from the portable root
workspace (`docs/adr/ADR-0001-architecture.md`).

## What it does today

- **Panel** anchored to the configured screen edge via `wlr-layer-shell`, with an
  exclusive zone equal to the theme's panel height so maximised windows are not
  covered. Styling comes entirely from `glaze-common` tokens
  (`ThemeTokens::to_gtk_css_with`), so Glaze Clay / Glaze Classic and light / dark
  all work from configuration.
- **Launcher** popover attached to the **Glaze** start button: inline search over
  the real XDG `.desktop` registry (`AppRegistry::discover`). Results are ranked
  by `glaze-service::search`; Enter (or a click) launches through
  `glaze-service::launch`, which tokenises `Exec` itself and never uses a shell.
  Escape closes; Up/Down navigate with wrapping; the highlighted row carries the
  `glaze-selected` class. All of this state lives in `glaze-shell-core::launcher`,
  which is unit-tested on any host.
- **Taskbar** rendered from `glaze-shell-core::taskbar::merge_tasks`: one button
  per merged entry, ordered pinned-first then running-first-seen. Pinned, running
  and focused are **separate CSS classes** (`glaze-pinned`, `glaze-running`,
  `glaze-focused`) so "merely pinned" and "actually open" are unmistakable
  (`GLAZE.md` §6.1). Labels appear per `panel.show_labels`, gated by
  `taskbar::should_show_label` against the panel's real allocated width.
- **Real window state** comes from a compositor backend attached at startup
  (`src/backend.rs`): `session.compositor = sway` uses `$SWAYSOCK`,
  `layer-shell` uses `wlr-foreign-toplevel-management`, `auto` tries both in
  turn. If nothing can attach — or the backend cannot report windows — the
  taskbar shows pinned apps only and logs why. **No window list is ever
  fabricated** (`GLAZE.md` §4.3, §7.2).
- **Tray zone** built from `glaze-shell-core::tray::TrayModel`, with the shared
  `overflow()` policy deciding when items go into a `»` menu. The model is empty
  today because the StatusNotifier watcher is not wired yet — nothing is faked.
- **Notification badge** from `glaze-shell-core::notifications::NotificationBadge`
  (hidden at zero unread, `99+` capped, suppressed by Do-Not-Disturb). Nothing
  feeds it yet, so it is hidden at startup.
- **Clock** label updated every second from the real system clock and formatted
  by `glaze-shell-core::clock` (honours `clock_24h`, `show_seconds`, `show_date`
  and `date_format`).
- **Localised labels** via `glaze_i18n::tr` for the start tooltip, launcher
  placeholder and empty-result text.
- **Degraded mode**: if `gtk4-layer-shell` is unsupported (or `--windowed` is
  passed) the shell runs as a normal decorated window — useful for nested
  testing. The chosen path is always logged to stderr.

## Known limitations (honest, not hidden)

- **Tray items / notifications are models only.** The panel renders the real
  `glaze-shell-core` models, but the D-Bus StatusNotifier watcher and notification
  server are not attached yet, so both are empty at runtime. No icons or counts
  are invented.
- **Clicking a running task focuses it only when a backend can.** Sway IPC can
  activate windows; the foreign-toplevel backend currently reports
  `Unsupported`, so clicks fall back to launching the app.
- The **show-desktop** affordance and the desktop surface are not implemented;
  the zone is omitted rather than drawn as a dead button.
- Window labels are decided from the panel's allocated width with a fixed
  reserved allowance for the other zones; GTK still elides labels that do not
  fit. Verify exact alignment on a real compositor.
- The exclusive zone is set to the theme panel height; if you change
  `.glaze-panel` padding in the generated CSS the visual height and the reserved
  space can drift.
- Windowed fallback cannot truly stay "always on top"; GTK4 has no such API.
  Only the layer-shell path reserves space and stays above tiled windows.
- Only **compile-verified** upstream; this host has no GTK sysroot. Runtime
  validation requires a Fedora/Wayland host. See `PROJECT_STATUS.md`.

## Build (Fedora)

Runtime and build dependencies:

```bash
sudo dnf install gtk4-devel gtk4-layer-shell-devel pkgconf-pkg-config rust cargo
```

Then:

```bash
cd apps/glaze-shell
cargo build --release
```

The binary is `target/release/glaze-shell`.

> The dependency trio is pinned to the coherent gtk-rs generation
> `gtk4 0.9` / `glib 0.20` / `gtk4-layer-shell 0.5`. Newer coherent generations
> exist (`gtk4 0.11` / `glib 0.22` / `gtk4-layer-shell 0.8`); bump all three
> together if you need them.

## Run nested (non-destructive)

Inside an existing Wayland session that implements `wlr-layer-shell` (Sway,
Hyprland, niri, a nested Sway in a window, ...):

```bash
cd apps/glaze-shell
cargo run --release
```

Force the plain-window fallback (useful under compositors without layer-shell):

```bash
cargo run --release -- --windowed
```

Options:

```text
--windowed      Force a normal decorated window (skip gtk4-layer-shell)
-h, --help      Print help
-V, --version   Print the version
```

Stop the shell with `Ctrl+C` (handled cleanly via the GLib main loop) or by
closing the windowed fallback.

## Configuration

Configuration is read from `$XDG_CONFIG_HOME/glaze/config.toml`
(`~/.config/glaze/config.toml`), with defaults used when the file is absent.
Relevant keys: `appearance.theme` (`clay` / `classic`), `appearance.scheme`
(`light` / `dark`), `appearance.reduced_motion`, `panel.position`
(`bottom` / `top` / `left` / `right`), `panel.show_labels`, `panel.clock_24h`,
`panel.show_seconds`, `panel.show_date`, `panel.date_format`,
`launcher.search_limit`, `launcher.pinned` (list of desktop ids),
`session.compositor` (`auto` / `sway` / `layer-shell` / `glaze` / `none`).

## Verify

```bash
cd apps/glaze-shell
cargo fmt --check
cargo clippy --all-targets
cargo build
```

The portable logic it depends on is tested from the repository root:

```bash
cargo test -p glaze-shell-core
```
