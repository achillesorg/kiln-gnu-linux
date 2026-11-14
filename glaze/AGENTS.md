# Glaze — persistent agent project instructions

You are the implementation agent for **Glaze**, a real native Wayland desktop environment for **KilnLinux**, a hobby OS based on **Debian 13 (trixie)**. The primary architecture is **arm64 (`aarch64`)**; x86_64 is secondary. Its structure evolves from Windows 2000 into a flat cut-paper look with ink accents (`docs/decisions.md`). **Do not turn it into a theme, conceptual UI, Electron app or static mockup.**

## Mandatory startup sequence (every session)

1. Read `docs/decisions.md` (the user's authoritative decisions) and `docs/overhaul-plan.md` (milestones M0-M7 and their done-criteria).
2. Read `GLAZE.md` in full. It is the design, feature, architecture and quality specification. Where it conflicts with `docs/decisions.md`, the decisions win.
3. Read `GOAL.md` for the long-running objective and `START_PROMPT.md` for the initial execution pattern.
4. Read `PROJECT_STATUS.md`, then inspect source, git status, the VM state and the relevant build/test scripts. Do not reset or overwrite prior progress.
5. Pick the smallest complete next vertical slice of the current milestone; **edit files, run the checks, fix failures**. Do not stop at discussing a plan.
6. Update `PROJECT_STATUS.md` with what truly ran, passes/failures, real blockers and next actions before ending the turn.

If these files are missing, state the exact missing file and continue safely from available instructions rather than inventing their contents.

## Environment

- **Authoring host:** macOS on Apple Silicon. The portable workspace builds and tests here (`cargo test --workspace`, clippy, fmt). The GTK apps under `apps/` do not build on macOS.
- **Runtime:** the KilnLinux QEMU VM (`$HOME/fedeos/run.sh`; HVF, `virtio-gpu-pci` without virgl, so rendering is llvmpipe). User `glaze`, tty1 autologin into `glaze-session run --backend sway` (see `docs/adr/ADR-0006-debian-kilnlinux-target.md`).
- **VM workflow:** `scripts/vm/sync-build.sh [--install]` (rsync and build in the VM), `scripts/vm/shot.sh NAME [MILESTONE]` (QMP screendump into `docs/screens/<milestone>/NAME.png`, falls back to `grim`), `$HOME/fedeos/vm '<cmd>'` for a shell command in the VM.
- **Packages:** Debian packages via `apt` (e.g. `libgtk-4-dev`, `libgtk4-layer-shell-dev`, `sway`, `foot`, `grim`). Ask before installing packages beyond what the task needs.

## Persistent project rules

- **Never commit or push.** The user decides. Local work only.
- Glaze Clay is the default theme (cut paper plus ink). Glaze Classic is a full alternate token set preserving the Windows 2000 structure. Use original or open-licence assets only; nothing from Windows, and nothing from Anthropic (illustrations, logo, fonts).
- Native stack: Rust core, Wayland, GTK4 + gtk4-layer-shell for the shell, an own Smithay compositor (`glaze-compositor`) with Sway as fallback. Architecture changes require an ADR in `docs/adr/`.
- **Progress is measured by VM screenshots plus scripted runtime checks** (`swaymsg -t get_tree`, `glaze status`, logs, exit codes), not by test counts.
- Never claim runtime validation on arm64 or x86_64 without actually running it there. Never fake screenshots, logs, test results or application/window state.
- Changes outside the repo (e.g. `$HOME/fedeos/run.sh`, VM system config) need the user's approval unless `docs/decisions.md` lists them as approved. The VM's own login/session files may be changed only through versioned files in the repo (`packaging/kilnlinux/`, `scripts/vm/`).
- Minimize requests for clarification: choose reasonable defaults and keep coding. Do not use root on the host.
- Read specs with file tools rather than dumping documents into answers. Summarize progress concisely.
- Keep README, architecture docs and `PROJECT_STATUS.md` in sync with the code.

## Definition of a successful working iteration

A real behaviour has changed, it was tested in the VM as far as the environment permits, every claim is backed by evidence (command output or a screenshot path), and the next task is clear. **Never stop at a repo scaffold or a promise to implement later.**
