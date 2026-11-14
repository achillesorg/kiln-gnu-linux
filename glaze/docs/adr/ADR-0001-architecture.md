# ADR-0001 — Core architecture and language/runtime choices

- **Status:** Accepted
- **Date:** 2026-11-10
- **Deciders:** Glaze lead engineering

## Context

Glaze must be a real native Fedora Wayland desktop environment for `x86_64` and
`aarch64`, structured like a Windows 2000 desktop but rendered as a modern,
restrained claymorphic UI. `GLAZE.md` §4.1 prescribes a Rust core, a Wayland
compositor (Smithay) eventually, GTK4 + `gtk4-layer-shell` for the shell, and a
native settings app — with **no** HTML/Electron desktop.

Development currently happens on a Windows host with no GTK/Wayland toolchain.
We must still be able to *verify* real logic (not mockups) on that host.

## Decision

1. **Language:** Rust (edition 2021) across the whole suite. One Cargo workspace
   for portable crates; the GTK apps are separate workspaces under `apps/`.
2. **Portability boundary:** everything that is not inherently Linux/Wayland
   (theme tokens, configuration, models, IPC types, `.desktop` parsing, search,
   launch planning, session supervision) lives in crates that build and unit
   test on **any** host, including Windows. This is what makes honest
   verification possible while the authoring host is not Fedora.
3. **Shell UI:** GTK4 + `gtk4-layer-shell` (native toolkit styling via CSS
   generated from `glaze-common` tokens). Not a web stack.
4. **Compositor:** staged. First milestone integrates against an existing
   wlroots compositor (Sway) for real window/task state; a native Smithay
   compositor (`crates/glaze-compositor`) is implemented incrementally behind the
   same model types. See ADR-0002.
5. **IPC:** newline-delimited JSON over a Unix socket with a versioned protocol
   (`glaze-common::ipc`). No arbitrary code execution paths.
6. **Identifiers:** packages/binaries use the `glaze-` prefix; reverse-DNS
   application ids use `org.glaze.*` (e.g. `org.glaze.Shell`, `org.glaze.Settings`).
   Glaze does not impersonate Fedora, Microsoft or any other trademark.

## Repository layout

```text
Cargo.toml                # portable workspace (glaze-common, -service, -session, -compositor)
crates/glaze-common/       # tokens, config, models, ipc, paths  (portable, frozen API)
crates/glaze-service/      # app registry, search, launcher, ipc server (core portable)
crates/glaze-session/      # session env + component supervisor
crates/glaze-compositor/   # native Smithay compositor (Linux-gated)
apps/glaze-shell/          # GTK4 panel + launcher            (separate workspace)
apps/glaze-settings/       # GTK4 settings app                (separate workspace)
themes/                   # design tokens + generated CSS
assets/                   # original SVG icons
packaging/fedora/         # RPM spec + optional session entry
scripts/                  # bootstrap/build/run/test/check
docs/                     # architecture, visual spec, testing, adr/
tests/                    # cross-crate integration tests
```

The two GTK app workspaces are excluded from the root workspace on purpose:
GTK cannot build on the Windows authoring host, and including them would make
`cargo test` unrunnable there.

## Consequences

- The most important logic (launcher correctness, theme contrast, config
  migration, session environment) is verifiable on any host and is covered by
  unit tests today.
- GTK widget code and the compositor can only be *compiled* on Linux; their
  runtime validation must happen on a Fedora host (documented in
  `PROJECT_STATUS.md`, never faked).
- Keeping tokens in `glaze-common` guarantees the shell, settings app and future
  compositor decorations share one visual language.
