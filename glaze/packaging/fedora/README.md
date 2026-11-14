# Glaze on Fedora — packaging and opt-in session

This directory contains the Fedora packaging for Glaze and the **opt-in** Wayland
session entry.

> **Safety rule.** Installing or enabling Glaze **never** replaces your existing
> desktop session, edits your login manager (GDM/SDDM/LightDM) configuration, or
> changes the bootloader. `glaze-session.desktop` is only ever *added* to
> `/usr/share/wayland-sessions/`, where the display manager offers it as one more
> choice. Removing the package removes it again.

## Files

| File | Purpose |
|---|---|
| `glaze.spec` | RPM spec: builds the Rust crates + GTK4 apps; installs the binaries (including the `glaze` CLI), man pages, completions, themes, assets, the Settings app entry and the opt-in session entry. |
| `glaze-session.desktop` | The Wayland session entry (`Name=Glaze`, `Exec=glaze-session run`, `DesktopNames=Glaze`). |
| `glaze-settings.desktop` | Application entry for `glaze-settings` (`Categories=Settings;DesktopSettings;`). |
| `../scripts/install-session.sh` | **No-root** user install / uninstall of the session, with an exact-removal manifest. |

## Install for one user (no root, no RPM)

If you would rather not build an RPM, a documented local install registers the
session for your user only:

```bash
scripts/build.sh --release                 # build the binaries first
scripts/install-session.sh --release       # -> ~/.local (use --prefix to change)
# log out, then pick "Glaze" at the login screen
scripts/install-session.sh --uninstall     # removes exactly what it installed
```

It writes a manifest of every file it creates, and `--uninstall` refuses to run
without one, so it can never delete files it did not install. As with the RPM it
never edits the login manager, your default session or the bootloader.

## Build the RPM (on Fedora)

```bash
# From the repository root, on a Fedora host with rpmbuild tooling:
sudo dnf install rpm-build rpmdevtools rust-packaging cargo gtk4-devel gtk4-layer-shell-devel

# Lay out a build tree and build:
rpmdev-setuptree
# Point Source0 at a release tarball of this repo, or use a local branch build:
rpmbuild -ba packaging/fedora/glaze.spec
```

The spec is **not** yet validated with `rpmbuild` on Fedora (it was authored on a
non-Fedora host). Verify the `BuildRequires`/`Requires` package names — especially
`gtk4-layer-shell-devel` — against your Fedora release before relying on it.

## Opt in to the Glaze session

1. Install the package (`sudo dnf install ./glaze-*.rpm`).
2. **Log out**, and at the login screen pick the session named **Glaze** from the
   session menu.
3. Log in. To leave, log out and pick your previous session again.

### Developer / nested run (no session change at all)

The safest way to try the shell is a nested run inside your current session, which
touches no system configuration:

```bash
glaze-session run --backend sway --nested      # Sway in a window + Glaze shell
```

## What Glaze does when it starts

`glaze-session run` exports the Wayland session environment
(`XDG_CURRENT_DESKTOP=Glaze`, toolkit backends, …) and supervises the component
graph (compositor, `glaze-service`, `glaze-shell`). `glaze-service` binds
`$XDG_RUNTIME_DIR/glaze/control.sock` (mode `0600`) and serves the shell. On a
session that already runs a notification daemon or tray host, Glaze **refuses to
claim the well-known D-Bus names twice** and stands down (see
`docs/adr/ADR-0004-system-integration.md`).

## Uninstall / revert

```bash
sudo dnf remove glaze
```

Your other desktop sessions are untouched.
