# Kiln GNU/Linux

A hobby Debian 13 based distribution (short names: Kiln, KilnLinux) with its own desktop environment, **Glaze**.

| Directory | What it is |
|---|---|
| `glaze/` | Glaze desktop environment (Rust): compositor, shell, settings, session. MIT or Apache-2.0. |
| `kiln-apps/` | Themed forks of essential apps (Files, Terminal, Text, Calculator, Monitor, Viewer) and the Kiln Store. Each keeps its upstream licence (GPL). |
| `brand/` | Logo kit, wallpapers and generators. |
| `glaze/packaging/kilnlinux/image/` | live-build recipe for the live ISO. |
| `build/` | Build scripts for the x86_64 live ISO (see below). |

Status: early beta. Developed and tested on aarch64 in QEMU. The x86_64 build has not been tested on real hardware yet.

## Build the live ISO (x86_64)

See `build/README.md`.

## Maintained by Hexsource

This is a project of **[Hexsource](https://hexsource.mandrusia.tech/)**. View the [GitHub organisation](https://github.com/hexsourceorg) for related projects.
