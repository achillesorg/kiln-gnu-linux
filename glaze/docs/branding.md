# Branding

Glaze is distro-neutral. Kiln artwork shows only on Kiln.

## Detection

`glaze_common::brand` parses `/etc/os-release` (fallback `/usr/lib/os-release`): `ID`, `ID_LIKE`, `NAME`, `PRETTY_NAME`, `VERSION_ID`, `LOGO`.
`ID=kiln` or `kiln` in `ID_LIKE` means Kiln. Anything else (Void, Debian, Fedora, Arch, missing file) gets the neutral Glaze mark and the distro name as text (`PRETTY_NAME`, else `NAME`, else "Linux").

## Config

```toml
[branding]
logo = "auto"   # auto | kiln | glaze | /path/to/image.svg
name = "auto"   # auto | literal text
```

- `auto` logo: Kiln symbol on Kiln, Glaze mark elsewhere.
- On the Start button, when `logo = auto` on a non-Kiln distro, the os-release `LOGO=` icon is used if the icon theme has it; otherwise the Glaze mark. On Kiln the Start button keeps the ink "start" glyph.
- A file path is used on the Start button, start menu header and About page; an unreadable file falls back to the Glaze mark.
- Settings > About > Branding edits both keys.

## Artwork

Glaze mark: rounded tile with one drip, 24 px grid, 1.5 px stroke. Mono `currentColor` source `apps/glaze-shell/resources/glaze-mark.svg`; full colour `glaze-symbol.svg` / `glaze-symbol-on-dark.svg` (shell resources and `apps/glaze-settings/resources/brand/`).

## Kiln-only by design (not distro-aware)

- `packaging/kilnlinux/**` (image build, Plymouth, fastfetch, profile scripts): Kiln images only.
- `assets/wallpapers` set `kiln` and `scripts/gen-wallpapers.py`: an optional selectable set; the default is `paper-hills`.
- `apps/glaze-shell/src/start_pane.rs`: `kiln-*` app ids are only first-match candidates beside generic ones (`foot`, `thunar`, ...).
- `apps/glaze-settings/src/colourscheme.rs`: `/etc/profile.d/kiln-theme.sh` probe, shown as "not installed" elsewhere.
- Wallpaper keyword and "Kiln strip" wording in `catalog.rs` / `startmenu.rs` / Appearance accent preset "Kiln red": preset names, not branding.
- Test fixtures and `crates/glaze-shell-core/src/view.rs` demo hostname.
- Docs and ADRs describing the Kiln target.

## Made distro-aware

Start button, start menu header, classic menu strip and About box title, Settings header subtitle, About page hero, Licences base-system row, `glaze-settings` usage text.
