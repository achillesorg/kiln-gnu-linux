# Kiln apps

Forked, rebranded and themed essential apps for Kiln GNU/Linux (Debian 13 + Glaze). Local repo, ready for GitHub later (no remote configured).

| App | Directory | Upstream | Licence |
|---|---|---|---|
| Kiln Files | `kiln-files` | Thunar 4.20.2 (Xfce) | GPL-2.0-or-later |
| Kiln Terminal | `kiln-terminal` | foot 1.21.0 | MIT |
| Kiln Text | `kiln-text` | GNOME Text Editor 48.3 | GPL-3.0-or-later |
| Kiln Calculator | `kiln-calculator` | GNOME Calculator 48.1 | GPL-3.0-or-later |
| Kiln Monitor | `kiln-monitor` | GNOME System Monitor 48.1 | GPL-2.0-or-later |
| Kiln Store | `kiln-store` | new (Python, GTK4 + libadwaita) | GPL-3.0-or-later |

Each fork has a `NOTICE-KILN.md` (upstream, tag, commit, changes). Shared: `theme/` (CSS templates and loaders),
`tools/` (generators and patch scripts), `icons/`, `icon-theme/Kiln`, `etc/xdg/mimeapps.list`.
Build and install everything to `/usr/local`: `./build-all.sh`. See `DECISIONS.md`, `INTEGRATION.md`, `PACKAGING-TODO.md`.
