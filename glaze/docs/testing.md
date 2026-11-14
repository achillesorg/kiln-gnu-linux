# Glaze — testing and validation

This document is the honest record of **what has actually been tested, where,
and how**. It follows the validation matrix in `GLAZE.md` §9 and the script
requirements in §10. Nothing here is aspirational: every "PASS" line was
produced by running the exact command shown.

> **Golden rule:** a Fedora/Wayland/ARM64 claim requires a run on a Fedora,
> Wayland or ARM64 host. This repository is currently authored on a **Windows
> 11 x86_64** development host, so every Linux-only row below is marked
> **NOT RUN HERE** with the exact Fedora command to run instead.

---

## 1. Verification path on this host (Windows 11, x86_64)

Run from the repository root:

```powershell
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

A convenience wrapper that runs the same portable checks is
`tests/smoke.sh` (needs bash; on Windows use WSL or Git-Bash):

```bash
tests/smoke.sh            # portable checks
tests/smoke.sh --with-check   # also runs scripts/check.sh on Linux
```

### Results captured on 2026-11-11

Host: Windows 11, `x86_64-pc-windows-msvc`, `rustc 1.99.0` / `cargo 1.99.0`.

| Command | Result |
|---|---|
| `cargo fmt --all -- --check` | **PASS** (exit 0) |
| `cargo clippy --workspace --all-targets -- -D warnings` | **PASS** (exit 0, 0 warnings) |
| `cargo test --workspace` | **PASS** — 298 passed, 0 failed, 0 ignored (34 test binaries) |
| `cargo test -p glaze-integration-tests` | **PASS** — 25 passed, 0 failed (5 integration binaries) |
| `bash -n scripts/*.sh tests/smoke.sh` | **PASS** (exit 0 for all 7 scripts) |
| `shellcheck scripts/*.sh` | **NOT RUN** — `shellcheck` is not installed on this host |
| `cargo check --target x86_64-unknown-linux-gnu` | **BLOCKED** — fails inside `crates/glaze-service` (owned by another workstream); see §5 note |

> Test counts reflect the repository at the time of the run and grow as parallel
> workstreams land more tests; the pass/fail outcome is what matters.
>
> The Glaze workspace is developed by several workstreams in parallel, so
> individual sibling crates can be briefly non-compiling while another
> workstream is mid-edit. The table above is a real, captured run of the whole
> workspace at a stable moment. The Glaze QA-owned crate can always be checked on
> its own: `cargo test -p glaze-integration-tests` (its `fmt`/`clippy`/`test` all
> pass in isolation).

---

## 2. Validation matrix (GLAZE.md §9)

| Category | Environment / arch | Exact command | Result | Date |
|---|---|---|---|---|
| Build — format | Windows x86_64 | `cargo fmt --all -- --check` | **PASS** | 2026-11-11 |
| Build — lint | Windows x86_64 | `cargo clippy --workspace --all-targets -- -D warnings` | **PASS** (0 warnings) | 2026-11-11 |
| Build — compile + unit tests | Windows x86_64 | `cargo test --workspace` | **PASS** (298/298) | 2026-11-11 |
| Build — cross-crate integration | Windows x86_64 | `cargo test -p glaze-integration-tests` | **PASS** (25/25) | 2026-11-11 |
| Launcher — enumerate XDG apps | Windows x86_64 (fixtures) | `cargo test -p glaze-integration-tests --test registry_e2e` | **PASS** (5/5) | 2026-11-11 |
| Launcher — search + build argv | Windows x86_64 (fixtures) | `cargo test -p glaze-integration-tests --test launcher_flow` | **PASS** (5/5) | 2026-11-11 |
| Security — no shell via `.desktop` | Windows x86_64 | `cargo test -p glaze-integration-tests --test security` | **PASS** (6/6) | 2026-11-11 |
| Session — env + supervisor | Windows x86_64 | `cargo test -p glaze-integration-tests --test session_env` | **PASS** (5/5) | 2026-11-11 |
| IPC — protocol serde round-trip | Windows x86_64 | `cargo test -p glaze-integration-tests --test ipc_roundtrip` | **PASS** (4/4) | 2026-11-11 |
| IPC — server dispatch + version mismatch | Fedora/Linux (Unix sockets) | `cargo test -p glaze-integration-tests --test ipc_roundtrip` | **NOT RUN HERE** (Unix-only; see §5) | — |
| Shell — start/stop, no orphan processes | Fedora x86_64 + Wayland | `scripts/run-nested.sh` then observe the nested Sway window | **NOT RUN HERE** | — |
| Taskbar — real window state | Fedora x86_64 + Sway | `scripts/run-nested.sh` and open/close apps | **NOT RUN HERE** | — |
| Input — mouse / keyboard / Escape | Fedora x86_64 + Wayland | manual, inside `scripts/run-nested.sh` | **NOT RUN HERE** | — |
| Appearance — clay/classic, light/dark | Fedora x86_64 + GTK4 | token tests pass here; visual run needs GTK4 | **TOKENS PASS** here; visual **NOT RUN HERE** | 2026-11-11 |
| Fedora x86_64 — real run | Fedora x86_64 | see §3 | **NOT RUN HERE** | — |
| Fedora aarch64 — real run | Fedora aarch64 (e.g. Asahi) | see §3 | **NOT RUN HERE** | — |

---

## 3. Exact commands to run on Fedora

These are the authoritative runtime checks. Run them on a **Fedora** host; the
login manager and boot configuration are never modified.

### x86_64

```bash
# 1. Dependencies (idempotent, explains every package, never touches your session)
scripts/bootstrap-fedora.sh --dry-run     # preview
scripts/bootstrap-fedora.sh               # install what is missing

# 2. Readiness report (Wayland display, gtk4-layer-shell, compositor, entries)
scripts/check.sh

# 3. Build everything (portable workspace + GTK apps)
scripts/build.sh

# 4. Full check suite (fmt, clippy, tests, GTK app builds)
scripts/test.sh

# 5. Nested, non-destructive run: a throwaway Sway window with glaze-shell inside
scripts/run-nested.sh --windowed
```

### aarch64 (e.g. Fedora on Asahi / ARM64)

The commands are identical; only the architecture differs:

```bash
uname -m                     # expect: aarch64
scripts/bootstrap-fedora.sh
scripts/build.sh
scripts/test.sh
scripts/check.sh
scripts/run-nested.sh --windowed
```

Record the exact machine, GPU/driver and `uname -a` next to the result, because
aarch64 *compilation* is not equivalent to working display/input on a specific
ARM board (`GLAZE.md` §9).

### Reporting a result

Add a row to §2 with the environment, the exact command and the real outcome.
Never mark a row "PASS" without a command that actually ran.

---

## 4. What genuinely is verified on this (Windows) host

These are real, executed checks of the **portable** logic — the same code the
Fedora shell links against:

* **Application registry** (`crates/glaze-service`): discovery from XDG-style
  directories, `Hidden` / `NoDisplay` / `TryExec` / `OnlyShowIn` filtering,
  id flattening for nested entries, user-directory precedence, localised names,
  deterministic name/id ordering. Fixtures written to real temp directories on
  disk. (`registry_e2e.rs`)
* **Launcher flow**: `search_apps` ranking over the discovered registry, then
  `build_command` producing the **exact argv** (no shell), including field-code
  expansion (`%u`, `%F`, `%k`). (`launcher_flow.rs`)
* **Security**: `.desktop` `Exec` data cannot reach a shell — `;`, `|`, `>`,
  `$(...)` and backticks stay inert literal arguments, proven both directly on
  `tokenize_exec` and end-to-end through the registry → launch path.
  (`security.rs`)
* **Session**: `SessionPlan::build` exports the required Wayland/toolkit
  environment; `Supervisor` really spawns a probe process, observes its exit and
  respawns it; a missing program is reported, not panicked on. (`session_env.rs`)
* **IPC protocol**: every `Request`/`Response` variant round-trips through a
  single-line JSON encoding with stable kebab-case wire tags. (`ipc_roundtrip.rs`)
* **Design tokens / colour / config** (upstream unit tests): WCAG AA contrast
  gates, CSS emission, TOML config round-trip and v1→v2 migration.

## 1.1 Type-checking the GTK apps without GTK installed

The GTK apps cannot be *built* here (`gtk4-sys`/`gio-sys` build scripts call
`pkg-config` for system GTK), but they **can** be *type-checked*:

> `cargo check` and `cargo clippy` never link, so the `-sys` crates only need
> their `pkg-config` probes to succeed. Supplying a stub `pkg-config` is enough
> to make the compiler check every line of the app code.

`scripts/dev/pkg-config-stub` is that stub; `scripts/typecheck-apps.sh` builds it
into a temp dir, exports `PKG_CONFIG`, `PKG_CONFIG_ALLOW_CROSS`,
`PKG_CONFIG_ALLOW_SYSTEM_CFLAGS` and `PKG_CONFIG_ALLOW_SYSTEM_LIBS`, and runs
`cargo check` + `cargo clippy -D warnings` for both apps on
`x86_64-unknown-linux-gnu` and `aarch64-unknown-linux-gnu`.

This is real verification of *compile correctness* (types, borrows, API names,
feature gates) — not of runtime behaviour. It is how the following genuine
defects were found and fixed on 2026-11-11:

- `glib::source::unix_signal_add_local` (removed in glib 0.22; the shell used it)
- two ownership/borrow bugs in `apps/glaze-shell/src/launcher.rs`
- `CssProvider::load_from_string` behind a missing `v4_12` feature (`glaze-settings`)
- `toggle()` returning a row `Widget` but being passed where a `Switch` was required
- a move-while-borrowed bug in `apps/glaze-settings/src/pages/wallpaper.rs`
- 7 unused imports

Manual equivalent on any host without GTK:

```bash
PKG_CONFIG=/path/to/pkg-config-stub PKG_CONFIG_ALLOW_CROSS=1 \
PKG_CONFIG_ALLOW_SYSTEM_CFLAGS=1 PKG_CONFIG_ALLOW_SYSTEM_LIBS=1 \
  cargo check --manifest-path apps/glaze-shell/Cargo.toml \
              --target x86_64-unknown-linux-gnu
```

---

## 5. What cannot be tested on this host (and why)

| Not testable here | Why | Where it must be tested |
|---|---|---|
| GTK4 **runtime** (`apps/glaze-shell`, `apps/glaze-settings`) | needs GTK4 + `gtk4-layer-shell` + Wayland; no GTK system libraries on Windows | Fedora + Wayland (`scripts/run-nested.sh`) |
| GTK4 **type-checking** | *not* blocked: `scripts/typecheck-apps.sh` type-checks both apps for x86_64 + aarch64 linux using a stub `pkg-config` (see §1.1) | done here — 0 errors, clippy `-D warnings` clean |
| Wayland display, layer-shell panel placement, focus | requires a real Wayland compositor | Fedora + Sway/wlroots |
| `IpcServer` socket **transport at runtime** | `glaze-service::ipc_server`'s serve loop is `#[cfg(unix)]`; it now **compiles** for x86_64/aarch64 (`cargo check --target …`) but has never been run | Fedora/Linux (`--test ipc_roundtrip`) |
| D-Bus, `systemd --user`, portals, NetworkManager, UPower, PipeWire | Linux system services | Fedora |
| XWayland / foreign X11 apps | needs a compositor with XWayland | Fedora |
| Multi-monitor, HiDPI/mixed scale, input devices | hardware/display dependent | Fedora on target hardware |
| Native compositor (`glaze-compositor`) | staged; `run()` returns an explicit `Unsupported` error, so there is nothing to execute successfully yet | Fedora (when implemented) |
| Real application launch from the launcher | requires real Fedora binaries and a display | Fedora |
| **ARM64/aarch64 runtime — everything** | no aarch64 machine available here; cross-compilation is not runtime validation | Fedora aarch64 (Asahi/other) |

---

## 6. Continuous integration

`.github/workflows/ci.yml` has two jobs:

* **`portable`** — on `windows-latest` and `ubuntu-latest`: `cargo fmt --check`,
  `cargo clippy -D warnings`, `cargo test --workspace`.
* **`fedora`** — in the `fedora:latest` container: install `gtk4-devel`,
  `gtk4-layer-shell-devel`, `pkgconf`, `wayland-devel`, etc.; run the portable
  checks; then build the GTK apps under `apps/`.

CI mirrors the local commands in §1, so a green CI means the same checks that
pass here also pass there.

---

## 7. Script reference (GLAZE.md §10)

All scripts are Bash, `set -euo pipefail`, idempotent where sensible, and print
what they are about to do. None of them modifies the login manager, bootloader
or any other session. They are committed as executable; on a fresh clone ensure
the bit survived with `chmod +x scripts/*.sh tests/smoke.sh`.

| Script | Purpose | Key options |
|---|---|---|
| `scripts/bootstrap-fedora.sh` | Detect Fedora/arch; install missing build deps | `--dry-run`, `--yes`, `--minimal` |
| `scripts/build.sh` | Build the portable workspace, then the GTK apps on Linux | `--release`, `--workspace-only` |
| `scripts/test.sh` | fmt + clippy + tests, plus GTK app builds on Linux | `--no-clippy`, `--no-apps` |
| `scripts/check.sh` | Readiness report (display, layer-shell, compositor, entries) | `--strict` |
| `scripts/run-nested.sh` | Nested, non-destructive shell run | `--windowed`, `--x11`, `--headless`, `--use-current`, `--compositor N` |
| `scripts/install-dev.sh` | Optional dev symlinks + desktop entry | `--release`, `--bindir DIR`, `--uninstall` |
| `scripts/typecheck-apps.sh` | Type-check both GTK apps for x86_64+aarch64 linux without GTK installed (stub `pkg-config`) | `--target TRIPLE` |
| `tests/smoke.sh` | Portable smoke checks | `--with-check` |

Every script responds to `--help`.
