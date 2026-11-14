# glaze-settings

The Glaze settings application for Kiln GNU/Linux: a native GTK4 control panel
bound to the live Glaze configuration. The user and developer documentation is in
[`docs/settings.md`](../../docs/settings.md). Schema requests to the shell and
compositor are in [`docs/settings-needs.md`](../../docs/settings-needs.md).

This crate is its own Cargo workspace (excluded from the root one). GTK is a
Linux-only dependency, so `cargo test` runs the portable core (config store,
catalog, search, validation, keybindings, theme import/export, system-tool
parsers, desktop-entry and mimeapps handling) on any host. The window builds and
runs only on Linux (GTK 4.14 or later; Kiln/Debian 13 ships 4.18).

```
cargo test && cargo fmt --check                          # any host
cargo build --release                                    # Kiln VM
cargo clippy --release --all-targets -- -D warnings      # Kiln VM
scripts/vm-smoke.sh                                      # headless Sway smoke test
scripts/vm-shots.sh OUTDIR light dark classic            # screenshots of every page
```

Module map: `store` (toml_edit, atomic writes, backups, undo), `catalog`
(pages and setting ids), `search`, `validate`, `keys` (keybinding defaults and
conflicts), `themeio`, `sysinfo` (wpctl/pactl, nmcli, upower/sysfs,
swaymsg/wlr-randr parsers), `desktop_apps`, `assets` (embedded ink icons,
brand marks and texture tiles), `style` (CSS from glaze-design tokens), `cli`,
and `ui/` (GTK window, widgets and one module per page).
