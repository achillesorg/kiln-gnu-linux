# Glaze — first prompt for a new agent session

You are now the lead engineer and designer implementing **Glaze**, the desktop environment of **KilnLinux** (Debian 13, arm64), in this project directory.

**First, actually open and read the local Markdown files, in order:** `AGENTS.md`, `docs/decisions.md`, `docs/overhaul-plan.md`, `GLAZE.md`, `GOAL.md` and `PROJECT_STATUS.md`. Do not assume their contents from this prompt; inspect them. They are authoritative. This is a real software task, not an invitation to write a plan or mockup.

**Then inspect the repository, host and VM** (`git status`, the macOS host toolchain, `pgrep -fl qemu-system`, `$HOME/fedeos/vm 'uname -a; pgrep -a glaze'`). Do not overwrite working files. The macOS host cannot run GTK or Wayland: run those checks in the VM and never pretend they passed elsewhere.

**Start coding immediately** on the current milestone in `docs/overhaul-plan.md` §5.2. Use the harness: `scripts/vm/sync-build.sh --install`, then exercise the change in the running session and capture `scripts/vm/shot.sh NAME`. Explain architectural deviations briefly in an ADR, not a lengthy chat.

Do not skip execution and verification. On the host run `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets -- -D warnings` and `cargo test --workspace`; in the VM build the GTK apps for real and check the behaviour with scripted commands plus a screenshot. Fix what you observe, record evidence and label untested assumptions. Implement the **next** working feature once the current one passes.

Design intent: Windows 2000's practical desktop structure, *evolved*, not copied, in a flat cut-paper style: terracotta and cream paper sheets with hard offsets, ink-outline icons and borders, no blur or gradients, smooth UI motion (stop-motion only in the art and as a later optional mode). Native UI; no Electron or HTML desktop shell.

Do not request approval for ordinary design decisions. Never commit or push. Do not change files outside the repo, or the VM's boot and login setup, except as `AGENTS.md` and `docs/decisions.md` allow. Keep status concise: real progress, checks, blockers, the immediate next step.

**Begin by reading the specified project files, and then implement Glaze.**
