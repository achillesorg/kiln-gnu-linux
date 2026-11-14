# Integration proposals for the lead (Glaze repo not touched)

## Default pinned apps (`panel.default_pins` / `launcher.pinned`)
Use desktop ids: `org.kiln.Files.desktop`, `org.kiln.Terminal.desktop`, (browser), `org.kiln.Text.desktop`.
Windows group with their launcher because the Wayland app id equals the desktop file id without `.desktop`
(`org.kiln.Files`, `org.kiln.Terminal`, `org.kiln.Text`) and `StartupWMClass` is set to the same value.
Kiln Terminal client windows use app id `org.kiln.Terminal.client`.

## Default apps
Merge `kiln-apps/etc/xdg/mimeapps.list` into `/etc/xdg/mimeapps.list` (replaces thunar.desktop and org.gnome.TextEditor.desktop).
Glaze Settings > Default apps should list the `org.kiln.*` desktop files.

## Colour scheme hooks
- Terminal: `glaze-colour-scheme` calls `kiln-terminal-scheme dark|light` (done by the lead).
- GTK3/libadwaita Kiln apps follow gsettings `color-scheme` live, no hook needed.

## Packaging
- `kiln-apps/build-all.sh` builds everything into /usr/local. Install `/usr/local/share/icons/Kiln` (icon theme) first.
- Terminal needs Debian `foot-terminfo`. Files re-uses Debian's thunar archive plugin via a symlink in /usr/local/lib/thunarx-3.
- Remove Debian's `thunar`, `foot`, `gnome-text-editor`... from packages.list once the Kiln apps are the defaults (or keep them hidden: `NoDisplay=true` override).
