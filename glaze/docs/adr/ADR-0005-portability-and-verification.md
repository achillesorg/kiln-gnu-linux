# ADR-0005 — Portability and verification strategy

- **Status:** Accepted
- **Date:** 2026-11-10
- **Deciders:** Glaze lead engineering

## Context

Glaze is authored on a Windows 11 host with no GTK4, no Wayland, no D-Bus and no
wlroots compositor. `GLAZE.md` §7.5 and §11 forbid faking runtime validation, and
§9 requires an honest validation matrix. We therefore need a strategy that makes
as much behaviour as possible **genuinely executable and verifiable here**, while
being explicit about what can only be proven on Fedora.

## Decision

Split every Glaze component along a strict **portability boundary**:

1. **Portable core (verified on the authoring host).** Logic that does not
   require the platform: colour/contrast maths, theme-token → CSS generation,
   versioned configuration and migration, the data models, the IPC protocol
   types, `.desktop` parsing, `Exec` tokenisation, registry discovery, search
   ranking, launch-plan construction, session environment planning, process
   supervision, Sway-IPC/foreign-toplevel message *parsing*, registry caching,
   launcher state, localisation catalogues, accessibility audits, argument
   parsing. These live in workspace crates and are covered by unit/integration
   tests that run with `cargo test --workspace` on any host.
2. **Linux-only runtime (must be validated on Fedora).** GTK4 widget code,
   `gtk4-layer-shell` integration, D-Bus transports, Wayland sockets, the
   Smithay compositor. These are `#[cfg(target_os = "linux")]`-gated or live in
   separate Cargo workspaces under `apps/`, so the root workspace always builds.
3. **Interface discipline.** Linux-only layers are thin shells over the portable
   core, so the *decisions* are tested even where the *rendering* cannot be. For
   example the launcher state machine (`glaze-shell-core`) is tested here; the GTK
   view merely binds it to widgets.

### Rules

- A claim in `PROJECT_STATUS.md` must name the host, architecture, exact command
  and observed result; anything not run is labelled **not run / unverified**.
- Never mark a compositor protocol `Working` unless it is implemented.
- Tests must inject environment/time (e.g. `now_unix`, explicit paths) so results
  are deterministic and never depend on the wall clock or the user's real files.
- Security-relevant behaviour (no shell interpolation of `.desktop` data) is
  asserted by tests on every host.

## Consequences

- The riskiest correctness questions (does the launcher rank/launch correctly,
  does `.desktop` data reach a shell, does config migrate, is text legible) are
  answered with real evidence today.
- GTK/compositor claims remain open until a Fedora x86_64 and aarch64 run
  happens; this is recorded, not hidden.
- Contributors on Linux get the same test suite plus the GUI build; no separate
  verification path is required.
