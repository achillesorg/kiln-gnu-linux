# Glaze theme files (`themes/`)

The tokens live in `crates/glaze-design` (`theme.rs` = built-in Clay/Classic
light/dark values, `css.rs` = GTK CSS, `overrides.rs` = this file format).
The files here are **real, runtime-read theme files** (design system v2):
`scripts/install-session.sh` installs them to `~/.local/share/glaze/themes/`
and the shell loads the one named by `appearance.theme_file`.

| File | What it is |
|---|---|
| `clay.toml` | Glaze Clay written out as a theme (the default values). |
| `classic.toml` | Glaze Classic (base = classic). |
| `ember.toml` | Example shareable variant: darker accent, 44 px bar, 1.25 px strokes. |

## Load order

```text
built-in tokens (appearance.theme + scheme)
  < theme file (appearance.theme_file; its [meta] base wins over appearance.theme)
  < [appearance.overrides]  (same shape as a theme file)
  < shortcuts: appearance.accent / radius / stroke_weight / shadow_strength,
    panel.density / panel.height, appearance.font_scale
```

Search path for a bare name: `~/.config/glaze/themes/`, `~/.local/share/glaze/themes/`,
then `$XDG_DATA_DIRS/glaze/themes/`. A value containing `/` is a path.

## Format

Sections: `[meta]` (name, schema, base, author, license, description),
`[palette]` / `[palette.light]` / `[palette.dark]` (any colour role:
canvas, surface, surface_raised, surface_sunken, border, text_primary,
text_secondary, accent, accent_text, focus, danger, success, warning, shadow,
highlight, panel, ink, accent_soft, accent_soft_text), `[metrics]`,
`[typography]`, `[shadow]`. Ranges are in `overrides.rs` (`INT_METRICS`,
`FLOAT_METRICS`).

Validation: every bad key/value is reported with its path (e.g.
`error: metrics.panel_height: 300 is outside 24..=96; ignored`) and the rest
of the file still applies. Clay shadows are forced to zero blur. Setting an
accent without `accent_text` derives a readable text colour. Contrast below
WCAG AA on the core pairs is a warning. `cargo test -p glaze-common` checks that
every file here loads with zero issues.
