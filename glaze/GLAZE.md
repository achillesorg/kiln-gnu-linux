# GLAZE — PRODUCT, DESIGN & ENGINEERING SPECIFICATION

> **Project:** Glaze, the desktop environment of KilnLinux  
> **Purpose:** A real, native Linux desktop environment inspired by the layout and usability of Windows 2000, evolved into a modern flat cut-paper interface with ink accents (`docs/decisions.md`).  
> **Status:** Greenfield specification — this document is the product source of truth.  
> **Primary target:** KilnLinux (Debian 13 "trixie" based), Wayland, `aarch64` (ARM64) first, `x86_64` (AMD64) second. See `docs/adr/ADR-0006-debian-kilnlinux-target.md`.  
> **Implementation mandate:** Working native code, reproducible builds, honest validation, incremental releases.

## 0. Agent reading order and authority

Read `AGENTS.md` (project working rules), `docs/decisions.md` (the user's decisions, which win over this file where they conflict), `docs/overhaul-plan.md` (milestones), **this file** (requirements and decisions), `GOAL.md` (completion objective), and `START_PROMPT.md` (initial execution brief). Check the actual code before claiming any feature is implemented. Maintain `PROJECT_STATUS.md` with verified progress, build/test results, limitations and the next actionable task. Keep this file aligned when architecture decisions genuinely change; record major decisions in `docs/adr/`.

**Priority:** Safety and user instructions > this specification's non-negotiable requirements > implementation notes > personal preferences not stated here. Do not silently dilute the project into a GTK theme, browser UI, mock desktop, wallpaper pack or single demo window.

## 1. Product vision

Imagine a continuous evolution of the Windows 2000 desktop, not a replica: a clear taskbar, familiar applications menu, window buttons, desktop shortcuts, dense but intelligible system affordances, keyboard efficiency, stable behavior and low friction. Preserve that mental model, then rebuild every surface as a native modern design with gentle volume, controlled rounding, thoughtful materials, consistent typography and superb animation timing.

Glaze must be something a KilnLinux user can eventually log into as a **real desktop session**, launch Linux applications from, manage windows on, configure, and use daily. The first milestones may be incomplete, but they must be real executable milestones in that direction.

### 1.1 Brand

- **Name:** Glaze (written "Glaze" in older text). Pronounced like “fed-ee”. The OS around it is **KilnLinux**.
- **Display name:** Glaze Desktop.
- **Package and binary prefix:** `glaze-` (e.g., `glaze-shell`, `glaze-session`, `glaze-settings`, `glaze-compositor` when implemented).
- **Identifier / desktop entry:** choose stable reverse-DNS identifiers documented in an ADR; do not impersonate Debian, Fedora, Microsoft, Anthropic or another project's trademarks.
- **Logo/icons:** original designs or appropriately licensed assets. Do not copy Windows artwork, system icons, fonts, logos, or proprietary files. Windows 2000 is a *structural reference*, not an asset library.
- **Voice:** crisp, practical, mature, optimistic. UI labels short and human.

## 2. Platforms and deployment

- **Required:** KilnLinux (Debian 13 based) on `aarch64`, using a Wayland session. Debian 13 `x86_64` is a secondary target. The reference machine is a QEMU VM (HVF, `virtio-gpu-pci` without virgl, so software rendering) on an Apple Silicon Mac.
- **Wayland first:** functional native XDG applications, focus, input, window management and multi-output semantics. XWayland for X11 apps is a desirable subsequent milestone.
- **Development:** must build and launch on Debian 13 with packages from its archive (GTK 4.18, gtk4-layer-shell 1.0.4, sway 1.10 at the time of writing). Check currently available packages with `apt-cache policy` and document version limitations; do not assume package names blindly.
- **ARM64:** first-class, not an afterthought. No x86-only intrinsics, binaries, or architecture-coded paths. Test on real AArch64 where available; cross-compilation alone is not equivalent to runtime validation.
- **Authoring host:** agents run on macOS. The portable crates are built and tested there; GTK, Wayland and session claims require a run in the KilnLinux VM (`scripts/vm/sync-build.sh`, `scripts/vm/shot.sh`). Do not claim runtime results from macOS-only checks.
- **Install safety:** support developer-mode nested launch first. No automatic replacement of GNOME, SDDM/GDM configuration, default session or other system desktop; never modify the bootloader or remove desktop packages. Session installation is an explicit opt-in using a `.deb` (later) or the documented local install (`scripts/install-session.sh`). On the KilnLinux dev VM the tty1 autologin profile is versioned in `packaging/kilnlinux/` (ADR-0006).

## 3. The design: Windows 2000 DNA → modern Glaze

### 3.1 Baseline interaction model

- Bottom taskbar as the default shell anchor (position can become configurable).
- Distinct start/home button, application hierarchy, user/system commands, search, running-app list, tray/status and clock.
- Window chrome with discernible title area, minimize, maximize/restore and close affordances where the app/compositor protocol permits.
- Desktop background with launchable shortcuts, selection and a context menu; workspace switching later.
- Obvious active/inactive window states and taskbar affordances.
- Menu density and predictable keyboard navigation inspired by classic desktop computing.
- Never depend on proprietary Windows shells or theming assets.

### 3.2 Evolution model: one shell, two deliberate themes

Design the layout to work in both modes; **the default is Glaze Clay**.

1. **Glaze Classic (heritage mode):** restrained square-ish corners, tighter metrics, flat gray/blue hierarchy, subtle bevel cues, utilitarian 2000-era desktop structure. A tribute, not a pixel-identical re-creation.
2. **Glaze Clay (default):** the same functional architecture with contemporary spacing, softened form, clay-like highlight/shadow geometry, responsive animation, modern typography and a calm palette.

This dual-theme approach makes the “starts like Windows 2000 and gets modified to be modern” relationship tangible, rather than throwing away the original design identity.

### 3.3 Claymorphism, rigorously defined

> **Superseded in part (2026-11-11, `docs/decisions.md`).** Glaze Clay is now a **flat cut-paper** style: terracotta and cream paper sheets stacked with small **hard offsets**, ink-outline icons and borders, no blur, no gradients, no glossy shadows. Read "diffuse outside shadows" below as hard offset sheet shadows. The restraint, legibility and hierarchy rules in this section still apply.

Claymorphism here means **gentle dimensional volume**: slightly inflated controls, diffuse outside shadows, faint inset highlights, extremely light material separation, and compositional hierarchy. It **does not** mean blur everywhere, extruded toy shapes, loud glossy skeuomorphism, illegible low-contrast neumorphism, or a chunky child-oriented interface.

- Mild depth on controls, menus, cards, and active elements only; large content regions mostly flat.
- Contrast and boundaries must remain readable without shadows.
- Use subtle layered borders and highlights instead of giant shadow stacks.
- No shadow on every nested surface; avoid double-triple nesting of raised cards.
- Design with light and dark themes; neither should be an inverse-color hack.
- Animated UI should feel physically coherent but never elastic/jiggly or slow.
- Support reduced motion, keyboard focus rings, high DPI and mixed scaling.

### 3.4 Initial design tokens (adjust by testing, not randomly)

These are **starting values**, not hard-coded limits. Establish one theme-token source shared across the shell and companion apps.

| Element | Glaze Clay baseline | Glaze Classic baseline |
|---|---|---|
| Panel height | 48–54 logical px | 32–38 logical px |
| General control radius | 9–12 px | 2–4 px |
| Menus/dialog radius | 14–18 px | 3–5 px |
| Window surface radius | 12–16 px when supported | 4–6 px |
| Floating overlay radius | 18–22 px | 4–6 px |
| Control height | 34–40 px | 26–32 px |
| Motion, micro-interaction | 100–160 ms | 0–100 ms |
| Motion, menu/overlay | 160–230 ms | 0–120 ms |
| Motion, workspace | 220–320 ms | 0–160 ms |
| Shadow behavior | soft, diffused, low-opacity | minimal, crisp |

Theme palettes should include named roles: `canvas`, `surface`, `surfaceRaised`, `surfaceSunken`, `border`, `textPrimary`, `textSecondary`, `accent`, `accentText`, `focus`, `danger`, `success`, `shadow`, `highlight`. Do not spread raw colors in individual components.

**Visual quality gates:**
- Active app clearly distinguished from merely pinned apps.
- Readable at 100%, 125%, 150%, 200% scale and on both light/dark backgrounds.
- Menus animate from the invoking affordance and do not pop in instantly.
- No layout jumps when apps start/close or time changes.
- No horizontal taskbar overflow without intentional scrolling or grouping.
- Original icons or licensed icon set, consistently rendered at matching size and optical weight.

## 4. Technical approach: native, achievable, incremental

### 4.1 Reference architecture (choose deliberately; document deviations)

A workable path is:

- **Rust core and services:** session supervision, configuration, search index, launcher data, IPC, notifications/system integration and compositor work where appropriate.
- **Wayland compositor:** eventually own a Smithay-based compositor (or justify another mature compositor library after verifying actual APIs and compatibility). For the first runnable shell, it is acceptable and **preferred** to integrate against a stable existing Wayland compositor (such as Sway) so the panel/launcher can be functional before writing every compositor subsystem.
- **Native shell UI:** GTK4 with `gtk4-layer-shell` for panel, menus, desktop surfaces and overlays when supported. `gtk4-layer-shell` is packaged in Debian (`libgtk4-layer-shell-dev`) and supplies Wayland layer-shell integration. If chosen, verify relevant Rust bindings/GObject integration and distro packages. Use CSS only as native toolkit styling; **do not implement the desktop in HTML/CSS/Electron**.
- **Settings:** GTK4 native application, shared theme tokens; use libadwaita selectively only when its styling restrictions do not compromise Glaze's brand.
- **IPC:** versioned local protocol (Unix socket or D-Bus, as appropriate), with well-scoped actions and no arbitrary code execution.
- **Portals and services:** integrate with `xdg-desktop-portal`, `systemd --user`, D-Bus, NetworkManager, UPower, PipeWire/WirePlumber and notification standards where appropriate. Do not reimplement OS services unnecessarily.
- **Application registry:** parse installed `.desktop` entries with standards-compliant libraries or robust parsing; honor Exec, TryExec, icon resolution, categories and XDG app conventions; prevent shell injection.
- **Packaging:** developer-friendly scripts, then reproducible `.deb` packages or a KilnLinux image recipe, and an optional Wayland session desktop entry. (`packaging/fedora/` is legacy, kept for reference.)

The UI and compositor must be replaceable independently. Do not embed all UI business logic inside the compositor. If the agent discovers a better proven architecture, document the tradeoff before changing it.

### 4.2 Suggested eventual components

```text
glaze/
├── AGENTS.md
├── GLAZE.md
├── GOAL.md
├── START_PROMPT.md
├── PROJECT_STATUS.md            # Created/updated by agent
├── README.md                    # Created by agent
├── Cargo.toml                   # If Rust workspace selected
├── crates/
│   ├── glaze-common/              # configuration, models, protocol
│   ├── glaze-session/             # session launcher/supervisor
│   ├── glaze-service/             # D-Bus/IPC, apps, notifications
│   └── glaze-compositor/          # compositor when implementation starts
├── apps/
│   ├── glaze-shell/               # panel, launcher, desktop surfaces
│   └── glaze-settings/            # settings application
├── assets/                      # original/licensed assets
├── themes/                      # Glaze Clay / Glaze Classic tokens
├── packaging/
│   ├── kilnlinux/                # tty1 session profile for the dev VM (ADR-0006)
│   ├── sway/                     # Glaze-owned sway config (fallback compositor)
│   └── fedora/                   # legacy RPM spec, reference only
├── scripts/                     # bootstrap, build, nested run, checks
├── tests/                       # integration and smoke tests
└── docs/
    ├── adr/                     # architecture decisions
    ├── architecture.md
    ├── visual-spec.md
    └── testing.md
```

Adjust layout to the actual implementation, but maintain explicit boundaries and clear names.

### 4.3 Integration realities not to fake

- A panel by itself is not a complete DE. It becomes an *honest first milestone* only when connected to a real compositor, can start real applications, display state, and has a working launch path.
- The taskbar must observe real application/window state using supported compositor protocols, not fabricated running-window lists. With an existing compositor, adapt to its supported IPC as a separate backend; replace with native compositor state when available.
- Rendering rounded **application windows** may require compositor-controlled decorations or cooperation with client-side decoration. Do not promise all foreign apps automatically render with rounded borders.
- System tray requires an actual implementation of StatusNotifierItem/AppIndicator or documented integration; no fake icons.
- Notification history requires a real notification server or integration. Respect other running notification daemons and avoid claiming the bus name twice.
- Power, brightness, volume, networking and Bluetooth need real platform integrations, proper permissions and graceful unsupported states.
- Locking requires secure session-lock support, not a fake fullscreen window.
- `systemd --user` and session environment variables must be correct for apps/portals.

## 5. Requirements backlog

Priority labels: **P0** core runnable shell; **P1** usable personal session; **P2** mature DE; **P3** optional enhancements.

### P0 — first verifiable runnable milestone

- Launchable shell on KilnLinux under a compatible Wayland compositor, including safe nested test mode.
- Glaze Clay bottom panel; a distinct start button; real `.desktop` app launcher with typed search.
- Clock; visible application/taskbar integration where backend supports it; real pinned launcher items.
- Open/close behavior, keyboard navigation, Escape dismiss, outside click dismiss, focus return.
- Glaze Classic styling variant selectable through config or settings if feasible this milestone.
- Original placeholder-independent UI assets; stable theme tokens.
- Build scripts with dependency checks; clear quickstart and screenshot/test instructions.

### P1 — daily-session foundation

- Own compositor **or** explicit equivalent managed compositor session with documented responsibilities.
- Real app launching, focus, move/resize, task switching, keyboard shortcuts, multiple workspaces.
- Settings app: appearance, panel and wallpaper first; persist via versioned configuration.
- Wallpaper, desktop surfaces and context menus; minimal launchable desktop shortcuts.
- Volume, network, battery/power status where hardware/services allow.
- Proper shutdown/restart/logout confirmation, no destructive shortcuts.
- Notification server / tray integration; proper portal compatibility.
- User session startup and shutdown behavior with logging and crash resilience.

### P2 — mature desktop

- Multi-monitor, hotplug, per-output scale, mixed DPI, adaptive panel overflow.
- XWayland application support, where support is available; handle CSD/SSD gracefully.
- Full keyboard shortcut editor and launcher search ranking.
- Usable file manager integration (XDG default app initially; optional Glaze file manager later).
- Window tiling/snapping, task previews where technically supported, alt-tab, overview.
- Login manager/session entry packaging and settings migration.
- Animation polish with reduced-motion controls, accessibility labels and screen-reader pathways.
- Reliable suspend/lock/unlock cycle, clipboard and drag/drop.

### P3 — extension ecosystem and richer UX

- Plugins/widgets with strictly defined security boundaries.
- Wallpaper scheduling, advanced theming, user presets and import/export.
- Additional companion apps where maintenance cost makes sense.
- Accessibility enhancements and translations.

## 6. Interaction design

### 6.1 Panel

Default order: Glaze button → pinned/favorite apps → running windows → flexible spacer → tray/status → clock/date → optional show-desktop. No duplicate affordances. Pinned vs active states must be visually unmistakable. Right-click menus for pin/unpin and window actions; keyboard support.

### 6.2 Launcher

The classic Start-menu information architecture, modern appearance: favorites/recent apps, application groups, inline search, quick system actions. Typing search is instantaneous; arrow keys navigate; Enter launches; Escape closes. Prevent malicious shell execution from `.desktop` data. Do not display fake search results or sample apps as installed apps.

### 6.3 Window behavior

Compositor owns window focus/workspace behavior. Titlebar/control placement should be consistent when Glaze draws decorations, without misleading decorations on third-party CSD windows. User can alt-tab, click a task, minimize/restore, and move/resize supported windows. Visual effects never compromise input correctness.

### 6.4 Motion

Use short easing curves, animated origin continuity, and GPU-friendly transforms/opacity when possible. No blocking the input loop. Menu show/hide must be reversible without glitches during rapid repeated input. Support no-animation mode and reduce-motion preferences.

### 6.5 Accessibility

Meaningful focus ring and accessible names; logical tab order; keyboard-first flows. Aim for WCAG 2.2 AA-level contrast for shell text and controls where applicable. Scale text and UI without clipping; high contrast and reduced motion settings.

## 7. Engineering rules

1. **Implement, not narrate.** Create/edit files, run build and relevant tests, fix failures before declaring milestone complete.
2. **No placeholder success.** A screenshot is not evidence a launcher really launches apps. Mock data allowed only behind clearly labeled dev/test fixtures, never disguised as actual state.
3. **No pseudocode in runtime files.** README may describe incomplete work, but production entry points must run.
4. **Small, verified vertical slices.** Every development pass should improve a demonstrable interaction end-to-end.
5. **Never fake tests or screenshots.** Distinguish compile-only, nested-session smoke-tested, bare-metal tested and ARM64-tested in `PROJECT_STATUS.md`.
6. **Keep dependencies reasonable.** Favor Debian packages and mature libraries; no web stack for the desktop shell.
7. **Safe by default.** Use developer commands under a non-root user; no login manager changes unless explicitly opted in; no dangerous writes to a live session.
8. **Maintainable code.** Strong typing, error handling, logging, clear module ownership, protocol-versioning where needed.
9. **License discipline.** Track license of all third-party assets; do not lift proprietary Windows assets.
10. **Token efficiency.** Prioritize repository inspection, direct edits and verification; avoid repeatedly dumping unchanged files into model context.
11. **Do not prematurely stop.** After each successful milestone, consult `GOAL.md`, log status, and pursue the next achievable increment while the current run/session and permissions allow. Do not claim completion because a scaffold was generated.
12. **Questions:** make reasonable technical decisions autonomously. Ask only when essential choice changes the user's goal or would risk user data/system stability.

## 8. Proposed roadmap with acceptance gates

### Phase 0 — bootstrap & design baseline

- Inspect cwd/OS/arch, initialize a safe project repo without overwriting existing work.
- Create `README.md`, `PROJECT_STATUS.md`, `docs/architecture.md`, `docs/visual-spec.md`, first ADR and build scripts.
- Decide buildable stack using actual current Debian 13 library versions. Show architecture rationale succinctly.
- Create theme tokens and a small visual component set; prove basic build on available host.

**Pass:** clean checkout documents prerequisites and executes a meaningful build/check (or records the precise external blocker).

### Phase 1 — launchable Glaze shell

- Build GTK4 Wayland shell panel under an existing compatible compositor.
- App menu/search from actual installed application entries.
- Glaze Clay default, Glaze Classic alternate; usable keyboard interactions.
- Clock and pinned/running tasks backed by real state when backend available.
- Run from a documented script, log failures and gather a real screenshot from the KilnLinux VM.

**Pass:** starting the shell shows panel; opening launcher and launching an installed app works; closing/reopening launcher, mouse and keyboard navigation work; no fake app registry.

### Phase 2 — interactive desktop integration

- Establish real compositor/task state bridge, workspaces, keybindings and desktop wallpaper layer.
- Add settings for appearance/panel/wallpaper, persistent config, tray/notifications with appropriate protocol support.
- Improve lifecycle: process supervision, logging, IPC and crash recovery.

**Pass:** real task switching works, settings survive restart and known services fail gracefully.

### Phase 3 — native complete session

- Implement and exercise the chosen first-class compositor strategy (Smithay or documented alternative) with xdg-shell, outputs, input and window management.
- Provide installable KilnLinux Wayland session alongside existing desktop, not replacing it.
- Verify portals, systemd environment, start/stop, XWayland or explicit limitations.

**Pass:** user can opt into a Glaze login session, run typical applications and exit back to their display manager; workspaces and focus work.

### Phase 4 — production polish & breadth

- Multi-monitor/hidpi, a11y, settings coverage, notifications/tray, lock/unlock, failure modes, package upgrades.
- Build/test on both architectures; `.deb`/image packaging; basic CI where environment permits.
- Design review: no clip, no rough animation, consistent icons and themes.

**Pass:** all major P1/P2 workflows verified or explicitly documented as outstanding; no blocking crash in tested happy path.

## 9. Validation matrix

Record each test with environment, architecture, command, result and date.

| Category | Minimum checks |
|---|---|
| Build | formatting, dependency resolution, compile, tests; `cargo fmt --check`, `cargo clippy`/`cargo test` where Rust applies |
| Shell | start/stop cleanly; crash errors logged; renderer visibly updates; no orphan processes |
| Launcher | enumerate XDG apps, search by name, launch real app, handle invalid entry |
| Taskbar | state corresponds to real windows; focus switching, close/move behavior where supported |
| Input | mouse, keyboard, tab focus, Escape, rapid menu toggles, pointer outside-click |
| Appearance | clay/classic, light/dark, reduced motion, scaling, long labels |
| KilnLinux aarch64 | real run in the KilnLinux QEMU VM (primary) |
| Debian 13 x86_64 | real run on an x86_64 Debian 13 machine/VM, if available |
| Security | no shell injection through .desktop data; input/bus errors handled; safe session startup |

Do not confuse arm64 compilation with working GPU/display/input on a specific ARM board. Record hardware and driver caveats honestly.

## 10. Scripts and developer experience

Aim for usable documented commands: `scripts/build.sh`, `scripts/install-session.sh`, `scripts/vm/sync-build.sh`, `scripts/vm/shot.sh`, `scripts/run-nested.sh`, `scripts/test.sh`. They must be **real**, idempotent where sensible, explain package changes, and do nothing dangerous to a user's existing graphical session. Native commands can be wrapped rather than reinvented. Linux script interpreter Bash. On the macOS host, run the portable checks there and the Linux commands in the VM; never run Debian package commands on the host.

Give meaningful errors for missing Wayland display, incompatible compositor, missing `gtk4-layer-shell`, absent desktop entries or unavailable status bus. If package names differ by Debian release or architecture, detect/document rather than invent.

## 11. Repository and agent progress discipline

Create and maintain `PROJECT_STATUS.md` containing:

- **Last verified build/run:** OS, arch, command and outcome.
- **Working features:** explicit evidence, not wishful descriptions.
- **In progress / blockers:** failures with exact reproduction steps.
- **Next 3 actions:** concrete file-level or feature-level tasks.
- **Architecture decisions:** references to `docs/adr/`.
- **Recent changes:** a concise dated changelog.

Before concluding any iteration, update status and README when commands/features changed. If validation cannot run on the current machine, state that plainly; continue with useful code and checks instead of inventing results.

## 12. Definition of done

**Do not label Glaze finished** until it is actually installable as an optional Wayland desktop session on KilnLinux, has a working application launcher, real task switching/window management, settings persistence, desktop surface, taskbar, a functional exit/login workflow, compatible application launching, acceptable stability, consistent modern cut-paper (Clay) and classic themes, and verified testing on both x86_64 and aarch64 (or the remaining architecture limitation is prominently disclosed).

A good early release is *Glaze Alpha*: visibly polished native panel plus real launcher and desktop-session integration in progress. That is an honest milestone, **not** completion of the whole DE.

## 13. Useful reference URLs (verify as needed)

- OpenCode instructions: https://opencode.ai/docs/instructions/
- OpenCode custom commands: https://opencode.ai/docs/commands
- GTK4 layer shell in Debian: https://packages.debian.org/trixie/libgtk4-layer-shell-dev
- Smithay: https://github.com/Smithay/smithay
- Wayland protocol definitions: https://gitlab.freedesktop.org/wayland/wayland-protocols
- XDG Desktop Entry Specification: https://specifications.freedesktop.org/desktop-entry-spec/latest/

These are references, not claims that every integration is already implemented or available on every Debian release.
