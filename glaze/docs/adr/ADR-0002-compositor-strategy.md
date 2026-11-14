# ADR-0002 — Staged compositor strategy

- **Status:** Accepted
- **Date:** 2026-11-10
- **Deciders:** Glaze lead engineering

## Context

`GLAZE.md` §4.1 allows either owning a compositor or establishing "an explicit
equivalent managed compositor session with documented responsibilities" for the
early milestones. A panel that cannot observe real windows or launch real apps
is not an honest milestone (§4.3). Writing a full Smithay compositor before the
shell exists would delay the first runnable slice and risk an unusable shell.

## Decision

Adopt a **two-stage** strategy behind one abstraction:

1. **Stage 1 — managed compositor session (current).** The Glaze session launches
   an existing wlroots compositor (Sway) and the shell reads real window /
   workspace / focus state through a backend adapter. Two supported mechanisms:
   - `wlr-foreign-toplevel-management` (preferred; a standard protocol).
   - Sway IPC (fallback; richer but Sway-specific).
   The shell consumes only `glaze_common::model::{WindowInfo, WorkspaceInfo}`, so
   the backend is replaceable without touching UI code.
2. **Stage 2 — native `glaze-compositor`.** A Smithay-based compositor is
   implemented milestone by milestone (`xdg-shell`, `wlr-layer-shell`, outputs,
   input/seat, foreign-toplevel, xdg-decoration, XWayland). It publishes the
   same model types and can be selected with `--backend glaze`.

Responsibilities are explicit: in Stage 1 the external compositor owns focus,
workspaces, input and window management; Glaze owns the shell surfaces, the
launcher, task/tray affordances and configuration. In Stage 2 Glaze owns
everything and the external compositor is dropped.

## Consequences

- The first runnable Glaze shell is achievable now, and it observes **real**
  window state rather than fabricated lists.
- `glaze-compositor` currently reports `MilestoneState::Planned` for every
  protocol and its `run()` returns an explicit `Unsupported` error until the
  backend exists — no fake "working" claims.
- Packaging must support both backends; the session entry documents which one is
  selected and the install path never replaces the user's existing session.
