# ADR-0007 — glaze-compositor structure: anvil layout, policy in portable code

- **Status:** Accepted (M3, nested)
- **Date:** 2026-11-12
- **Deciders:** compositor workstream (agent B), per `docs/decisions.md` Q1

## Context

`docs/decisions.md` Q1 chose an own Smithay compositor with Sway as fallback.
Until now `glaze-compositor` was a protocol server that drew nothing. M3 needs
a real compositor (rendering, input, stacking windows, layer-shell for the
shell), plus the user's priorities: a stacking <-> tiling toggle, Glaze-drawn
paper frames, smooth animations, and a taskbar that works without Sway.

## Decision

1. **Structure after Smithay's MIT `anvil` example**, trimmed: one `State`,
   one handler file per protocol, a `BackendData` enum (winit now, udev next),
   grabs in `input.rs`, element building in `render.rs`.
2. **Policy is portable and unit-tested** (macOS host): `wm.rs` (windows,
   focus, workspaces, minimise/maximise, Alt+Tab), `glaze-wm::layout_mode`
   (tiling toggle, dwindle), `frame.rs` (frame geometry, hit-testing, the CPU
   title painter), `anim.rs` (tweens), `control.rs` (socket protocol). The
   Smithay side only mirrors `wm` onto surfaces in `State::apply_layout()`.
3. **Renderer:** concrete `GlesRenderer` for now (winit only supports GLES;
   the DRM backend will also use GLES via GBM). Render elements are an enum
   from `render_elements!`. A pixman path, if needed on DRM, is a backend
   concern and would require making `render.rs` generic again.
4. **Server-side frames are composited, not sub-surfaces:** the title bar is
   a CPU-painted `MemoryRenderBuffer` (repainted only on change), hairlines
   and shadow are `SolidColorRenderElement`s with stable ids, so damage
   tracking stays effective on llvmpipe.
5. **Taskbar protocol:** serve `zwlr_foreign_toplevel_manager_v1` (hand-written
   `GlobalDispatch`/`Dispatch`, Smithay 0.7 has no helper) alongside
   `ext_foreign_toplevel_list_v1`, so the same shell code works on Sway and
   on glaze-compositor. The tiling toggle uses the compositor's own control
   socket (same JSON framing as `glaze-service`), not a new Wayland protocol.
6. **Close animations** snapshot the dying window's textures in
   `toplevel_destroyed` (before Smithay drops them) and animate a "ghost".

## Consequences

- The shell stays compositor-agnostic (ADR-0002 holds).
- Most behaviour is testable on the host; the Linux code is thin glue, but it
  can only be compiled and run in the VM (no Linux target on the Mac host).
- Screencopy is not served yet, so screenshots are taken of the parent
  session in nested mode.
