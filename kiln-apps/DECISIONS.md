# Kiln apps: decisions (agent E, defaults chosen autonomously)

| Topic | Decision |
|---|---|
| Repo layout | One local git repo (`kiln-apps`, no remote). Each fork is a plain source tree (upstream `.git` dropped) with `NOTICE-KILN.md` stating upstream, tag and commit. Shared pieces: `theme/` (CSS templates + loaders), `tools/` (generators, patch scripts), `icons/` (app icons), `icon-theme/Kiln` (folder/place icon theme, inherits Adwaita), `etc/`. |
| Pinned upstream | The tag matching Debian trixie's packaged version: Thunar 4.20.2, foot 1.21.0, GNOME Text Editor 48.3. |
| Install | Built in the VM with the upstream build system into `/usr/local` (never touches Debian packages). Binaries `kiln-files`, `kiln-terminal`, `kiln-text`, ...; ids `org.kiln.<Name>`. |
| Names | Product names "Kiln Files/Terminal/Text/Viewer/Calculator/Monitor/Store"; no GNOME/Xfce in names; About dialogs say "Based on X <version>". Translations of changed strings fall back to English. |
| Theme tokens | Light: bg #FAF9F5, raised #FCFBF7, panel #F5F2EA, hairline #CFC6B4, ink #2B2926, muted #6B665C, accent #D97757 (dark text #2B1A12), selection #F2D5C6. Dark: bg #2B2926, raised #33302C, sunken #24221F, hairline #4A4640, ink #F3EFE6, muted #B3AC9F, selection #584035. Single source: `tools/gen-css.py`. |
| Light/dark | GTK3 (Files): loader follows gsettings `color-scheme`. libadwaita apps: loader follows `AdwStyleManager` dark. Terminal: reads `~/.config/glaze/colour-scheme`, live switch by SIGUSR1/SIGUSR2 (`kiln-terminal-scheme`, called by glaze-colour-scheme). `KILN_SCHEME=light|dark` forces a scheme for testing. |
| libadwaita theming | Override libadwaita's named colours plus a small rule set (hairlines, 5-6px radii, no shadows, terracotta focus ring); no widget rewrites. CSS loaded at USER priority from `/usr/local/share/kiln-<app>/`. |
| Fonts | UI: Inter (installed). Terminal: Noto Sans Mono (JetBrains Mono not installed; no extra downloads). |
| Icons | Own flat paper app icons (generated, `tools/gen-icons.py`); icon theme `Kiln` set by each app for folders/places; other icons fall back to Adwaita. |
| Not done by design | No push, no GitHub, no edits to the Glaze repo; integration changes are proposals in `INTEGRATION.md`. |

## Status notes (end of session)
- Kiln Viewer (Loupe 48.1, commit 49f38084): NOT built. `tools/patch-viewer.py` (rust theme loader `kiln_theme.rs`, rename, forced-dark removed) is written but untested, and was never applied to the repo; the Loupe checkout is in ~/kiln-apps-build/loupe in the VM. `apt-get build-dep loupe` was still unpacking (IO-starved by the parallel compositor build) when I stopped. Crates.io download is now authorised by the lead (cargo with rustup toolchain, CARGO_HOME inside the build dir, keep out of git). Older note: Loupe is Rust and needs ~200 crates (glycin etc.); downloading crates.io is not authorised and Debian's librust packages were not checked. Defer; Debian's `loupe` stays the default image viewer until then. Plan: `apt-get build-dep loupe` + build offline with Debian's /usr/share/cargo/registry, then `tools/rebrand-gnome.py` + the shared `theme/adw.css.in` loader (needs a Rust-side CSS provider instead of the C loader).
- Kiln Monitor: built and installed; light and dark screenshots verified (screens/monitor-1.png, monitor-light.png).
- Kiln Store: runs, lists suggestions; apt search and install/remove of `cowsay` verified with the same apt-get command line but via `sudo -n` (pkexec needs a password prompt that could not be answered here). Flatpak listing came back empty: the Flathub appstream data was not available (`flatpak update --appstream` produced nothing within 100 s), so Flatpak search/install is unverified.
- Kiln Files: home icon fixed and verified (screens/files-about.png shows it); About dialog strings confirmed in the binary but the dialog was not opened.
