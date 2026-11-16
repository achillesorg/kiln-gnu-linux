# Kiln GNU/Linux

A hobby Debian 13 based distribution (short names: Kiln, KilnLinux) with the **[GlazeDE](https://github.com/achillesorg/GlazeDE)** desktop environment (Glaze).

| Directory | What it is |
|---|---|
| [`glaze/`](glaze/) | Bundled GlazeDE desktop source (Rust): compositor, shell, settings, session. Standalone upstream: **[GlazeDE](https://github.com/achillesorg/GlazeDE)**. MIT or Apache-2.0. |
| `kiln-apps/` | Themed forks of essential apps (Files, Terminal, Text, Calculator, Monitor, Viewer) and the Kiln Store. Each keeps its upstream licence (GPL). |
| `brand/` | Logo kit, wallpapers and generators. |
| `glaze/packaging/kilnlinux/image/` | live-build recipe for the live ISO. |
| `build/` | Build scripts for the x86_64 live ISO (see below). |

Status: early beta. Developed and tested on aarch64 in QEMU. The x86_64 build has not been tested on real hardware yet.

## Build the live ISO (x86_64)

See `build/README.md`.

## Maintained by Achilles

This is a project of **[Achilles](https://achilles.mandrusia.tech/)**. View the [GitHub organisation](https://github.com/achillesorg) for related projects.

## GlazeDE development

GlazeDE has its own [source repository](https://github.com/achillesorg/GlazeDE) with the complete desktop environment, documentation and CI. Kiln retains the bundled `glaze/` directory for build compatibility; the two repositories do not automatically synchronise changes.
