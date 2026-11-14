# Glaze assets (`assets/`)

Original, project-authored UI artwork for the Glaze desktop environment. Nothing
here is copied from Windows, Microsoft, GNOME, KDE or any other vendor: Windows
2000 is a *structural* reference for layout, never an asset library.

- License and provenance: [`LICENSE.md`](LICENSE.md) — `MIT OR Apache-2.0`, same
  as the Glaze code.
- Visual contract: [`docs/visual-spec.md` §5.5](../docs/visual-spec.md).

## Geometry contract

Every icon in this directory obeys the same rules so a shell panel row looks
consistent at any scale:

| Property | Value |
|---|---|
| Grid / viewBox | `0 0 24 24` |
| Default render size | consumer-controlled (Glaze renders at one size per surface; the SVG only declares a nominal 24) |
| Stroke width | `2` (optical, on the 24-grid) |
| Caps / joins | `round` |
| Fill | `none`, except deliberate solid details (status dots, charge blocks) |
| Colour | monochrome (`#000000` strokes/fills); the widget's foreground colour is applied by the consumer |
| Padding | 2–3 units on every edge, so the glyph never touches the pixel boundary |

**Monochrome-capable** means the consumer recolours the artwork: GTK symbolic
recolouring (files named `*-symbolic.svg`, drawn with the CSS `-gtk-icon-style`
foreground), a `GdkTexture` recolour, or an `ImageView` composited with the
current role colour. Glaze's own rule is to pick the colour from a
`glaze-common::theme::ThemeRole` (e.g. `text_secondary` for pinned tasks,
`accent_text` for the focused task) — never a raw hex value in a widget.

## Contents

| File | Purpose | Used by |
|---|---|---|
| `glaze-mark.svg` | the Glaze mark: clay tile + F letterform | start button, about pages, packaging |
| `icons/glaze-application-symbolic.svg` | generic application / running window | taskbar fallback when a `.desktop` icon cannot be resolved |
| `icons/glaze-settings-symbolic.svg` | settings (gear) | `glaze-settings` launcher entry, shell settings action |
| `icons/glaze-terminal-symbolic.svg` | terminal | `Terminal=true` launcher entries / terminal fallback |
| `icons/glaze-files-symbolic.svg` | files / folder | file manager action, desktop "Home" shortcut |
| `icons/glaze-power-symbolic.svg` | power | session menu: shutdown / restart / logout |
| `icons/glaze-network-symbolic.svg` | network (node graph) | tray/status when NetworkManager state is available |
| `icons/glaze-volume-symbolic.svg` | volume / audio | tray/status when PipeWire/WirePlumber state is available |
| `icons/glaze-battery-symbolic.svg` | battery | tray/status when UPower state is available |

The network/volume/battery icons are *artwork*. They do not imply that the
corresponding system integration is implemented: `GLAZE.md` §4.3 forbids showing
fake state, so the shell must only display them when a real backend reports
state (and must show a clear unsupported/absent indication otherwise).

## How the shell loads them

1. **Icon theme (preferred, packaging path).** Install `icons/` into
   `$XDG_DATA_DIRS/icons/hicolor/scalable/status/` (package-time), then ask the
   icon theme for e.g. `glaze-settings-symbolic`. This gives free HiDPI, caching
   and theme-switching behaviour.
2. **Direct resource load (development/nested mode).** Load the file through
   `gtk4::Image::from_file()` / `gdk::Texture::from_filename()` from the
   repository `assets/` directory. Files are plain standalone SVGs, so both paths
   work without modification.

Either way the consumer supplies the pixel size and the colour; the artwork is
size- and colour-agnostic.

## Adding an icon

1. Author it on the 24×24 grid with a 2px stroke, round caps/joins.
2. Keep it monochrome; no embedded brand colours, no gradients, no clipping masks
   (GTK symbolic recolouring is alpha-based).
3. Name it `glaze-<thing>-symbolic.svg` and add it to the table above.
4. Confirm it is original (or add a third-party license note in `LICENSE.md`).
