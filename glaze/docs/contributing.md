# Contributing to Glaze

Thanks for working on Glaze. This page is the practical companion to
[`AGENTS.md`](../AGENTS.md) (working rules) and [`GLAZE.md`](../GLAZE.md) (the
authoritative specification). Read those first; this page tells you how to
build, test and extend the code safely.

---

## 1. Prerequisites

- **Rust**, stable toolchain (edition 2021). The suite is currently verified
  with `rustc` 1.99.
- **Fedora Linux** if you want to build or run anything with a GTK4 or Wayland
  dependency (the `apps/` shells and the compositor backend).
- The **portable crates build and unit-test on any host**, including Windows.
  That is deliberate: it is how real logic can be verified while the authoring
  host is not Fedora. See
  [`docs/adr/ADR-0001-architecture.md`](adr/ADR-0001-architecture.md).

### Fedora build dependencies (GTK apps)

Install these system packages before building the GTK apps (names are the
Fedora package names; verify against your Fedora release):

```sh
sudo dnf install gtk4-devel gtk4-layer-shell-devel
```

Everything else the portable crates need comes from `cargo`.

### Helper scripts

The `scripts/` tree provides thin, non-destructive wrappers. Run them from the
repository root:

```sh
scripts/bootstrap-fedora.sh   # check/install Fedora build deps
scripts/build.sh              # build the workspace (and apps/ on Linux)
scripts/check.sh              # readiness report for this host
scripts/test.sh               # fmt + clippy + tests (+ app builds on Linux)
scripts/run-nested.sh         # run the shell nested inside a window
scripts/install-dev.sh        # per-user developer install
```

They print what they do and never touch a login manager, display-manager
configuration or boot settings; read a script before running it if you are
unsure. `scripts/check.sh` is the fastest way to learn what your host is
missing.

---

## 2. Build and test

The scripts wrap these commands, but the direct invocations are the source of
truth:

```sh
# Build and run every test in the portable workspace.
cargo test --workspace

# Check formatting (CI-style; does not modify files).
cargo fmt --all -- --check

# Lint every target, treating warnings as errors (as scripts/test.sh does).
cargo clippy --workspace --all-targets -- -D warnings

# Build a single portable crate while iterating.
cargo test -p glaze-service
cargo test -p glaze-cli
```

The GTK applications live in **separate Cargo workspaces** under `apps/` and are
excluded from the root workspace on purpose (GTK cannot build on the Windows
authoring host). Build them on Fedora:

```sh
cd apps/glaze-shell && cargo build
cd apps/glaze-settings && cargo build
```

Formatting is enforced by `rustfmt.toml` at the root. Keep `cargo fmt`,
`cargo clippy` and `cargo test --workspace` clean before you finish a change,
and record the real result in `PROJECT_STATUS.md`.

---

## 3. Crate map

The root workspace (`Cargo.toml`) contains the portable crates. The `apps/` GTK
workspaces are excluded.

| Crate | Portability | Responsibility | Status |
|---|---|---|---|
| `crates/glaze-common` | portable | Colour type + WCAG maths, theme tokens + GTK CSS, versioned config, data models, IPC types, XDG paths | **Implemented, tested** |
| `crates/glaze-service` | core portable; socket Linux-only | `.desktop` registry, desktop-entry parsing, search ranking, safe launch, IPC server | **Implemented, tested** (library; daemon binary pending) |
| `crates/glaze-session` | portable | Session environment plan + component supervisor with bounded respawn | **Implemented, tested** |
| `crates/glaze-cli` | portable parse; transport Linux-only | The `glaze` command-line client (`apps`, `search`, `launch`, `exec-line`, `status`, `config get/set`, `ipc`) | **Implemented, tested** |
| `crates/glaze-shell-core` | portable | GTK-free shell logic: panel ordering/overflow, launcher state machine, clock formatting, task grouping | **Implemented, tested** |
| `crates/glaze-state` | portable | Launcher state: recently used apps, frecency ordering, pinned-id resolution | **Implemented, tested** |
| `crates/glaze-backend` | core portable; socket Linux-only | Compositor backends (`foreign_toplevel`, `sway`) behind one `Backend` trait | **Implementations done; Unix transport Linux-only** |
| `crates/glaze-cache` | portable | On-disk application-registry cache with signature-based invalidation | **Implemented, tested** |
| `crates/glaze-i18n` | portable | Locale detection + small embedded message catalogue | **Implemented, tested** |
| `crates/glaze-a11y` | portable | Accessible roles/names + WCAG contrast/focus audit (`glaze-a11y-report`) | **Implemented, tested** |
| `crates/glaze-compositor` | portable surface; Smithay backend Linux-only | Native compositor capability + milestone reporting; `run()` returns an honest `unsupported` error until built (ADR-0002) | **Portable surface done; backend is a stub** |
| `crates/glaze-integration-tests` | portable | Cross-crate integration tests: session env, security, registry end-to-end, launcher flow, IPC round-trip | **Implemented** |

`apps/glaze-shell` (GTK4 panel) and `apps/glaze-settings` (GTK4 settings) are
planned; their source trees are not in the repository yet.

When you touch a crate, keep the boundary honest: if it is not inherently
Linux/Wayland, it belongs in the portable workspace so it can be tested
everywhere.

---

## 4. The design-token rule

**Never put raw colours or metrics in widgets.** Glaze has exactly one source of
visual truth: `glaze-common::theme`.

- Colour and metric values exist only in `ThemeTokens` / `Palette` /
  `ThemeMetrics` in `crates/glaze-common/src/theme.rs`.
- Adding a colour means **adding a `ThemeRole`** — that is the only supported way
  to introduce a new colour to the design system.
- Widgets are styled by **CSS classes**, not inline colours. The token emitter
  produces rules for classes such as `.glaze-panel`, `.glaze-start-button`,
  `.glaze-task-button`, `.glaze-launcher`, `.glaze-launcher-entry`,
  `.glaze-menu`, `.glaze-clock` and `.glaze-desktop-icon`, plus
  `@define-color glaze_<role>` definitions.
- A widget loads that CSS into a `GtkCssProvider`; it must not write hex or
  colour names directly.

This is enforced by tests in `glaze-common::theme`:
`css_uses_tokens_not_raw_hex` fails if component CSS contains a raw `#colour`,
and `css_defines_every_role_color` fails if a role is not emitted. The contrast
gate `shell_text_meets_wcag_aa` fails if shell or accent text drops below WCAG
2.2 AA in any theme/scheme combination; `glaze-a11y` re-audits the real palettes.

If you need a new visual value:

1. Add a `ThemeRole` variant (and a palette entry for all four
   theme/scheme combinations) or a `ThemeMetrics` field.
2. Give it a snake-case `key()` and add a component CSS rule that references the
   role.
3. Let the existing tests prove every combination is defined and legible.

Do not add a colour in a widget "just this once".

---

## 5. The security rule

**Never pass `.desktop` data to a shell.** Application data is hostile input and
must be treated as such.

- `Exec=` lines are tokenised by `glaze-service::desktop::tokenize_exec`, with
  field codes expanded, producing an **argv vector**. There is a test that
  proves shell metacharacters stay inert.
- Launching goes through `glaze-service::launch::build_command` and spawns the
  process directly — never `sh -c`, never a shell string.
- The IPC protocol (`glaze-common::ipc`) cannot execute arbitrary code: a
  `launch-app` request is limited to an id that already exists in the validated
  registry.
- The control socket lives at `$XDG_RUNTIME_DIR/glaze/control.sock` with mode
  `0600`, so only the owning user can talk to it.

When adding any feature that runs a program or opens a socket, keep the same
discipline, and add a test that demonstrates the dangerous input is neutralised
rather than merely asserting it is.

---

## 6. Architecture decisions (ADRs)

Any change to the architecture — language, component boundaries, the portability
line, the compositor strategy, the IPC contract, identifiers, packaging model —
requires an **Architecture Decision Record** before or alongside the code.

- ADRs live in [`docs/adr/`](adr/) and are named
  `ADR-NNNN-short-title.md` (zero-padded, incrementing). Read the highest-numbered
  ADR first; new ones may have landed since this page was written.
- Use the same shape as the existing records:
  `Status`, `Date`, `Deciders`, `Context`, `Decision`, `Consequences`.
- Superseding an accepted ADR means writing a new one that references the old
  one; do not silently rewrite history.
- Keep [`docs/architecture.md`](architecture.md) and `PROJECT_STATUS.md` in sync
  when a decision changes something user-visible or build-relevant.

If you believe a different architecture is genuinely better, document the
tradeoff in an ADR **before** changing the code, exactly as `GLAZE.md` §4.1 asks.

---

## 7. Working rules that apply to code changes

From [`AGENTS.md`](../AGENTS.md) and `GLAZE.md` §7:

- **Implement, don't narrate.** Create and edit files, run the checks, fix the
  failures.
- **No placeholder success.** Mock data is allowed only behind clearly labelled
  test fixtures, never disguised as real state.
- **Small, verified vertical slices.** Every pass should improve one
  interaction end to end.
- **Never fake tests, logs or screenshots.** Distinguish compile-only,
  nested-session smoke-tested, bare-metal-tested and ARM64-tested in
  `PROJECT_STATUS.md`. Do not claim a Fedora or aarch64 runtime test you did not
  actually run.
- **Safe by default.** No root, no login-manager or boot changes, no changes to
  a live graphical session during development. Nested testing first.
- **Update the docs.** When a command, flag or feature changes, update the
  matching man page in `docs/man/` and the user guide **from the source**, and
  update `PROJECT_STATUS.md`.

---

## 8. Where things live

| Path | What |
|---|---|
| `crates/` | Portable workspace (see the crate map above) |
| `apps/` | GTK4 app workspaces (`glaze-shell`, `glaze-settings`) — planned |
| `themes/` | Design tokens + generated CSS |
| `assets/` | Original SVG icons |
| `scripts/` | Bootstrap, build, check, test, nested run, dev install |
| `packaging/` | Shell completions (RPM spec and session entry are planned, not present yet) |
| `docs/` | Architecture, ADRs, user guide, man pages |
| `docs/man/` | Roff manual pages (see `docs/man/README.md` to install them) |
| `packaging/completions/` | Bash and zsh completions for `glaze` and `glaze-session` |

---

## 9. Shell completions

`packaging/completions/` contains completion scripts for the `glaze` client and
for `glaze-session`:

```sh
# bash (current shell)
source packaging/completions/glaze.bash
source packaging/completions/glaze-session.bash

# bash (installed, system-wide)
sudo install -m 0644 packaging/completions/glaze.bash \
  /usr/share/bash-completion/completions/glaze
sudo install -m 0644 packaging/completions/glaze-session.bash \
  /usr/share/bash-completion/completions/glaze-session

# zsh: place on $fpath under the command names
cp packaging/completions/glaze.zsh "${fpath[1]}/_glaze"
cp packaging/completions/glaze-session.zsh "${fpath[1]}/_glaze-session"
```

Complete only what the code actually supports. If you add a subcommand or flag,
update the matching man page **and** the completion script in the same change.
The `glaze` completion deliberately completes only the commands the parser in
`crates/glaze-cli/src/args.rs` accepts — keep it that way.
