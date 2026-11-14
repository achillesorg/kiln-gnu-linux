# Glaze manual pages

This directory holds the roff (`.1`) manual pages that ship with Glaze:

| Page | Command |
|---|---|
| `glaze.1` | `glaze` — command-line client (work in progress) |
| `glaze-session.1` | `glaze-session` — session environment and component supervisor |
| `glaze-service.1` | `glaze-service` — registry, launcher and local IPC service |
| `glaze-shell.1` | `glaze-shell` — GTK4 panel, launcher, clock and taskbar |
| `glaze-settings.1` | `glaze-settings` — GTK4 settings application |
| `glaze-compositor.1` | `glaze-compositor` — native Wayland compositor |

Each page describes the command **as it actually is**: where a binary is not
built yet, or the native compositor returns an `unsupported` error, the page
says so explicitly rather than documenting aspirational flags. Keep it that way
when you edit a page — update the page from the source, never the source from
the page.

## Installing the pages

There is intentionally no makefile or build step; the pages are plain roff and
are installed by copying them into a `man1` directory. Nothing here writes to a
login manager, a display manager or any other system configuration.

### System-wide (needs write access to `/usr/share/man`)

```sh
# from the repository root
sudo install -d /usr/share/man/man1
sudo install -m 0644 docs/man/*.1 /usr/share/man/man1/
# refresh the manual index (Fedora ships mandb)
sudo mandb
```

(`install` is part of `coreutils`; `mandb` is in the `man-db` package.)

### Per user (no root)

```sh
# from the repository root
mkdir -p "$HOME/.local/share/man/man1"
cp docs/man/*.1 "$HOME/.local/share/man/man1/"
mandb -u    # optional; man also finds the pages without an index
```

Make sure `$HOME/.local/share/man` is on your `MANPATH`. On most Fedora setups
it already is; if `man glaze-session` reports nothing, add:

```sh
export MANPATH="$HOME/.local/share/man:$MANPATH"
```

### Reading without installing

```sh
man ./docs/man/glaze-session.1
# or, if you prefer a pager command:
groff -Tutf8 -man docs/man/glaze-session.1 | less -R
```

## Uninstalling

```sh
sudo rm /usr/share/man/man1/{glaze,glaze-session,glaze-service,glaze-shell,glaze-settings,glaze-compositor}.1
# per-user:
rm "$HOME/.local/share/man/man1"/{glaze,glaze-session,glaze-service,glaze-shell,glaze-settings,glaze-compositor}.1
```
