# Kiln tool suite (first-party system tools)

Status: design proposal, 2026-11-18. Nothing here is implemented. Facts about the repo were read on this date; facts about Debian/Mint behaviour marked "verify" were not checked against live sources.

Sibling docs (written in parallel, they win on their own topic): `drivers.md` (driver concept, profiles), plus base-system, channels, phone-desktop convergence docs. Where this doc assumes an interface from them, it says so under "Assumptions".

## 0. Strategy in one paragraph

Mint did not rewrite Ubuntu; it put a small set of friendly, consistent tools in front of apt: Update Manager (mintupdate), Driver Manager (mintdrivers), Software Manager (mintinstall), Software Sources (mintsources), System Reports (mintreport), Welcome (mintwelcome), Backup (mintbackup) and Timeshift snapshots. Kiln does the same on Debian 13, in Rust, in the Glaze visual language, with one shared privilege model, and with a design goal Mint never had: the same binaries are adaptive, so they also run on the TilesDE phone and on a docked Glaze desktop.

## 1. Current state (read, not assumed)

| Item | State |
|---|---|
| Kiln Store (`/Users/xavi/fedeos/kiln-apps/kiln-store`) | Python 3, GTK4 + libadwaita, about 15 KB (`kiln_store/app.py` 10 KB, `backends.py` 5 KB), `meson.build`, CSS themes, desktop file. Per `kiln-apps/DECISIONS.md`: apt search and install/remove of one package verified, but via `sudo -n` (pkexec prompt not answerable in the test run). Flatpak listing empty (Flathub appstream data never arrived), so Flatpak search/install is **unverified**. Not packaged as .deb (`PACKAGING-TODO.md`). Depends on python3-gi + libadwaita, which conflicts with the "libadwaita-free" rule for Glaze apps. |
| Glaze settings widget kit | `glaze/apps/glaze-settings` is its own cargo workspace (GTK4 via gtk4-rs 0.11, `v4_14`, Linux-only target dependency, portable core tested on macOS). Kit lives in `src/ui/widgets.rs` (978 lines: `PageBuilder`, `Card`, `Row`, `row()`, `info_row`, `switch`, `segmented`, `choice`, `dropdown`, `slider`, `spin`, `entry`, `dialog`, `confirm`, `empty_state`, `order_list`) and `ui/wincontrols.rs`. It is coupled to the app's `App` type and to `catalog::Page`. No libadwaita. |
| Design tokens | `glaze/crates/glaze-design` (color.rs, css.rs, theme.rs, motion.rs, kinds.rs). Reusable as is. |
| D-Bus | `glaze-service` has an optional zbus 5 feature (`dbus`), off by default. zbus is the established choice. |
| Polkit agent | `mate-polkit` is in `kiln.list.chroot`; the Glaze session must start it (verify it does; Store test failed to show a prompt). |
| Image | `flatpak` in image list; Flathub remote and Mozilla apt repo provisioned by `provision-apps.sh` (decisions.md line 39). No snapshot tool (timeshift/snapper) installed. |

## 2. Cross-cutting decisions

### 2.1 Language and UI toolkit

Decision: every tool is Rust, GTK4 via gtk4-rs, no libadwaita, built from one shared widget kit styled by `glaze-design` tokens.

Rationale: matches glaze-settings (same dependency set, same MSRV, same theme code), removes the Python/libadwaita stack from the image, lets logic be unit-tested on the macOS authoring host.

Work: extract the kit (section 4).

### 2.2 Privilege model

Decision: GUI apps never run as root and never call `pkexec <shell command>`. One system service, `kiln-helperd`, owns every privileged mutation behind typed D-Bus methods, each guarded by its own polkit action.

Rationale: pkexec of arbitrary argv is how the current Store works; it is un-auditable and prompts per command. A typed API lets us validate inputs, serialise on the apt/dpkg lock, snapshot before upgrades, and show consistent auth prompts. Reads (what is upgradable, which drivers match, sources listing, hardware inventory) are unprivileged and done in `kiln-core` directly (`apt-get -s`, `apt-cache policy`, `dpkg-query`, `lspci`), so the root attack surface stays small.

Design:

- System bus name `org.kiln.Helper1`, object `/org/kiln/Helper1`, activated by D-Bus (`/usr/share/dbus-1/system-services/org.kiln.Helper1.service` with `SystemdService=kiln-helperd.service`; unit has `Type=dbus`, idle exit after 60 s).
- Bus policy `/usr/share/dbus-1/system.d/org.kiln.Helper1.conf`: any user may call; the helper itself authorises.
- Authorisation: for every method the helper calls `org.freedesktop.PolicyKit1.Authority.CheckAuthorization` with subject `("system-bus-name", {"name": <sender unique name>})`, flag `AllowUserInteraction=1`. Never trust a uid sent by the client.
- Policy file `/usr/share/polkit-1/actions/org.kiln.helper.policy`:

| Action id | Default (active session) | Used by |
|---|---|---|
| `org.kiln.helper.apt.refresh` | yes | Update Manager background refresh (safe: only `apt-get update`) |
| `org.kiln.helper.apt.upgrade` | auth_admin_keep | Update Manager |
| `org.kiln.helper.apt.install` / `.remove` | auth_admin_keep | Software, Driver Manager, Reports fixes |
| `org.kiln.helper.kernel.manage` | auth_admin_keep | Update Manager kernels tab |
| `org.kiln.helper.sources.write` | auth_admin | Sources |
| `org.kiln.helper.snapshot.create` | auth_admin_keep | Update Manager, Backup |
| `org.kiln.helper.snapshot.restore` / `.delete` | auth_admin (never keep) | Backup |
| `org.kiln.helper.driver.apply` | auth_admin_keep | Driver Manager |
| `org.kiln.helper.firewall.set` | auth_admin_keep | Welcome |

- Method shape (all return a job object path, never block):
  `RefreshIndexes()`, `ApplyUpgrade(as packages, b snapshot, s reason)`, `Install(as packages)`, `Remove(as packages, b purge)`, `KernelInstall(s package)`, `KernelRemove(s package)`, `SourcesWrite(s file_name, s deb822_text)`, `SourcesDelete(s file_name)`, `SnapshotCreate(s label)`, `SnapshotList()`, `SnapshotRestore(s id)`, `SnapshotDelete(s id)`, `DriverApply(s profile_id)`, `FirewallSet(b enabled)`, `Cancel(o job)`.
- Job object `/org/kiln/Helper1/Job/<n>`: properties `State` (queued, running, done, failed, cancelled), `Progress` (0..100, -1 unknown), `Message`; signals `Log(s line)` and `Finished(b ok, s summary)`. Jobs are owned by the sending bus name; others cannot cancel them.
- Input validation in the helper, not the client: package names match `^[a-z0-9][a-z0-9+.-]+$` and must exist in the apt cache; `SourcesWrite` accepts deb822 only, requires `Signed-By:` pointing at a file under `/usr/share/keyrings` or `/etc/apt/keyrings`, rejects `trusted=yes`, file name `^[a-z0-9-]+\.sources$`, written atomically under `/etc/apt/sources.list.d/`; `DriverApply` takes a profile id and resolves it against signed profile files in `/usr/share/kiln-drivers/profiles.d/` (the concept in `drivers.md`), never a package list from the caller.
- One global mutex per resource (apt/dpkg, snapshots). Requests queue. Environment scrubbed (`LANG=C.UTF-8`, `DEBIAN_FRONTEND=noninteractive`, `APT_LISTCHANGES_FRONTEND=none`), apt run with `-o APT::Status-Fd=3 -o Dpkg::Use-Pty=0` and status lines parsed for progress.
- Flatpak is not routed through the helper. System-scope Flatpak operations use Flatpak's own polkit actions (`org.freedesktop.Flatpak.*`), user-scope needs none. Rationale: avoids reimplementing Flatpak's authorisation; one fewer root code path.

Risks: polkit prompt never appears if no agent runs (mitigation: Welcome and `kiln-ctl doctor` check for the agent; image test below). Long upgrades outliving the 60 s idle exit (mitigation: helper stays alive while any job is running). Prompt fatigue (mitigation: `_keep` where safe).

Work: helperd skeleton + polkit check + job manager M; apt backend L; sources backend S; kernel backend M; policy/D-Bus/systemd files S.

### 2.3 Where state lives

| Kind | Path |
|---|---|
| Per-user config | `~/.config/kiln/<tool>.toml` (tool = update, drivers, backup, welcome, sources-ui). Glaze desktop settings stay in `~/.config/glaze/config.toml`; Welcome edits it through the same comment-preserving `toml_edit` code glaze-settings uses. |
| Per-user state/cache | `~/.local/state/kiln/` (`welcome.done`, update history, last refresh, dismissed update ids), `~/.cache/kiln/` (mirror speed results, AppStream thumbnails) |
| System config | `/etc/kiln/update.toml` (default level, auto-refresh interval, snapshot policy, kernel keep count), `/etc/kiln/backup.toml` (snapshot schedule) |
| System state | `/var/lib/kiln/snapshots/` (index.json plus snapshot data for the rsync backend), `/var/lib/kiln/jobs/` (last 50 job logs), `/var/log/kiln/helperd.log` |
| Shipped data | `/usr/share/kiln-tools/` (mirrors.toml, update-levels.toml, report-checks.d/, welcome checklist), `/usr/share/kiln-drivers/profiles.d/` |

User config is the user's preference; anything that changes the system goes through the helper. Config files are TOML, `serde` + `toml`, unknown keys preserved.

### 2.4 Packaging

One source package `kiln-tools`, binary packages: `kiln-tools-common` (kiln-ctl, kiln-helperd, polkit/D-Bus/systemd files, shared data), `kiln-update`, `kiln-drivers`, `kiln-welcome`, `kiln-sources`, `kiln-backup`, `kiln-report`, `kiln-software`, and metapackage `kiln-tools`. Built with `cargo` offline against vendored crates (Debian policy: vendor tarball, `debian/copyright` per crate class); installs to `/usr`. Desktop ids `org.kiln.Update.desktop` etc. match the existing `org.kiln.<Name>` scheme. Tool group in Glaze start menu: "Administration". Replaces `kiln-store` (Conflicts/Replaces once `kiln-software` ships).

Install to `/usr/local` from `build-all.sh` as an interim, like other kiln-apps. Image: add `kiln-tools` to `kiln.list.chroot`, and drop nothing from Debian's own tools since none overlap (no `software-properties`).

### 2.5 Test approach (all tools)

1. `kiln-core` is portable (no GTK, no D-Bus): trait-based backends (`AptBackend`, `SnapshotBackend`, `PolkitAuthority`, `Clock`, `Runner`), unit tests with fixtures (captured `apt-get -s` output, `apt-cache policy`, `lspci -nn`) run on macOS with `cargo test`.
2. `kiln-helperd` integration tests run in a Debian 13 container/VM on a private `dbus-daemon` with a fake polkit authority (allow/deny) and a scratch `chroot` for apt (`apt-get -o RootDir=`). Never on the Mac host, never against the live apt of the dev VM except the final smoke test.
3. UI: each page is built from the kit; render-smoke tests under `xvfb`-less `GSK_RENDERER=cairo` with Sway headless, screenshot via `scripts/vm/shot.sh`, per the Glaze rule that progress equals VM screenshots plus scripted checks.
4. `kiln-ctl` (headless CLI over the same core + D-Bus client) makes every flow scriptable: `kiln-ctl update plan --json`, `kiln-ctl drivers list`, `kiln-ctl snapshot create`. UI acceptance tests drive the CLI; UI only needs screenshot checks.

## 3. Order of work (value / effort)

| # | Piece | Value | Effort | Why this position |
|---|---|---|---|---|
| 0 | Workspace, kiln-core, kiln-ui kit extraction, kiln-helperd skeleton | enabling | L | everything depends on it |
| 1 | Update Manager | very high | L | Mint's defining tool; lets us ship "newer than stock" safely with snapshots and kernel handling |
| 2 | Driver Manager | very high | M | owner priority (proprietary drivers); mostly UI over `drivers.md` profiles + helper `DriverApply` |
| 3 | Welcome | high | S-M | cheap; Windows-switcher checklist is the first impression; reuses glaze-settings catalog |
| 4 | Sources | high | M | channels and mirror picker are how "newer software" is exposed; needed by Update Manager "Channels" |
| 5 | System Reports + Report an issue | medium | M | catches missing firmware/codecs/microcode; support funnel |
| 6 | Backup | medium | M-L | system snapshots part is already built in step 1; file backup UX is extra |
| 7 | Software (Store rewrite) | high but costly | L | Python Store works as stopgap; rewrite last, after the shared pieces exist |

Rule: nothing ships in the image until its helper methods pass the container tests.

## 4. Shared code: workspace, crates, UI kit

Decision: a new cargo workspace `/Users/xavi/fedeos/kiln-tools` (sibling of `kiln-apps`, own git repo, no remote), with `glaze-design` and a new `glaze-kit` as path dependencies from the Glaze repo.

Rationale: tools evolve at their own pace, are Mint-style first-party but not part of the compositor workspace; Glaze's own workspace excludes GTK apps for the same macOS reason, so we copy that pattern (portable crates default, GTK crates Linux-only).

### 4.1 Directory tree

```
kiln-tools/
  Cargo.toml                      workspace (portable members) + exclude of Linux-only
  rust-toolchain.toml             channel pinned like glaze-settings (rust-version 1.92)
  deny.toml                       licence + advisory gate (cargo-deny)
  debian/                         source package kiln-tools
  data/
    dbus/org.kiln.Helper1.service, org.kiln.Helper1.conf
    polkit/org.kiln.helper.policy
    systemd/kiln-helperd.service, kiln-update-refresh.{service,timer}   (user units)
    mirrors.toml, update-levels.toml, welcome-checklist.toml
    report-checks.d/*.toml
    desktop/org.kiln.Update.desktop ... (one per tool)
    icons/                        own flat paper icons (no third-party art)
  crates/
    kiln-proto/                   D-Bus interface definitions + client proxies (zbus), portable types
    kiln-core/                    portable logic, no GTK, no root
    kiln-helperd/                 root daemon, binary + lib (Linux)
    kiln-ctl/                     headless CLI, uses kiln-core + kiln-proto
    kiln-ui/                      GTK4 kit + app scaffold (Linux target dep, same pattern as glaze-settings)
    kiln-update/ kiln-drivers/ kiln-welcome/ kiln-sources/
    kiln-backup/ kiln-report/ kiln-software/          one GUI binary each
  tests/
    fixtures/ (apt-get -s outputs, lspci, policy files)
    container/ (Dockerfile.debian13, run-helperd-tests.sh)
```

Glaze side: new crate `glaze/apps/glaze-kit` (extracted from `glaze-settings/src/ui/widgets.rs` and `wincontrols.rs`). `kiln-ui` re-exports it and adds Kiln-specific widgets. Rationale for the split: glaze-settings and the Kiln tools must look identical; one kit owned by Glaze prevents drift. If extraction is rejected, fall back to a one-time copy with `PageBuilder`/`Card`/`Row` and a note, at the cost of drift.

### 4.2 Cargo.toml sketch (workspace)

```toml
[workspace]
resolver = "2"
members = [
  "crates/kiln-proto", "crates/kiln-core", "crates/kiln-ctl",
  "crates/kiln-helperd",
]
# GTK crates only build on Linux; included via `--workspace --exclude` on macOS CI
# or listed in default-members only on Linux (see scripts/check.sh)
exclude = []
default-members = ["crates/kiln-proto", "crates/kiln-core", "crates/kiln-ctl"]

[workspace.package]
version = "0.1.0"
edition = "2021"
rust-version = "1.92"
license = "GPL-3.0-or-later"     # decision: tools are GPL-3+ like kiln-store; kit is MIT OR Apache-2.0 and may be linked

[workspace.dependencies]
glaze-design = { path = "../glaze/crates/glaze-design" }
glaze-common = { path = "../glaze/crates/glaze-common" }
glaze-kit    = { path = "../glaze/apps/glaze-kit" }
kiln-proto   = { path = "crates/kiln-proto" }
kiln-core    = { path = "crates/kiln-core" }
kiln-ui      = { path = "crates/kiln-ui" }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
toml = "0.8"
toml_edit = "0.22"
zbus = { version = "5", default-features = false, features = ["tokio"] }
tokio = { version = "1", features = ["rt", "process", "sync", "time", "macros"] }
thiserror = "2"
tracing = "0.1"
gtk4 = { version = "0.11", features = ["v4_14"] }     # Linux only, via target dep in kiln-ui

[profile.release]
lto = "thin"
codegen-units = 4
```

`kiln-ui/Cargo.toml` puts `gtk4` and `glaze-kit` under `[target.'cfg(target_os = "linux")'.dependencies]`, exactly like glaze-settings, so `cargo test` for the portable layer works on the Mac. Each GUI crate is `[[bin]]` with `main()` gated `#[cfg(target_os = "linux")]` and a stub on other hosts.

### 4.3 Crate responsibilities

- `kiln-proto`: D-Bus interface trait (`#[zbus::proxy]` client, `#[zbus::interface]` server stub), job types, error enum with stable D-Bus error names (`org.kiln.Helper1.Error.NotAuthorized`, `.Busy`, `.InvalidArgument`, `.Failed`). No logic.
- `kiln-core`: modules `apt` (parse `apt-get -s`, `apt-cache policy`, `dpkg-query`; compute update list), `levels` (classification rules), `kernels`, `flatpak` (CLI wrapper `flatpak ... --columns`), `sources` (deb822 parse/serialise, mirror ranking), `drivers` (hardware match against profile files per `drivers.md`), `snapshots` (trait + metadata), `backup` (engine trait), `report` (checks, redaction), `config` (typed config load/save), `runner` (command execution trait for fakes), `progress` (apt Status-Fd parser). All pure or trait-injected.
- `kiln-helperd`: `main.rs`, `authority.rs` (polkit client), `jobs.rs`, `backends/{apt,kernel,sources,snapshot,driver,firewall}.rs`, `iface.rs` (implements `kiln-proto` server side). Only crate that links root-only code; `#![forbid(unsafe_code)]`.
- `kiln-ctl`: `kiln-ctl update|drivers|sources|snapshot|report|doctor` with `--json`.
- `kiln-ui`: described next.

### 4.4 Common UI kit (`glaze-kit` + `kiln-ui`)

Decision: three layers.

1. `glaze-kit` (extracted): `PageBuilder`, `Card`, `Row`, `info_row`, `switch`, `segmented`, `dropdown`, `entry`, `dialog`, `confirm`, `empty_state`, `order_list`, window controls, CSS loader from `glaze-design` tokens, light/dark following `~/.config/glaze/colour-scheme`. The only change: replace the `App` and `catalog::Page` coupling with a small trait `KitHost { fn icon(&self, name, size, tone) -> gtk::Widget; fn css(&self) -> &CssHandle }` and a `PageMeta { icon, title, blurb }` struct.
2. `kiln-ui` additions (used by at least two tools, else it stays in the tool):
   - `AppShell`: window with sidebar tabs (Mint-style left list) plus content, header with refresh/menu; collapses to bottom tab bar below 600 px width (the phone/dock convergence rule; the kit exposes a width-class signal `Compact | Regular | Wide`, GTK4 has no breakpoint widget without libadwaita, so we implement our own `AdaptiveBox` on `size-allocate`).
   - `JobView`: subscribes to a helper job (signals `Log`, `Finished`), shows progress bar, expandable log, Cancel; one implementation for all tools.
   - `AuthGate`: shows a calm "Waiting for authentication" state if polkit prompt is pending; detects "no agent" after 5 s and explains it.
   - `PackageList`: virtualised `gtk::ListView` with columns (checkbox, name, versions, size, origin badge), sortable, used by Update Manager, Software and Driver Manager details.
   - `Badge` (level, origin: Debian, Backports, Kiln, Flathub, Proprietary), `Toast`, `StatusBanner` (info/warn/error, with action button), `Timeline` (snapshot list), `SizeLabel`, `ProgressRing`.
   - `Wizard` (page stack with back/next, used by Welcome and Backup setup).
   - Accessibility: every control sets accessible label/role (kit already does so for buttons); keyboard-only operation is an acceptance test.
3. Per-tool pages in each GUI crate.

Style: Glaze Clay tokens (bg #FAF9F5, accent #D97757 per kiln-apps theme), flat paper cards, 1 px hairlines, no shadows; Glaze Classic alternate supported automatically because the kit consumes tokens.

Work: extraction M; AppShell + adaptive L-ish (M); JobView/AuthGate S; PackageList M; remaining widgets S each.

Risks: gtk4-rs without libadwaita means no ready-made adaptive widgets (accepted, own implementation, tested at 360, 600, 1280 px); extraction touches glaze-settings (needs user approval before editing Glaze repo; keep a copy-first fallback).

Acceptance tests: `cargo test -p kiln-core` green on macOS; glaze-settings still builds and its screenshots are unchanged after the extraction (pixel diff within tolerance); a demo binary `kiln-ui-gallery` renders every widget at 3 widths in light and dark, screenshots archived in `docs/screens/tool-suite/`.

## 5. Tool 1: Update Manager (`org.kiln.Update`)

### 5.1 Purpose

One place for all updates (Debian packages, Kiln packages, backports, firmware, Flatpaks, kernels), with Mint's trust model: show what changes, classify risk by level, snapshot before risky changes, make kernels and drivers a managed activity rather than a surprise.

### 5.2 Levels

Decision: five levels, defined in shipped data `/usr/share/kiln-tools/update-levels.toml` so they are tunable without a rebuild. Rules, evaluated top-down per package, first match wins:

| Level | Meaning | Rule | Default |
|---|---|---|---|
| 1 Safe | Security fixes and Kiln's own tools | origin `Debian-Security`, or package in Kiln repo and name `kiln-*`/`glaze-*` | selected |
| 2 Normal | Point-release updates in the stable pocket | origin `Debian` suite `trixie` or `trixie-updates`, no new dependencies installed, no removals | selected |
| 3 Newer | Backports and Kiln "newer software" channel (browser, Mesa, office, etc.) | origin `trixie-backports` or Kiln newer channel, or Mozilla repo | selected after the user opts in at first run (Welcome), else shown unselected |
| 4 Careful | Needs reboot or can break boot/graphics: kernel, firmware, bootloader, systemd, glibc, libc6, Nvidia and other DKMS driver packages, `*-firmware`, microcode | name/glob list plus `dkms` dependency check | unselected, snapshot forced, reboot prompt |
| 5 Risky | Update requires removing a package or installing new dependencies outside the update's own family | from `apt-get -s` showing `Remv` lines or new `Inst` not in the same source package family | unselected, red badge, confirm dialog |

Flatpak updates are shown in a separate "Flatpak" group, never mixed into levels (Flatpak updates do not touch the base system), but are in the same Apply button.

Users can set a minimum visible level and "ignore this package/version" (stored in `~/.config/kiln/update.toml`). Level classification code lives in `kiln-core::levels` and has table-driven tests with fixture `apt-get -s` outputs.

### 5.3 UI sketch (Regular width)

```
+--------------------------------------------------------------+
| Update Manager                      [Refresh]  [Menu]        |
+----------+---------------------------------------------------+
| Updates  | Banner: Last checked 14 min ago. 23 updates.      |
| Kernels  | [Snapshot before updating: ON]   Channel: Stable+ |
| Flatpak  |---------------------------------------------------|
| History  | [x] Select all safe (levels 1-3)                  |
| Channels | [ ] name           version          size   level    |
|          | [x] linux-libc-dev 6.12.48-1 ->..   2.1 MB  L4 !   |
|          | [x] firefox        140.x -> 141.x   70 MB   L3    |
|          | [x] openssl        3.5.x -> ...      2 MB   L1    |
|          |---------------------------------------------------|
|          | Details panel: changelog, origin, why this level  |
|          |---------------------------------------------------|
|          | 22 selected, 140 MB            [Install updates]  |
+----------+---------------------------------------------------+
```

Compact width: sidebar becomes bottom tabs, details open as a full-screen sheet.

Tabs: Updates, Kernels, Flatpak (updates and installed), History (job log, snapshots taken, reboot-needed flag), Channels (embeds the Sources channel toggles, section 8).

### 5.4 Flow

1. Refresh: user systemd timer `kiln-update-refresh.timer` (default every 4 h, 10 min after login, skipped on metered connection via NetworkManager `Metered` property) calls `RefreshIndexes()` (polkit action `apt.refresh`, yes, no prompt), then `kiln-core` computes plan with `apt-get -s dist-upgrade` unprivileged and writes `~/.local/state/kiln/updates.json`. The Glaze tray shows a count badge by watching that file or a `org.kiln.Update1` user-bus notification (small `StatusNotifierItem` via the existing `glaze-service` tray watcher; implementation detail left to the tray owner).
2. Apply: client sends the package list selected, `snapshot=true` for L4/L5 forced and L1-3 per policy (default: snapshot only if more than 10 packages or any L4/L5). Helper creates snapshot (section 9 engine), runs `apt-get install --only-upgrade`-style transaction for the exact set, with `dist-upgrade` semantics only for L5 after explicit confirm. It re-simulates server-side and aborts with `InvalidArgument` if the real transaction differs from what the client was shown (removals appear that were not listed).
3. After apply: parse `/var/run/reboot-required`, show "Restart to finish", restart via logind (`org.freedesktop.login1.Reboot`, no helper needed). Offer offline-style apply ("install on shutdown") as stretch only.
4. Failure: show log, mark snapshot as "failed update", offer rollback.

Config conflicts (dpkg conffile prompts): always `--force-confold` and list changed conffiles afterwards in History (Mint-like behaviour is to ask; for a switcher audience we keep the user's file and record a `.dpkg-dist` note). Decision recorded; revisit if base system ships many conffile changes.

### 5.5 Safe snapshots

Decision: snapshot engine is a helper backend with two implementations selected at runtime: `btrfs` (read-only subvolume snapshots of `@`, created next to `@home` layout, requires the installer to lay out btrfs; this is an assumption on the Calamares installer doc) and `rsync` (hardlinked `rsync -aAXH --link-dest` of `/` excluding `/home`, `/var/lib/flatpak` optional, caches, `/proc`, etc., into `/var/lib/kiln/snapshots` or a user-chosen disk). Default image filesystem recommendation: btrfs with zstd compression; ext4 users get the rsync fallback with a warning about time and space.

Rationale: Mint's Timeshift is exactly this pair. Writing our own thin engine in the helper avoids a Timeshift dependency (GTK3/Vala, Mint-maintained) and lets Update Manager treat snapshots as an API.

Restore: from Backup tool (section 9), runs from the live session via reboot into a one-shot restore step (btrfs: swap default subvolume and reboot; rsync: restore initramfs-less in a systemd unit `kiln-restore.service` before the display manager). Boot menu entries for btrfs snapshots through `grub-btrfs` (verify it is in trixie; otherwise generate GRUB entries ourselves) so a broken graphics driver can be undone without a working desktop.

Retention: keep last 5 automatic pre-update snapshots + 1 per week for 4 weeks + manual ones; prune when free space < 15%.

### 5.6 Kernel handling

Decision: Kiln ships `linux-image-amd64`/`-arm64` from the **backports** pocket when it is newer than stable by default (supports the "newer software, newer hardware" goal), keeps the stock stable kernel installed as fallback, and never removes the running kernel.

- Kernels tab lists installed `linux-image-*` and available ones (stable and backports), with: version, source (stable/backports), "running" and "fallback" tags, size, and Install/Remove.
- Auto-clean: keep running + newest + one previous (configurable `kernel.keep = 3`), removal runs only after the new kernel booted successfully once (marker written by a boot-success unit).
- Driver interlock (from `drivers.md`): before `KernelInstall`, helper checks `dkms status` and Nvidia profile: the new kernel's headers must be installed and DKMS modules must build (`dkms build -k <new>` dry run in a job step); on failure the install is refused with a plain message, and the old kernel stays default. This is the key proprietary-driver safeguard.
- Kernel updates are level 4 and always snapshot first.

### 5.7 Components and files

`crates/kiln-update/{main.rs,pages/{updates,kernels,flatpak,history,channels}.rs}`, `kiln-core::{apt,levels,kernels,flatpak}`, helper backends `apt.rs`, `kernel.rs`, `snapshot.rs`, `data/update-levels.toml`, user units, `org.kiln.Update.desktop`. Config `~/.config/kiln/update.toml` and `/etc/kiln/update.toml`.

### 5.8 Work breakdown

| Task | Size |
|---|---|
| apt plan parser and level classifier with fixtures | M |
| helper apt backend: refresh, apply, progress parsing, server-side re-simulation | L |
| snapshot backend btrfs | M |
| snapshot backend rsync | M |
| kernel backend and DKMS interlock | M |
| Flatpak updates (CLI wrapper, user and system scope) | S |
| UI pages (4 tabs, JobView) | L |
| refresh timer, tray count, reboot-required flow | S |
| History and rollback UI | S |

### 5.9 Risks

- apt output format changes: use `Debug::pkgProblemResolver`-free stable lines (`Inst`, `Remv`, `Conf`) and pin tests to trixie fixtures; later migrate to `rust-apt` (libapt-pkg bindings, verify packaging in trixie) if parsing hurts.
- Partial failure mid-dpkg: always `dpkg --configure -a` recovery job exposed as "Repair".
- Backports kernel with Nvidia: mitigated by DKMS interlock; residual risk when Nvidia needs a newer driver series than the kernel supports (drivers.md owns the matrix).
- Pinning mistakes making backports leak into the whole system: Sources tool is the only writer of pin files; tests assert pin output.

### 5.10 Acceptance tests

1. On the Debian 13 VM, `kiln-ctl update plan --json` lists the same package set as `apt-get -s dist-upgrade`, each with a level; fixture tests cover all five levels.
2. Apply a level-1 update set: helper job reaches `done`, no `Remv` lines, `dpkg --audit` clean.
3. With btrfs root: apply an L4 set, a snapshot exists before the transaction started (timestamps), rollback from the boot menu returns the old kernel.
4. Kernel install on a VM with Nvidia profile simulated by a dummy DKMS module that fails to build: install refused, old kernel remains default, message shown.
5. Server-side re-simulation: craft a plan that gains an unexpected removal; helper aborts with `InvalidArgument`.
6. Unauthorised user (polkit fake denies): all mutating calls return `NotAuthorized`; `RefreshIndexes` succeeds without a prompt.
7. UI keyboard-only walk-through; screenshots at 360, 800, 1280 px, light and dark.

## 6. Tool 2: Driver Manager (`org.kiln.Drivers`)

### 6.1 Purpose

Make proprietary drivers a first-class, reversible, safe choice: detect hardware, show which drivers are available and which is active, install/switch with one click, and recover if the new driver breaks the display.

### 6.2 Assumptions on `drivers.md`

Shipped signed profile files `/usr/share/kiln-drivers/profiles.d/*.toml`, each with: `id`, `name`, `vendor`, `match` (PCI ids or modalias globs), `packages` per arch, `conflicts`, `kernel_min`/`kernel_max`, `needs_reboot`, `licence` (free, proprietary), `blurb`, `risk`. Examples expected: `nvidia-current` (trixie `nvidia-driver`), `nvidia-open`/series from the Kiln newer channel, `nouveau`, `broadcom-sta`, `amd-firmware`, `intel-microcode`, `amd64-microcode`, `printer-hplip`. If `drivers.md` chose a different schema, `kiln-core::drivers` adapts; the helper API (`DriverApply(profile_id)`) does not change. Debian trixie facts (nvidia-driver in non-free, `nvidia-detect`, `nvidia-kernel-dkms`) must be verified in `drivers.md`, not here.

### 6.3 UI sketch

```
Driver Manager                                   [Re-scan]
Banner: Your graphics card can use a proprietary driver.
+--------------------------------------------------------+
| NVIDIA GeForce RTX 4060        PCI 10de:28a1           |
|  ( ) Nouveau (open source)         Active              |
|  (o) NVIDIA 5xx proprietary   [Recommended]  [Badge]   |
|       Better performance, CUDA, hardware video.        |
|       Needs restart.   Snapshot will be taken first.   |
|  ( ) NVIDIA open kernel module (newer cards)           |
+--------------------------------------------------------+
| Intel Wi-Fi AX210       firmware-iwlwifi   Installed   |
| CPU microcode           intel-microcode    Installed   |
+--------------------------------------------------------+
                          [Apply changes]  [Restart]
```

Radio groups for mutually exclusive profiles per device; plain-language licence badge ("Open source", "Proprietary") so users make an informed choice (owner wants proprietary support, not hiding it).

### 6.4 Architecture

- Detection (unprivileged): `lspci -nnk`, `/sys/bus/*/devices/*/modalias`, `ubuntu-drivers`-like logic reimplemented: match profile `match` rules; hybrid graphics detection via `/sys/class/drm` and `/sys/class/power_supply` (laptop).
- Apply: `DriverApply(profile_id)` -> helper resolves profile, runs apt install of the profile's package set and removes conflicts, rebuilds initramfs, writes modprobe blacklist for nouveau if needed (files `/etc/modprobe.d/kiln-nvidia.conf`), ensures `nvidia-drm.modeset=1` kernel parameter via `/etc/default/grub.d/kiln-nvidia.cfg` and `update-grub`. Wayland/Glaze specifics (GBM, `GLAZE` compositor env, Sway `--unsupported-gpu`) belong to drivers.md; the helper only applies profile-declared steps.
- Safety: snapshot forced; a "recovery" boot entry (nouveau/modeset off) stays in GRUB; a first-boot watchdog: if the session fails to start twice after a driver change, `kiln-driver-recover.service` reverts to the previous profile (state in `/var/lib/kiln/drivers/last-good.json`). Marker written by Glaze session on successful compositor start.
- Offline mode: profile packages may be on the live image (`/usr/share/kiln-drivers/offline/` apt repo) so Wi-Fi drivers (Broadcom) install without internet.
- Update Manager integration: level 4 includes profile packages; Update Manager asks Driver Manager (via kiln-core) whether a kernel update is compatible.

### 6.5 Files and work

`crates/kiln-drivers/`, `kiln-core::drivers`, helper `backends/driver.rs`, `data/` profile schema validator.

| Task | Size |
|---|---|
| Profile schema, loader, hardware matcher (fixtures: several real `lspci -nn` dumps) | M |
| Helper `DriverApply` with snapshot, conflict removal, initramfs, GRUB args | M |
| Recovery watchdog unit and GRUB recovery entry | M |
| UI (device cards, radio groups, JobView) | M |
| Hybrid graphics (render offload env, `kiln-ctl drivers prime`) | M, optional later |
| Offline driver repo on ISO | S |

### 6.6 Risks

Blank screen after an Nvidia install (mitigations: snapshot, recovery entry, watchdog); secure boot with unsigned DKMS (Kiln must enrol a MOK or document, owned by drivers.md; the tool shows a "Secure Boot is on" banner with the enrolment step, since a silent failure is the worst outcome); license/legal for redistribution of firmware (non-free-firmware is on by decision).

### 6.7 Acceptance tests

1. Matcher unit tests: fixtures for Nvidia desktop, Optimus laptop, AMD-only, VM (virtio-gpu): VM shows no proprietary offers.
2. Container test: `DriverApply("nvidia-current")` installs the expected packages in a chroot (no kernel modules), writes expected modprobe/grub files, is idempotent, and `DriverApply("nouveau")` reverts them.
3. Hardware test (user has Nvidia hardware or a PCI-passthrough VM): install, reboot to Glaze session, `nvidia-smi` works, session screenshot captured; then break the module deliberately, observe watchdog rollback after two failed starts.
4. Secure Boot on: banner appears and Apply is blocked until enrolment confirmed.

## 7. Tool 3: Welcome (`org.kiln.Welcome`)

### 7.1 Purpose

First-run guide and permanent "first steps" page (Mint Welcome equivalent), with a Windows-switcher checklist that makes Glaze feel familiar in two minutes.

### 7.2 Behaviour

- Autostart once: `/etc/xdg/autostart/org.kiln.Welcome.desktop` with `OnlyShowIn=Glaze;` and an exec guard `kiln-welcome --first-run` that exits if `~/.local/state/kiln/welcome.done` exists. Also reachable from Start menu and Update Manager menu. "Show at startup" toggle.
- Wizard pages (Wizard widget): 1 Welcome and language/keyboard (reuses locale settings of glaze-settings if present), 2 Look (light/dark, Clay vs Classic, accent), 3 Windows switcher checklist, 4 Safety (snapshots and updates: enable automatic snapshots, choose update channel, check drivers), 5 Apps (go to Software, install Flathub essentials), 6 Done.
- Adaptive: on phone (TilesDE, compact width) only pages 1, 2 and 4 appear; desktop-only items hide.

### 7.3 Windows-switcher checklist

Each row is a toggle or choice bound to an existing glaze-settings catalog entry (same ids, same `Effect`, same validation), so Welcome never duplicates setting logic. Proposed items (defaults chosen to match Windows 10 habits; owner may reorder):

| Row | Maps to | Default |
|---|---|---|
| Taskbar at the bottom with Start on the left | taskbar page | on |
| Windows-key opens Start menu; Win+E Files, Win+D desktop, Win+L lock, Win+Left/Right snap, Win+Tab overview | keys.rs bindings | on |
| Alt+Tab switches windows, Alt+F4 closes | keys.rs | on |
| Show file extensions and hidden-file toggle in Files | Kiln Files gsettings/xfconf | on |
| Natural scrolling off for mouse, on for touchpad (user choice) | input page | ask |
| Clipboard history (Win+V) | cliphist already in image | on |
| Screenshot key (PrintScreen) opens region capture | grim/slurp binding | on |
| Night light schedule | displays page | off, offer |
| Default apps: browser, mail, media (shows `org.kiln.*`) | defaultapps page | review |
| Sound, Wi-Fi, Bluetooth tray icons visible | taskbar tray | on |
| Firewall on (ufw, `FirewallSet`) | helper | on |
| Keep recently used files in Start menu | startmenu page | on |
| "Where is...?" glossary: Control Panel = Settings, Task Manager = Kiln Monitor (Ctrl+Shift+Esc), Device Manager = Driver Manager, Windows Update = Update Manager, System Restore = Backup > Snapshots, Programs and Features = Software | static card with launch buttons | n/a |

The glossary is the cheapest, highest-value piece. Item list lives in `data/welcome-checklist.toml` so it can be tuned without code.

### 7.4 Components

`crates/kiln-welcome`, uses `glaze-common` config API and the glaze-settings catalog (needs the catalog extracted from `apps/glaze-settings/src/catalog.rs` into the portable `glaze-common`/`glaze-kit` side; otherwise Welcome writes the same TOML keys through `toml_edit` with a unit test that compares keys against the catalog). Checklist state: `~/.config/kiln/welcome.toml` (shown/dismissed items), done marker in state dir.

### 7.5 Work

| Task | Size |
|---|---|
| Wizard scaffold, pages 1,2,6 | S |
| Checklist data file and row binding to catalog | M |
| Glossary card with launch actions | S |
| Firewall row (helper `FirewallSet`, ufw dependency) | S |
| Compact (phone) variant | S |
| Autostart, first-run guard, packaging | S |

### 7.6 Risks

Duplicated setting logic if the catalog is not shared (mitigation above); `ufw` blocking user services unexpectedly (default policy: deny incoming, allow outgoing, document); first-run window on a headless/SSH session (guard on `WAYLAND_DISPLAY`).

### 7.7 Acceptance tests

Fresh user on the VM: Welcome appears once, never again after Done; every checklist toggle changes the matching `~/.config/glaze/config.toml` key and the live shell reflects it (screenshot before/after); glossary buttons start the right app (`org.kiln.Monitor` etc.); on a 360 px window the compact variant has no horizontal scroll.

## 8. Tool 4: Sources (`org.kiln.Sources`)

### 8.1 Purpose

Edit where software comes from, safely: choose a mirror, enable channels (the user-facing way to get "newer software"), add third-party repos with proper key handling, manage Flatpak remotes.

### 8.2 Channels

Decision: expose channels, not raw lines. Each channel is a bundle of deb822 stanzas plus pins, defined in `/usr/share/kiln-tools/channels.d/*.toml` (shape defined with the channels doc; assumed here):

| Channel | Contents | Default |
|---|---|---|
| Stable | trixie, trixie-updates, trixie-security (main contrib non-free non-free-firmware per decisions.md) | on, cannot be disabled |
| Newer software | trixie-backports with pin priority 500 for a curated package list (kernel, mesa, firefox via Mozilla, etc.), 100 elsewhere (Debian default for backports is NotAutomatic, so it needs explicit pins; verify on trixie) | on |
| Kiln extras | Kiln repo (newer Glaze, kiln-* tools, newer apps) | on |
| Mozilla | Firefox regular, as provisioned today | on |
| Flathub | Flatpak remote | on |
| Testing | Debian testing, pin 1 | off, red warning, hidden unless "Advanced" |

### 8.3 UI sketch

```
Sources
 Mirror:  ( ) Automatic (deb.debian.org CDN)   ( ) Choose fastest   ( ) Custom
          [Test mirrors]  Germany  deb.example   42 ms   [Use]
 Channels:  Stable (always)  | Newer software [on] | Kiln extras [on] | Testing [off]
 Third party:  [Add repository...]  list of .sources with Signed-By, Remove
 Flatpak remotes: flathub [on] [Add remote...]
```

### 8.4 Architecture

- Parser/serialiser for deb822 in `kiln-core::sources` (round-trip preserving comments), fixture tested.
- Mirror test: unprivileged, parallel HEAD requests with timing against `mirrors.toml` (shipped list by country, plus the CDN), result cached in `~/.cache/kiln/mirrors.json`. Pick by median latency of 3 requests and a `Release` file freshness check (Date header and `Date:` field) so stale mirrors are rejected. Use `reqwest` (rustls) or a small `ureq`; no curl shell-out.
- Writes via helper `SourcesWrite/SourcesDelete`, files named `kiln-*.sources`; the tool never edits `/etc/apt/sources.list` (legacy file left empty with a header comment) and never edits other vendors' files except enable/disable via `Enabled: no`.
- Third-party key handling: user supplies a URL or file for a key; tool downloads, shows fingerprint, requires explicit confirmation (typed or checkbox), converts to `/etc/apt/keyrings/<name>.gpg` through the helper (`KeyInstall`, added to the interface), `Signed-By` set. Never `apt-key`. For Mozilla, verifies the fingerprint as `provision-apps.sh` does (shared constant in `kiln-core`).
- Branding: all new files and comments say Kiln, not Debian; suite names (`trixie`) and origins (`Origin: Debian`) stay, since they are technical and the apt archive is Debian's. Where Debian trademarks are used for fact (mirror names), keep them factual.
- After any write: helper job `RefreshIndexes`, and Update Manager gets refreshed.

### 8.5 Work

| Task | Size |
|---|---|
| deb822 parser/serialiser | M |
| Channel files + pin generation + tests | M |
| Mirror tester and ranking | S |
| Helper `SourcesWrite/Delete/KeyInstall` with validation | M |
| UI | M |
| Flatpak remotes via CLI | S |

### 8.6 Risks

Mis-pinned backports (strong tests: golden pin files, `apt-cache policy` checked in container); mirror list rot (list is data, update with the Kiln repo; fall back to CDN); users adding untrusted repos (fingerprint confirm plus explicit warning text).

### 8.7 Acceptance tests

Container: enable then disable "Newer software" produces exactly the golden files and `apt-cache policy mesa` shows backports 500 only for listed packages; helper rejects `trusted=yes` and missing `Signed-By`; mirror test against a local fake HTTP server ranks correctly and drops a stale mirror; legacy `sources.list` untouched.

## 9. Tool 5: System Reports and Report an issue (`org.kiln.Report`)

### 9.1 Purpose

Two jobs: (a) proactive "your system could be better" checks like Mint's System Reports; (b) a safe way to file a useful bug report.

### 9.2 Checks (plug-in data files)

Each check is a TOML file in `report-checks.d/` with: `id`, `detect` (typed probe: `package_missing`, `hardware_match`, `file_absent`, `service_inactive`), `title`, `explain`, `fix` (a helper operation, e.g. `Install(pkgs)`), `severity`. Initial set:

- Missing CPU microcode (`intel-microcode`/`amd64-microcode`).
- Missing firmware for detected devices (match modaliases to firmware packages; reuses the driver matcher).
- Missing language support for the chosen locale (check `check-language-support`-style by locale).
- Missing media codecs/hardware video acceleration packages (`vainfo` result for the GPU).
- No snapshot configured / last snapshot older than 30 days / low disk space.
- Proprietary driver available (links to Driver Manager).
- Firewall off.
- Updates not applied for 30 days; reboot required for more than 7 days.
- Failed systemd units; apt in a half-configured state (`dpkg --audit`).

Report tray: a quiet badge, never a popup; one "Fix" button per item running the helper.

### 9.3 Report an issue

Flow: user describes the problem, tool builds a diagnostic bundle, shows a full preview (every file, scrollable, with redactions highlighted), user chooses Save to file or Open the issue page (browser, URL has only title and version, body is copied to clipboard to avoid long/PII URLs). Nothing is sent automatically. No telemetry.

Bundle contents: `/etc/os-release`, `kiln-ctl versions` (Glaze, TilesDE if present, kiln-tools, kernel), `uname -a`, `lspci -nnk`, `lsusb`, loaded modules, `journalctl -b -p warning -n 500`, `journalctl -b -1 -p err -n 200` (previous boot for crashes), `glaze status`, `apt-cache policy` summary, `dkms status`, GPU driver info, `dpkg --audit`, sources list (URLs only, no credentials). Redaction pass in `kiln-core::report`: hostname, username (replaced with `user`), home path, IPv4/IPv6 (except link-local labels), MAC addresses, serial numbers, any `token=`/`key=`/`password=` patterns, `.netrc` never read. Redaction is unit tested with tricky fixtures.

### 9.4 Work

| Task | Size |
|---|---|
| Check engine and 8 checks | M |
| Redaction library + tests | S |
| Bundle collector, preview UI | M |
| Tray badge integration | S |

### 9.5 Risks

PII leakage (mitigation: preview-all, redaction tests, explicit consent, no auto-upload); noisy checks causing alert fatigue (severity thresholds, "dismiss forever" per check, stored in config).

### 9.6 Acceptance tests

Fixtures with seeded secrets and identifiers produce a bundle containing none of them (grep test); each check triggers and clears in a container with a package removed/added; preview shows exactly the bytes that end up in the archive (hash match).

## 10. Tool 6: Backup (`org.kiln.Backup`)

### 10.1 Purpose

Two familiar things in one tool: "System snapshots" (Timeshift-like, protects the OS) and "Files" (File History / Mint Backup-like, protects the user's data).

### 10.2 Tabs and UI sketch

```
Backup
 [System snapshots] [Files]
System snapshots:  Engine: btrfs   Free: 38 GB   Schedule: daily + before updates
  Timeline:  today 08:02 (auto) | before update 2026-11-17 | weekly ...
  [Create snapshot now]  [Restore...]  [Delete]  [Settings]
Files:  Back up to: [External drive WD 1TB v] or [Folder...]   Next: today 20:00
  Include: Documents, Pictures, Desktop, Music (checkboxes) + [Add folder]
  Mode: ( ) Simple copy (browsable in Files)   ( ) Versioned and encrypted
  [Back up now]   Restore: [Browse versions...]
```

### 10.3 Architecture

- System snapshots: UI over helper `Snapshot*` API (engine from section 5.5). Schedule: root systemd timer `kiln-snapshot.timer` reading `/etc/kiln/backup.toml`. Restore requires reboot-step; a clear explanation dialog ("Your files in Home are not changed; changes to system files since the snapshot are undone").
- Files, engine trait `BackupEngine { plan, run, list_versions, restore }`:
  - Decision: ship `rsync` simple-copy engine first (rsync is in the image path already; hardlinked dated folders via `--link-dest`, plain files you can open in Kiln Files, Windows File History feel), then add `restic` as the versioned/encrypted engine (CLI JSON output; verify trixie restic version with `apt-cache policy restic`). Runs unprivileged, so no helper involvement.
  - Triggers: user systemd timer, and on-plug via udisks2 `InterfaceAdded` for the chosen drive UUID (backup runs when the drive is connected, Windows-like). Skip on battery below 30% and on metered networks for remote targets.
  - Encryption key for restic stored via Secret Service (libsecret/KeePassXC in image), never in plain config; recovery key export dialog at setup.
  - Verification: weekly `restic check`/`rsync --checksum` sample, result in History.
- Excludes: caches, `~/.local/share/Trash`, Flatpak runtime dirs, `node_modules`-like globs configurable; Flatpak app data (`~/.var/app`) included by default.

### 10.4 Work

| Task | Size |
|---|---|
| Snapshot UI (list, create, delete, settings) | M |
| Restore flow (btrfs swap, rsync restore unit) | L |
| rsync files engine, timers, drive-plug trigger | M |
| restic engine and key handling | M |
| Backup setup Wizard | S |

### 10.5 Risks

Restore flow is the highest-risk code in the suite (can brick boot: needs the rsync restore to be tested with a deliberately broken system, fallback of GRUB entries); disk-full during snapshot (pre-check free space and refuse); users believing snapshots protect Home (UI says so explicitly).

### 10.6 Acceptance tests

Container/VM: create, list, delete, prune per retention; rsync restore of a broken `/etc/fstab` scenario boots after restore; files backup to a loop-mounted disk, plug event triggers a run, restored file hash equals original; backup skipped on low battery (simulated UPower value); restic key not present in any config file (grep).

## 11. Tool 7: Software (`org.kiln.Software`, Store rewrite)

### 11.1 Purpose

Browse, search, install and remove apps from Debian (apt), Kiln extras and Flathub, with one list, clear origin badges, and an "Installed" view. The Windows-switcher mental model: "the Microsoft Store" without ads.

### 11.2 Decisions

- Keep the Python Kiln Store as the stopgap until `kiln-software` reaches parity; do not invest in it, but fix its pkexec usage by pointing it at `kiln-ctl install` once helperd exists (S, optional).
- Rewrite in Rust with the kit. Data source: AppStream catalogs. Debian side `/usr/share/swcatalog/xml` and `/var/lib/swcatalog/xml` (via `appstream`-data package; verify trixie path and package name, since AppStream changed the directory name from `app-info` to `swcatalog`); Flathub appstream via `flatpak update --appstream` at first run, fixing the empty listing seen in the Store test (root cause unknown: likely missing remote or network at test time; first task is to reproduce).
- Install paths: apt via helper `Install/Remove` (polkit), Flatpak via CLI with `--noninteractive`, system scope default (Flatpak's own polkit), user scope option in settings.
- Preference rule when the same app exists in both: show both, default to Debian/Kiln for system-integrated tools, Flathub for apps where the upstream Flatpak is the official one (curated mapping file `data/app-preferences.toml`). Mint chose a similar curated approach.

### 11.3 UI sketch

```
Software        [ Search apps... ]                [Installed] [Updates->]
Categories: Internet  Office  Media  Graphics  Games  Dev  System
Featured row (curated, no paid placement)
+-------+ Firefox      Web browser        [Install]  badges: Mozilla
| icon  | 5 screenshots | summary | version | size | licence | source: [Mozilla v]
+-------+
```

Source picker on the app page when multiple origins exist. "Proprietary" badge visible (licence from AppStream `project_license`).

### 11.4 Work

| Task | Size |
|---|---|
| Reproduce/fix Flathub appstream retrieval, verify Flatpak search and install | S |
| AppStream parser (rust `appstream` crate or own XML via `quick-xml`), search index (reuse glaze-settings `search.rs` ideas, `tantivy` not needed, simple tokenized scoring) | M |
| apt backend via helper, Flatpak backend via CLI | M |
| Grid/list UI, app page, screenshots cache | L |
| Installed + queue view, JobView | M |
| Curated featured/preference data | S |
| Packaging, replaces `kiln-store` | S |

### 11.5 Risks

Large catalogs make cold start slow (cache index to `~/.cache/kiln/software-index`); two sources with different versions confuse users (badges and source picker); screenshots need network (lazy and cached).

### 11.6 Acceptance tests

VM with network: search "vlc" returns apt and Flathub entries; install a small apt package and a small Flatpak (each with progress and success), remove both; installing as normal user shows exactly one polkit prompt for apt and none or Flatpak's own for flatpak; offline: UI shows cached catalog and explains why install is disabled.

## 12. Cross-tool integration summary

- Update Manager -> Driver Manager (kernel compatibility), Sources (channels tab embeds), Backup/Snapshots (pre-update snapshot).
- Welcome -> Update Manager (choose channels, first refresh), Driver Manager (scan), Backup (first snapshot), Software.
- Reports -> any tool through "Fix" buttons and deep links `kiln-update --page kernels`, `kiln-drivers --device <pci-id>`.
- Phone/dock (DeX-style) convergence: all tools use the AppShell adaptive width class; on TilesDE they launch from the Tiles "Settings" group, same binaries; hardware/driver and snapshot tools hide where meaningless (no GRUB on some phones: helper reports capabilities via `GetCapabilities()` so UI hides rather than fails).
- Branding: no Debian logos or names in tool UIs except where factual (origin labels "Debian", licence texts); About dialogs say "Kiln GNU/Linux".

## 13. Roadmap, milestones and gates

| Milestone | Content | Exit criterion |
|---|---|---|
| T0 | Workspace, kiln-core skeleton, helperd skeleton, polkit round trip, kit extraction decision | fake-authority test passes; `kiln-ctl doctor` on VM shows agent running and helper reachable |
| T1 | Update Manager core (plan, levels, apply, snapshots btrfs) + minimal UI | acceptance tests 5.10 (1,2,3,5,6) |
| T2 | Kernels, Driver Manager | tests 5.10(4), 6.7 (1,2) and hardware test when available |
| T3 | Welcome, Sources | 7.7, 8.7 |
| T4 | Reports, Backup | 9.6, 10.6 |
| T5 | Software rewrite | 11.6 |

Total rough size: foundation L, Update L, Drivers M, Welcome M, Sources M, Reports M, Backup L, Software L. Sized in S/M/L as above; no dates promised.

## 14. Open decisions for the owner

1. Approve extracting `glaze-kit` and the catalog from `glaze/apps/glaze-settings` (touches the Glaze repo; currently the safest alternative is copy-first).
2. Licence for kiln-tools: GPL-3.0-or-later proposed (matches kiln-store); kit stays MIT OR Apache-2.0.
3. Root filesystem default in the installer: btrfs with `@`/`@home` (enables fast snapshots) versus ext4 plus rsync fallback.
4. Whether the backports kernel is the default (proposed yes) given Nvidia interplay.
5. Which repo hosts "Kiln extras" and how it is signed (needed by Sources and Update Manager level 1/3 rules).
