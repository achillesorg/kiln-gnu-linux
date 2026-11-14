# Glaze user guide

Glaze (from **FedoraDE**) is a native Fedora Wayland desktop environment for
`x86_64` and `aarch64`. Its structure starts from the Windows 2000 desktop
model — a bottom panel, a distinct start button, an application launcher with
typed search, running-window tasks and a clock — and evolves it into a modern,
restrained claymorphic interface. It is not a theme, not a browser UI and not a
mockup; every part is native Rust, GTK4 and Wayland.

> **Current state:** Glaze is an early, honest work in progress. This guide
> describes both the parts that work today and the interaction model that the
> shell implements as it lands. For the precise, evidence-backed list of what
> is built and tested right now, always read
> [`PROJECT_STATUS.md`](../PROJECT_STATUS.md). Do not treat this guide as a
> claim that every surface is finished.

---

## 1. What works today, and what does not

Read [`PROJECT_STATUS.md`](../PROJECT_STATUS.md) first — it is the source of
truth. In summary:

**Implemented and unit-tested (portable crates, run anywhere):**

- The design-token and theme system (`glaze-common::theme`): **Glaze Clay**
  (default) and **Glaze Classic**, each in light and dark, emitting GTK4 CSS.
- Versioned configuration with migration (`glaze-common::config`).
- The real `.desktop` application registry, XDG visibility rules, desktop-entry
  parsing and launcher search ranking (`glaze-service`).
- The shell-safe launch path: applications are launched from an argv vector and
  never through a shell (`glaze-service`).
- The session environment and the component supervisor with bounded respawn
  (`glaze-session`).
- The **`glaze` command-line client** (`glaze-cli`): list and search the real
  application registry, print the exact launch argv, launch an application,
  read and write configuration, and send raw IPC requests. See
  [`docs/man/glaze.1`](man/glaze.1).

**In progress / not yet available — do not expect these:**

- The **GTK4 shell** (`glaze-shell`) that draws the panel and launcher. Its
  source tree is not yet present in the repository, so there is **no panel to
  click today**.
- The **settings application** (`glaze-settings`). Configure Glaze with
  `glaze config set`, by editing `~/.config/glaze/config.toml` (see §6), or with
  the settings app once it lands.
- The **native compositor** (`glaze-compositor`). Running it returns an explicit
  `unsupported` error; it does not start a session. The first runnable milestone
  targets an existing wlroots compositor (Sway) instead (see
  [`docs/adr/ADR-0002-compositor-strategy.md`](adr/ADR-0002-compositor-strategy.md)).
- **Installing Glaze as a login session.** There is no RPM and no session entry
  yet. Glaze never edits your login manager, display manager or boot settings.
- Window task state, the system tray and notifications, XWayland, workspaces
  and multi-monitor support.

If you are a developer who wants to build and test the portable parts today, see
[`docs/contributing.md`](contributing.md).

---

## 2. The panel

The Glaze panel is the anchor of the desktop. It is intended to sit along the
bottom edge by default and is laid out, left to right, as:

| Region | Contents |
|---|---|
| **Glaze button** | The start button. Opens the launcher. Clearly shows when the launcher is open. |
| **Pinned apps** | Applications you have pinned, always visible. |
| **Running windows** | One task button per live window, with active and focused states that are visually unmistakable. |
| **Spacer** | Flexible space that keeps the right-hand group at the edge. |
| **Tray / status** | Status indicators, once a real tray implementation lands. |
| **Clock / date** | Time and date, with no layout jump as the value changes. |
| **Show desktop** | Optional; a future affordance. |

Pinned versus merely-running applications are meant to look different at a
glance (see the task-button styles in the theme). With no compositor backend
attached, the running-window area is genuinely empty — Glaze never shows a
fabricated window list.

---

## 3. The launcher

The launcher is the classic start-menu information architecture in a modern
surface: your applications, with inline search that filters as you type. It
enumerates the **real** applications installed on the machine through the
`glaze-service` registry (XDG `.desktop` entries); it never shows sample apps as
if they were installed.

### Opening and closing

- Click (or activate) the **Glaze button** to open the launcher.
- The launcher opens from the button and animates out from it, rather than
  appearing instantly.
- **Click anywhere outside** the launcher to dismiss it. Keyboard focus returns
  to where it was.
- Press **Escape** to dismiss it without launching anything.

### Searching

Search is instantaneous and deterministic. Results are ranked by a documented
rule — exact name match first, then prefix, word-prefix, substring and
subsequence matches, with smaller contributions from keywords, generic name,
comment and the desktop-file id. Matched characters in the name are highlighted.
An empty search box shows the application list (and recently used apps when that
state is available).

### Keyboard reference

These are the launcher keys the shell implements:

| Key | Action |
|---|---|
| Any printable character | Append to the search query; results update immediately |
| <kbd>Backspace</kbd> | Delete the last character of the query |
| <kbd>↓</kbd> / <kbd>↑</kbd> | Move the selection down / up through the results |
| <kbd>Enter</kbd> | Launch the selected application |
| <kbd>Escape</kbd> | Close the launcher without launching |
| <kbd>Tab</kbd> | Move focus between the launcher's controls (query box, results) |

### Launching behaviour

Pressing <kbd>Enter</kbd> asks `glaze-service` to launch the selected application
by its desktop id. The service expands the entry's `Exec` field codes, builds an
argv vector and spawns the process detached. **It never passes `.desktop` data
to a shell**, so a malicious `Exec=` line cannot execute shell commands.
Applications declaring `Terminal=true` are wrapped in an available terminal
emulator.

---

## 4. Themes: Glaze Clay and Glaze Classic

Glaze ships one shell and two deliberate themes. Both are real, fully specified
token sets — dark is a designed palette, not an inversion of light.

| | **Glaze Clay** (default) | **Glaze Classic** (heritage) |
|---|---|---|
| Character | Soft, modern, rounded, claymorphic | Tighter, squarer, 2000-era structure |
| Panel height | ~52 logical px | ~36 logical px |
| Control corner radius | ~10 px | ~3 px |
| Menu / overlay radius | ~16 / ~20 px | ~4 / ~5 px |
| Motion (micro / menu) | 130 ms / 190 ms | 60 ms / 90 ms |
| Shadow | Soft and diffused | Minimal and crisp |

Both themes provide the same named colour roles (canvas, surface, raised and
sunken surfaces, border, primary/secondary text, accent and accent text, focus,
danger, success, warning, shadow, highlight) plus success and warning colours.
The colour system is checked so that shell text and accent text meet **WCAG 2.2
AA** contrast in all four theme/scheme combinations.

### Light and dark

Each theme has a **light** and a **dark** scheme, selected independently of the
theme family. Glaze also honours a **reduced-motion** setting: when enabled, UI
transitions are emitted with `0ms` durations.

Themes are chosen in configuration today (§6). A settings UI will follow.

---

## 5. Clock and pinned applications

- **Clock.** The panel clock shows the time, and the date, and can show seconds.
  It uses tabular figures so digits do not shift the layout, and it updates
  without causing a layout jump. The 12-/24-hour format and the date are
  configurable, with an optional custom date format string.
- **Pinned applications.** Applications can be pinned so they stay on the panel.
  Pinned entries are identified by their **desktop-file id** (for example
  `org.mozilla.firefox` or `glaze-settings`), which you can list with the
  registry or read from the application's `.desktop` file name. Pinned items
  remain visually distinct from running windows. Pinning is configured today by
  editing the `pinned` list in `~/.config/glaze/config.toml`; right-click
  pin/unpin is planned.

---

## 6. Configuration

Glaze stores user configuration as TOML at:

```text
$XDG_CONFIG_HOME/glaze/config.toml      # default: ~/.config/glaze/config.toml
```

Every key has a safe default, so a partial or hand-edited file still loads. The
file is **versioned** and older files are migrated forward automatically without
discarding settings.

The keys below match the implemented configuration schema
(`glaze-common::config`). The settings application that would edit them is not
built yet. Until it is, set values with the client (`glaze config set <key>
<value>`, validated and written safely) or edit the file by hand; both are
supported, and `glaze config get` prints the effective configuration.

### Appearance — `[appearance]`

| Key | Values | Default | Meaning |
|---|---|---|---|
| `theme` | `clay`, `classic` | `clay` | Glaze Clay or Glaze Classic. |
| `scheme` | `light`, `dark` | `light` | Colour scheme. |
| `reduced_motion` | `true`, `false` | `false` | Emit zero-duration transitions. |
| `scale` | number | `1.0` | UI scale multiplier; `1.0` follows the system. |
| `font_scale` | number | `1.0` | Extra font scaling on top of the theme base size. |

### Panel — `[panel]`

| Key | Values | Default | Meaning |
|---|---|---|---|
| `position` | `top`, `bottom`, `left`, `right` | `bottom` | Docked edge. |
| `height` | integer (logical px) or omitted | theme default | Override panel thickness. |
| `autohide` | `never`, `smart`, `always` | `never` | Auto-hide behaviour. |
| `show_labels` | `true`, `false` | `true` | Window titles next to task icons. |
| `clock_24h` | `true`, `false` | `true` | 24-hour clock. |
| `show_seconds` | `true`, `false` | `false` | Show seconds in the clock. |
| `show_date` | `true`, `false` | `true` | Show the date. |
| `date_format` | string or omitted | none | Custom date format fallback. |

### Launcher — `[launcher]`

| Key | Values | Default | Meaning |
|---|---|---|---|
| `search_limit` | integer | `8` | Maximum results shown. |
| `show_recent` | `true`, `false` | `true` | Show recently used apps on an empty query. |
| `pinned` | list of desktop ids | `[]` | Applications pinned to the panel / favourites. |

### Desktop — `[desktop]`

| Key | Values | Default | Meaning |
|---|---|---|---|
| `wallpaper` | path or omitted | none | Wallpaper image; omitted uses the theme canvas colour. |
| `show_icons` | `true`, `false` | `false` | Show desktop icons. |

### Session — `[session]`

| Key | Values | Default | Meaning |
|---|---|---|---|
| `compositor` | `auto`, `sway`, `layer-shell`, `glaze`, `none` | `auto` | Which compositor backend the shell binds to. |
| `restore_apps` | `true`, `false` | `false` | Restore applications on session start. |
| `debug` | `true`, `false` | `false` | Verbose shell logging. |

### Example

```toml
# ~/.config/glaze/config.toml
version = 2

[appearance]
theme = "clay"
scheme = "dark"
reduced_motion = false

[panel]
position = "bottom"
clock_24h = true
show_date = true

[launcher]
pinned = ["org.mozilla.firefox", "glaze-settings"]
```

---

## 7. Running Glaze from the command line

Two commands are usable today: the `glaze` client and the `glaze-session`
developer tool. Their full interfaces are in
[`docs/man/glaze.1`](man/glaze.1) and
[`docs/man/glaze-session.1`](man/glaze-session.1).

### `glaze` — inspect and drive the service

```sh
glaze apps                              # list discovered applications
glaze search firefox --limit 5          # ranked search of the real registry
glaze exec-line org.mozilla.firefox     # print the exact launch argv (JSON)
glaze launch org.mozilla.firefox        # launch it (never via a shell)
glaze status                            # version, registry size, paths
glaze config get                        # print the whole config as TOML
glaze config get panel.position         # print one value
glaze config set appearance.scheme dark # persist a value
glaze ipc '{"kind":"ping"}'             # raw IPC request (Unix only)
```

`glaze` exits `0` on success, `1` for a user error (bad input, unknown app or
value, failed request) and `2` for an internal error (config I/O, IPC transport).

### `glaze-session` — developer-mode session launcher

On a Fedora/Wayland host:

```sh
# Print the environment a Glaze session would export
glaze-session env --backend auto

# Check that the components a session needs are present
glaze-session check --backend sway

# Start the session, nested inside a window (host session untouched)
glaze-session run --backend sway --nested

# Show the planned session id and environment as JSON
glaze-session status
```

`glaze-session` accepts `env`, `check`, `run` and `status`, plus `--backend`
(`auto`, `sway`, `glaze`, `none`) and `--nested`, and `-h`/`--help`.

---

## 8. Honest limitations

Glaze will not pretend to do what it does not do yet:

- **No native compositor.** `glaze-compositor` returns an explicit error instead
  of starting a fake session. Window management comes from an existing wlroots
  compositor in the first milestone.
- **No full session install.** There is no RPM, no optional session entry and no
  installer. Glaze never modifies your login manager, other desktops or boot
  settings. Development testing is nested and non-destructive.
- **No fabricated state.** The taskbar shows an empty list rather than a fake
  window list when no backend is attached; the launcher shows only real
  installed applications; no test result or screenshot in the repository is
  invented.
- **Architecture caveat.** Runtime validation on Fedora `x86_64` and `aarch64`
  requires a real Fedora host or VM; nothing from the Windows authoring host is
  claimed as a Fedora test. See [`PROJECT_STATUS.md`](../PROJECT_STATUS.md) for
  the exact, dated evidence.

---

## 9. Where to go next

- [`PROJECT_STATUS.md`](../PROJECT_STATUS.md) — what is built, tested and blocked.
- [`docs/architecture.md`](architecture.md) — how the components fit together.
- [`docs/adr/`](adr/) — architecture decisions.
- [`docs/contributing.md`](contributing.md) — build, test and contribution rules.
- [`docs/man/`](man/) — manual pages.
