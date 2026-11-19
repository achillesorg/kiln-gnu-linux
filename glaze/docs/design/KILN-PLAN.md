# Kiln GNU/Linux: master plan

Status: synthesis of the five design docs in this folder, written 2026-11-18.
Nothing here is built. Nothing is committed. Where the docs disagreed, this file
records the ruling (section 3) and wins over the sibling docs until they are
updated to match.

Inputs: `convergence.md`, `fresh-software.md`, `drivers.md`, `tool-suite.md`,
`windows-switcher.md`, plus the current live-build recipe in
`packaging/kilnlinux/image/` (branding hook 0200, `auto/config`, package lists).

Size key: S = under 2 days, M = 2 to 7 days, L = over a week.
Owner key (suggested model tier per task):
- **opus**: architecture, security-sensitive or hard-to-reverse work (signing keys, privileged daemon, compositor policy, kernel/driver gating, licence calls).
- **sonnet**: normal implementation against a clear spec (packaging, GUI pages, CLI, CI workflows, tests).
- **haiku**: mechanical work (string and file sweeps, package lists, fixture collection, doc tables, version-drift reports).

---

## 1. Vision

Kiln is Debian 13 made into a finished personal computer, the way Mint finished
Ubuntu, but aimed at people leaving Windows and at people who want one device for
pocket and desk. Five things together make it different from Debian and from Mint:
it feels like Windows on day one (Super key, taskbar, Win+E/R/L, Task View,
double-click a `.exe`, metric-compatible fonts, a "Move from Windows" wizard);
the same OS runs on a phone with the TilesDE shell and turns into the Glaze desktop
when docked to a monitor and keyboard, without restarting apps (DeX-style,
one compositor); it ships a Mint-style suite of first-party tools (Update Manager,
Driver Manager, Welcome, Sources, Reports, Backup, Software) that share one
safe privilege model and run on both phone and desktop; it is fresher than Debian
stable where freshness matters (backports kernel, Mesa, LibreOffice, pipewire, plus
Kiln rebuilds) while the Debian security base is never touched; and proprietary
drivers are first class, with Kiln packaging current Nvidia branches itself
(Debian is stuck at 550 in every suite), signed modules, Secure Boot handled,
an offline driver payload on the ISO and automatic fallback if a driver breaks boot.
Debian marks are removed where the trademark allows; Debian's copyright and
attribution stay.

## 2. Architecture overview

```
                        +------------------------------------------------+
  Hardware              |  x86_64 PC / laptop      |  arm64 phone (Mobian dev pkgs) |
                        +------------------------------------------------+
  Kernel + drivers      kiln-kernel (gated backports ABI) or 6.12 LTS
                        kiln-nvidia-modules-<branch>-<abi> (prebuilt, signed) / DKMS
                        firmware-* (non-free-firmware), microcode
  Base                  Debian 13 trixie + trixie-security  (never pinned away)
  Fresh layer           trixie-backports (explicit list) + Kiln rebuilds (~kiln13uN)
  Kiln repo             aptly -> rclone -> Cloudflare R2, suites trixie-stable / trixie-fresh,
                        components main / non-free, ed25519 subkey, kiln-archive-keyring
  Rollback              btrfs @ / @home / @snapshots / @var_log, snapper, grub-btrfs,
                        one apt hook (kiln-snapshot); rsync engine on ext4
  Session               glaze-compositor (Smithay), runtime policy Handheld | Desktop,
                        glaze-dock state machine, glaze-touch recogniser
    clients             Glaze shell (GTK4 + layer-shell)   TilesDE (Qt 6.8 / QML)
                        squeekboard (OSK), apps (GTK4, Qt, Flatpak, XWayland, Wine/Bottles)
  System tools          kiln-tools workspace: kiln-core, kiln-hwdb, kiln-helperd (root, D-Bus,
                        polkit per method), kiln-ctl, kiln-drivers CLI, GUIs on glaze-kit
  Installer             Calamares + kilndrivers module + offline /kiln-drivers payload
```

Data flow for privileged actions, everywhere: GUI or CLI (user) -> D-Bus
`org.kiln.Helper1` -> polkit check per method -> apt/dpkg/snapper inside one
locked job -> `kiln-snapshot` before risky transactions. Hardware decisions come
from one table, `kiln-hwdb`, read by Driver Manager, Calamares, the first-login
notifier and CI.

## 3. Contradictions between the design docs, and rulings

| # | Topic | Conflict | Ruling |
|---|---|---|---|
| R1 | Default Nvidia branch | `fresh-software.md`: 595 default on fresh, 580 on stable/legacy. `drivers.md`: 580 default, 590/595 opt-in. | **580 is the recommended branch on both channels** at launch (covers Maxwell to Blackwell, one branch to test). 595 is packaged in `trixie-fresh` and shown in Driver Manager as "newer". A row becomes `recommended` only once it has `tested` hardware entries (drivers.md rule). Driver choice is per GPU, not per channel; channel only controls which branches are visible. Re-verify branch numbers on nvidia.com before packaging. |
| R2 | Prebuilt vs DKMS modules | `drivers.md`: prebuilt signed modules for open and closed. `fresh-software.md`: never redistribute prebuilt modules, DKMS only. | **Open kernel modules (MIT/GPL dual-licensed) ship prebuilt and Kiln-signed** per kernel ABI. **Closed modules (legacy GPUs on 580) use DKMS** with the MOK flow until a licence review explicitly clears prebuilt closed modules (Ubuntu does ship them, so this is a review item, not a ban). DKMS is also the fallback for any ABI without a prebuilt package. |
| R3 | Kernel update path | `fresh-software.md`: pin `linux-image-amd64` from backports at 600, apt hook holds kernel if DKMS fails. `drivers.md`: gated metapackage `kiln-kernel`. `tool-suite.md`: Update Manager refuses kernel install when DKMS dry-run fails. | All three, layered. **Server side (primary):** `linux-image-*` metapackages are removed from the backports pin list; fresh users get kernels only through `kiln-kernel`, which `Depends` on one exact backports ABI package and is published only after CI built and boot-tested Nvidia open modules for that ABI. **Client side (backstop):** the Update Manager interlock and a dpkg hook refuse to make a kernel default if the installed Nvidia branch has no module for it. `kiln-kernel-lts` (Debian 6.12) stays installed in fresh. |
| R4 | Repo layout and pins | `drivers.md`: one `kiln` repo, suite `trixie-kiln`, component `kiln-nonfree`, pin 700, reprepro or aptly, `+kiln1` versions. `fresh-software.md`: aptly, suites `trixie-stable`/`trixie-fresh`, components `main`/`non-free`, `~kiln13uN` versions. | **fresh-software.md layout wins**: aptly, suites `trixie-stable` and `trixie-fresh`, components `main` and `non-free`. Nvidia 580 lives in `trixie-stable/non-free`, 595 additionally in `trixie-fresh/non-free`. Version suffix everywhere `~kiln13uN`. Pin table in section 5 merges both: Nvidia packages get an explicit 700 pin from `o=Kiln`. |
| R5 | Snapshot engine | `fresh-software.md`: snapper on btrfs, Timeshift rsync fallback, apt hook. `tool-suite.md`: own helper backends btrfs + rsync, rejects Timeshift. | **snapper for btrfs** (mature, grub-btrfs integrates), **Kiln rsync engine for ext4** (no Timeshift dependency), both behind the `SnapshotBackend` trait in `kiln-core`. **One** apt hook: `kiln-snapshot` (binary in `kiln-tools-common`, uses the same crate). When the helper already snapshotted for a job it sets `KILN_SNAPSHOT_DONE=1` so the hook skips. Calamares default: btrfs with `@`, `@home`, `@snapshots`, `@var_log`, `/boot` inside `@`. |
| R6 | Driver daemon | `drivers.md`: `kiln-driversd` with polkit `org.kiln.drivers.apply`, installs via PackageKit. `tool-suite.md`: `kiln-helperd` method `DriverApply(profile_id)` over signed `profiles.d`. | **One root daemon, `kiln-helperd`.** `DriverApply(row_id)` resolves against `kiln-hwdb` rows in `/usr/share/kiln-drivers/hwdb.d/` (drivers.md schema; "profile" = hwdb row). Helper calls apt directly under its own lock, **no PackageKit**. `kiln-drivers` CLI stays (calls the helper in a session, apt directly in live/Calamares). Polkit action is `org.kiln.helper.driver.apply`. |
| R7 | Driver Manager location | `drivers.md`: `kiln-apps/kiln-drivers`. `tool-suite.md`: `kiln-tools` workspace. | **`/Users/xavi/fedeos/kiln-tools/crates/kiln-drivers`** (GUI) plus `crates/kiln-hwdb`. kiln-apps keeps user apps only. |
| R8 | Nvidia arch | `fresh-software.md`: also arm64 for SBSA. `drivers.md`: x86_64. | **x86_64 only** for Nvidia until there is arm64 Nvidia hardware to test. |
| R9 | Keyring file name | `drivers.md` payload line uses `kiln-archive-keyring.gpg`. | **`/usr/share/keyrings/kiln-archive-keyring.pgp`** everywhere. |
| R10 | ISO default channel | Owner decision pending in `fresh-software.md`. | The owner asked for newer software: **desktop ISOs default to `fresh`**. Phone images default to `stable` with Mobian device kernels. |
| R11 | Kiln Store | `fresh-software.md`: Store lists Debian via PackageKit. `tool-suite.md`: Python Store is a stopgap, rewritten last as `kiln-software`. | Python Store keeps shipping until Phase 3 step 7; no new PackageKit work goes into it beyond fixing the polkit prompt. `kiln-software` replaces it (Conflicts/Replaces). |
| R12 | Super shortcuts vs convergence | `windows-switcher.md` moves workspaces off Super+1..9 and makes Super+Tab Task View. `convergence.md` adds Handheld gestures. | No real conflict: Windows bindings apply in Desktop policy, gestures in Handheld. Keymap lives in `glaze-wm/src/keybindings.rs`, one table; Handheld ignores desktop-only bindings when no keyboard is present. |

## 4. Phase plan (dependency order)

```
Phase 0 (branding, os-release, repo, keyring)
   |
Phase 1 (.deb for Glaze/apps/base, channels, pins, CI, snapshots, Windows quick wins)
   |                         \
Phase 2 (hwdb, helperd core,  Phase 4 (convergence prototype in QEMU)  <- can start after Phase 0;
   Nvidia pkgs, Driver Mgr,      |                                         needs Phase 1 debs only for P3+
   Calamares driver step)        |
   |                             |
Phase 3 (rest of tool suite) Phase 5 (real devices: x86 convertible, OnePlus 6, Fairphone 5)
```

Phase 4 steps P0 to P2 are additive in the compositor and can run in parallel with
Phases 1 to 3 (convergence.md section 9). Desktop milestones M0 to M5 in
`docs/decisions.md` keep priority.

---

### Phase 0: identity and infrastructure

**Decision.** Kiln becomes a distinct vendor: `os-release` `ID=kiln`, `ID_LIKE=debian`,
owned by a `kiln-base` package (not a sed in a hook), Debian visual marks removed,
and a Kiln apt repository with its own signing key exists before anything else ships.

**Rationale.** The current hook 0200 seds `/etc/os-release`, which `base-files`
overwrites on its next upgrade. A repo plus keyring is the prerequisite for every
later phase (debs, channels, Nvidia).

**Components / files.**
- `packaging/kilnlinux/deb/kiln-base/` (new): `dpkg-divert` of `/usr/lib/os-release`, `/etc/issue`, `/etc/issue.net`; `/etc/dpkg/origins/kiln`; `/etc/default/grub.d/10-kiln.cfg`; fastfetch config; Plymouth theme (moved from `stage.sh`).
- `packaging/kilnlinux/deb/kiln-archive-keyring/` (new).
- `scripts/repo/` (new): `aptly.conf`, `repo-init.sh`, `repo-add.sh`, `repo-publish.sh`, `repo-rollback.sh`, `rclone-sync.sh`.
- `packaging/kilnlinux/image/config/hooks/normal/0200-branding.hook.chroot` (shrinks to Flathub only), `auto/config`, `config/archives/kiln.list.chroot` + key.
- `stage-kiln/build` x86 pipeline: same archive entries.

`os-release` target content:
```
PRETTY_NAME="Kiln GNU/Linux 1 (Ember)"
NAME="Kiln GNU/Linux"
VERSION_ID="1"
VERSION="1 (Ember)"
VERSION_CODENAME=trixie        # kept: third-party repos (Docker, VS Code, etc.) build apt lines from it
KILN_CODENAME=ember            # placeholder name, owner picks
DEBIAN_CODENAME=trixie
ID=kiln
ID_LIKE=debian
HOME_URL="https://<kiln-domain>/"
SUPPORT_URL="https://<kiln-domain>/help"
BUG_REPORT_URL="https://<kiln-domain>/bugs"
LOGO=kiln-logo
```

**Work breakdown.**

| Task | Size | Owner |
|---|---|---|
| Generate offline ed25519 primary + 1-year signing subkey, revocation cert, runbook | S | opus |
| `kiln-archive-keyring` package; confirm trixie `sqv` accepts the key against a throwaway repo | S | sonnet |
| aptly scripts, suites `trixie-stable`/`trixie-fresh`, components `main`/`non-free`, local publish, test from arm64 VM | M | sonnet |
| R2 bucket + custom domain + cache rules (owner registers domain); upload order pool, by-hash, Release last | S | sonnet |
| `kiln-base`: os-release via `dpkg-divert`, issue files, dpkg origins, GRUB distributor cfg | M | sonnet |
| Branding sweep (checklist in section 6): grep image, package lists and Calamares branding for Debian marks | S | haiku |
| ID=kiln compatibility audit: grep kiln-apps, glaze, tilesde, Calamares config, hooks for `ID=debian` / `ID == "debian"` checks | S | haiku |
| Wire repo + keyring into live-build (arm64) and `stage-kiln/build` (amd64) | S | sonnet |

**Risks.** `ID=kiln` breaks scripts that test `ID` only (Docker's convenience
script, some vendor installers): mitigated by `ID_LIKE=debian` and keeping
`VERSION_CODENAME=trixie`. Changing `GRUB_DISTRIBUTOR` also changes the default
EFI directory, and Debian's signed GRUB looks for its config under `EFI/debian`:
Calamares and `grub-install` must keep `--bootloader-id=debian` (see checklist).
Single maintainer holds the key and bucket credentials: runbook plus offline
revocation cert.

**Acceptance tests.**
1. Live ISO (arm64 and amd64) boots; `cat /etc/os-release` shows `ID=kiln`, `ID_LIKE=debian`; systemd boot banner says "Welcome to Kiln GNU/Linux".
2. In the installed VM, `apt install --reinstall base-files` leaves Kiln `os-release` intact (diversion works).
3. `apt update` against the R2 repo is clean, a tampered `Release` fails, `repo-rollback.sh` restores a previous snapshot within 5 minutes.
4. `grep -ri debian` over the GRUB menu, Plymouth theme, `/etc/issue`, fastfetch output and Calamares branding returns only "based on Debian" attribution strings.
5. With Secure Boot on (x86 VM with OVMF secboot vars), the installed system boots through Debian's shim and GRUB.

---

### Phase 1: packaging and the fresh channel

**Decision.** Glaze, kiln-apps, TilesDE and kiln-base ship as `.deb` from the Kiln
repo into `/usr` (nothing under `/usr/local`). Two channels, selected by
metapackage: `kiln-channel-stable` and `kiln-channel-fresh` (desktop default).
Fresh = Debian base + backports for Mesa, LibreOffice, pipewire (explicit
package list) + Kiln rebuilds (wlroots 0.20 as `libwlroots-0.20`, sway 1.12,
flatpak 1.18) + kernels through `kiln-kernel` only (R3). Rollback (R5) lands in this
phase because fresh without rollback is unsafe. The cheap Windows-familiarity
items (keymap, fonts, templates) ship here because they are packaging and config.

**Rationale.** Without debs there is no upgrade path, no Update Manager, no
rollback of apps. Current versions (from `fresh-software.md` table, verified
2026-11-18): trixie kernel 6.12 vs backports 7.3.4; Mesa 25.0.7 vs 26.1.6;
LibreOffice 25.2 vs 26.8.2; pipewire 1.4.2 vs 1.6.9; wlroots 0.18.2 has an open
CVE fixed in 0.20.2. Core ABI (glibc, systemd, GTK 4.18, Qt 6.8) never moves.

**Components / files.**
- `packaging/kilnlinux/deb/{glaze,kiln-apps,tilesde,kiln-base,kiln-channel}/debian/`, `scripts/deb/make-orig.sh` (cargo vendor component tarball), `scripts/deb/build-in-container.sh`.
- `packaging/kilnlinux/apt/`: `kiln.sources`, `90-kiln-channel` templates, `fresh-set.list`, `gen-pins.sh`.
- `.github/workflows/deb-build.yml` (amd64 + arm64, `debian:trixie` container, lintian, twice-build + diffoscope).
- `kiln-snapshot` + `/etc/apt/apt.conf.d/80kiln-snapshot` (in `kiln-tools-common`; until Phase 3 exists it is a small standalone package `kiln-snapshot`).
- Calamares partition config: btrfs layout (coordinate with the Calamares work in progress).
- Windows quick wins: `crates/glaze-wm/src/keybindings.rs`, `packaging/sway/sway.conf`, fontconfig aliases in `kiln-base` (Liberation, Carlito, Caladea), LibreOffice defaults (MS formats on save), `~/Templates` in `/etc/skel`.
- `packaging/kilnlinux/image/stage.sh`: stop copying `/usr/local`; package lists install from repo.

**Work breakdown.**

| Task | Size | Owner |
|---|---|---|
| `glaze` source package (vendored crates, `cargo build --locked --frozen --offline`, rustc from backports in build chroot only) | L | sonnet |
| `kiln-apps` source package (7 binaries, Python Store via dh-python) | L | sonnet |
| `tilesde` source package (Qt 6.8) | M | sonnet |
| `cargo about` licence aggregation into `debian/copyright` | S | haiku |
| CI workflow, lintian gate, reproducibility rebuild | M | sonnet |
| Channel metapackages, pin generator, golden `apt policy` tests (amd64 + arm64 containers) | M | opus |
| Kiln rebuilds via sbuild: wlroots 0.20, sway 1.12, flatpak 1.18, published to `trixie-fresh` | M | sonnet |
| `kiln-kernel` / `kiln-kernel-lts` metapackages (fresh depends on exact backports ABI) | S | opus |
| `kiln-snapshot` + apt hook, snapper config, grub-btrfs on amd64 and arm64 (verify grub-btrfs in trixie, else generate entries) | M | sonnet |
| Calamares btrfs subvolume layout, snapper init at first boot | M | sonnet |
| Weekly version-drift report (tracker.debian.org / rmadison) | S | haiku |
| Windows keymap parity (Super+E/D/I/R/L, PrtSc, Ctrl+Shift+Esc, Super+1..9 = taskbar app, workspaces to Ctrl+Super+arrows) | S | sonnet |
| Fonts, fontconfig aliases, LibreOffice defaults, Templates, drive labels | S | haiku |
| `kiln-channel status|switch` CLI with snapshot-first downgrade | M | sonnet |

**Risks.** Partial upgrades mixing Mesa and libgbm: pins by source-package glob,
CI `apt-get -s dist-upgrade` must show zero kept-back fresh-set packages.
Backports security fixes lag (NotAutomatic): fresh set kept small. Vendor tarball
size (100+ MB): release asset, not git. Fresh-to-stable downgrade is weak: "supported
with snapshot restore as fallback".

**Acceptance tests.**
1. Clean trixie VM, add repo, `apt install glaze kiln-apps`: `glaze-session` starts, `glaze status` OK, `find /usr/local -type f` empty.
2. Two CI builds of `glaze` produce identical debs (`diffoscope` clean); lintian has no `E:`.
3. Fresh channel: `apt policy mesa-vulkan-drivers libreoffice-core` shows backports origin; `apt policy linux-image-amd64` shows Debian only; `kiln-kernel` pulls the CI-approved ABI. Stable channel: all Debian origins except `kiln-*`/`glaze*`/`tilesde*`.
4. Simulated `apt full-upgrade` never installs from sid/forky.
5. VM: install a deliberately broken Mesa from a test repo, reboot, pick the pre-snapshot in GRUB, desktop works; `/home` checksums unchanged.
6. `snapper list` shows a labelled pre/post pair after a kernel install; no snapshot after installing one trivial package.
7. Windows keys: Super+E opens Kiln Files, Super+L locks, Super+1 launches pinned taskbar app 1 (scripted in VM via control socket).

---

### Phase 2: drivers, Driver Manager, Calamares driver step

**Decision.** Kiln packages Nvidia itself from Debian's `nvidia-graphics-drivers`
packaging on new upstream tarballs: 580 (recommended, open modules Turing+,
closed via DKMS for Maxwell/Pascal/Volta) and 595 (fresh, opt-in), Kepler and
older on nouveau/NVK. Secure Boot keeps Debian's signed shim/GRUB/kernel and adds
one Kiln module-signing certificate enrolled via MOK. One `kiln-hwdb` table drives
everything. The minimal tool-suite core (`kiln-core`, `kiln-helperd` skeleton,
`glaze-kit` extraction) is pulled forward into this phase because Driver Manager
needs it (R6). Live session uses open drivers; Calamares installs proprietary
drivers from the offline payload.

**Rationale.** Debian has no Nvidia newer than 550.163.01 in any suite and 550
does not follow kernel 7.x (forum reports, verify by CI build), so "fresh" and
"Nvidia works" are the same project. Ubuntu/Mint solve this with
`ubuntu-drivers` + signed prebuilt modules; Debian with `isenkram` and
`nvidia-detect`. Kiln uses the same modalias primitive with its own richer table.

**Components / files.**
- `packaging/kilnlinux/nvidia/` (overlay, `uscan` watch, `supported-gpus.json` import), per-ABI build job, module-signing key in CI secret store (subkey-style custody as in Phase 0).
- `kiln-tools` workspace (`/Users/xavi/fedeos/kiln-tools`): `crates/kiln-core`, `crates/kiln-hwdb`, `crates/kiln-helperd` (D-Bus `org.kiln.Helper1`, methods `DriverApply`, `Install`, `Remove`, job objects), `crates/kiln-drivers-cli`, `crates/kiln-drivers` (GUI), `crates/kiln-ui`; `glaze/crates/glaze-kit` extracted from `apps/glaze-settings/src/ui/widgets.rs`.
- Data: `/usr/share/kiln-drivers/hwdb.d/*.toml` (package `kiln-hwdb`), polkit `/usr/share/polkit-1/actions/org.kiln.helper.policy`.
- `kiln-gpu-guard.service`, `/etc/grub.d/42_kiln_gpu` (open-drivers and safe-graphics entries), `/etc/modprobe.d/kiln-nvidia.conf`.
- Calamares module `kilndrivers` (QML page + Python job), placed between Partition and Users.
- ISO payload `/kiln-drivers/` flat repo built by `stage-kiln/build/build-all.sh`; `-lite` ISO variant without it.

**Work breakdown.**

| Task | Size | Owner |
|---|---|---|
| Nvidia source bump pipeline for 580 (lint, build, publish to `trixie-stable/non-free`) | L | sonnet |
| Per-ABI prebuilt open-module build + Kiln signing + publish; `kiln-kernel` CI gate | L | opus |
| Nvidia 595 branch in `trixie-fresh/non-free` | M | sonnet |
| Licence review: Nvidia redistribution, prebuilt closed modules, codecs (owner + counsel) | S | opus |
| `kiln-core` skeleton, backends as traits, fixtures runnable on macOS | M | sonnet |
| `kiln-helperd` skeleton: polkit check by bus name, job manager, apt backend (install/remove), input validation | L | opus |
| `kiln-hwdb` crate, schema, matcher, golden tests from sysfs fixture tarballs | M | sonnet |
| Generators: Nvidia ID lists per branch, firmware modalias map | M | sonnet |
| Fixture collection from real machines | S, ongoing | haiku |
| `glaze-kit` extraction from glaze-settings (decouple from `App`/`catalog::Page`) | M | sonnet |
| Driver Manager GUI (device groups, radio choices, details sheet, Apply, Revert, Advanced/GPU mode) | L | sonnet |
| MOK enrolment flow (UI dialog + `mokutil --import` + first-reboot text) | M | opus |
| `kiln-gpu-guard`, GRUB fallback entries, failed-boot counter | M | opus |
| First-login hardware notifier | S | sonnet |
| Offline payload builder + lite ISO + CI check that every hwdb row installs offline | M | sonnet |
| Calamares `kilndrivers` module | M | sonnet |
| Wayland on Nvidia in glaze-compositor (GBM, `nvidia-drm.modeset=1`, explicit sync `linux-drm-syncobj-v1`) | L | opus |

**Risks.** Build and signing infrastructure is ongoing load: start with x86_64,
one kernel ABI, one branch (580). Wrong PCI IDs install the wrong driver: only
rows with `tested` hardware may be `recommended`. Explicit-sync regressions on
580 with wlroots/Sway: Sway fallback needs `--unsupported-gpu`; Glaze compositor
tested on real Nvidia hardware (owner must provide one). ISO grows by 1.0 to 1.3 GB:
two ISO variants.

**Acceptance tests.**
1. On a CI machine with a deliberately withheld ABI, `apt-get install kiln-kernel` never selects a kernel lacking `kiln-nvidia-modules-*`.
2. `kiln-drivers detect --json` matches the golden file for every fixture; unknown devices produce no suggestion.
3. Real Nvidia PC, Secure Boot on: switch nouveau to 580 open in the GUI, enrol MOK on reboot, `nvidia-smi` works, `glaze status` reports the Nvidia renderer; "Revert to open drivers" returns to nouveau and the session starts.
4. Guard: `mv` the nvidia module away, two failed boots, third boot falls back to open drivers with a notification.
5. VM with NIC removed: Calamares with "Install proprietary drivers" checked completes; target has firmware and microcode packages; payload source removed from target apt sources.
6. Helper: a client passing a raw package list to `DriverApply` is rejected; an unauthorised caller gets a polkit denial (fake-authority container test).

---

### Phase 3: the rest of the tool suite

**Decision.** Rust, GTK4, no libadwaita, styled by `glaze-design`, one source package
`kiln-tools`, adaptive layouts so the same binaries run under TilesDE and Glaze.
Order: Update Manager, Welcome, Sources, System Reports, Backup, Software (Store
rewrite). Driver Manager already shipped in Phase 2. Windows-switcher medium items
land here because they reuse the same kit.

**Rationale.** Mint's value is a few friendly tools in front of apt; Kiln adds a
typed privilege model (no `pkexec <argv>`) and phone/desktop adaptivity. Update
Manager is first because it is what makes "fresh" safe for normal users.

**Components / files.** `kiln-tools/crates/{kiln-update,kiln-welcome,kiln-sources,kiln-report,kiln-backup,kiln-software,kiln-ctl}`; helper backends `apt.rs` (upgrade, levels), `kernel.rs`, `snapshot.rs` (snapper + rsync), `sources.rs` (deb822 only, `Signed-By` required), `firewall.rs`; data `/usr/share/kiln-tools/{update-levels.toml,mirrors.toml,report-checks.d/}`; Windows items: `kiln-run` (Win+R aliases `/usr/share/kiln/run-aliases`), lock (`ext-session-lock-v1`), Action Centre, `kiln-winrun` + Bottles, "Move from Windows" wizard, Task View in glaze-compositor/shell.

**Work breakdown.**

| Task | Size | Owner |
|---|---|---|
| Helper apt backend: upgrade with server-side re-simulation, Status-Fd progress | L | opus |
| Update Manager: five levels, kernels tab with DKMS interlock, channel toggle, history, Flatpak updates | L | sonnet |
| Snapshot backend in helper (snapper + rsync engine), Backup restore flow (btrfs default-subvolume swap, `kiln-restore.service`) | L | opus |
| Welcome (first-run checklist, "Where is X?" Windows sheet, drivers step, firewall) | M | sonnet |
| Sources (channel, mirror speed test, third-party deb822 editor) | M | sonnet |
| System Reports (missing firmware, microcode, codecs, crash reports; local only) | M | sonnet |
| Backup files engine (rsync first, restic later), drive-plug trigger | M | sonnet |
| Software (Store rewrite: Kiln curated, Flathub, Debian; replaces Python Store) | L | sonnet |
| `kiln-run`, lock, screenshot, Show desktop | M | sonnet |
| Action Centre, tray SNI overflow, taskbar pinning | M | sonnet |
| `kiln-winrun` (.exe dialog, one Bottles prefix per app; x86 native, arm64 FEX/Box64 experimental) | M | sonnet |
| Migration wizard (read-only import of folders, bookmarks, wallpaper, exported Wi-Fi XML) | L | sonnet |
| Task View overview | L | opus |
| Strings, a11y, translations template | S | haiku |

**Risks.** Restore flow can brick boot: tested with a deliberately broken
`/etc/fstab` and a broken Mesa before shipping. Polkit prompt not appearing (no
agent running in the Glaze session today): Welcome and `kiln-ctl doctor` check it.
Scope: ship each tool only when its helper methods pass container tests.

**Acceptance tests.**
1. Update Manager applies a level-4 kernel update: snapshot taken first, DKMS dry run passes, old kernel remains default until the new one boots once.
2. Sources rejects a `.sources` file with `trusted=yes` or without `Signed-By`.
3. Backup restores a broken-fstab system to bootable; restored file hashes equal originals.
4. All tool windows render at phone width (360 px) under TilesDE and at desktop width under Glaze (VM screenshots).
5. Double-clicking a `.exe` on x86 opens the Run dialog and a test installer runs in its own prefix.
6. `kiln-software` installs and removes a Flatpak and a deb; the Python Store is removed by Conflicts/Replaces on upgrade.

---

### Phase 4: convergence prototype in QEMU

**Decision.** One `glaze-compositor` runs the session and switches at runtime
between a Handheld policy (TilesDE as client) and a Desktop policy (Glaze shell as
client). Apps are re-configured, never restarted. Dock detection is a pure state
machine crate `glaze-dock`; the TilesDE gesture recogniser moves into `glaze-touch`
and `tilesde/compositor` stops being a separate compositor. When docked the phone
screen is a touchpad plus keyboard pad and turns off when a mouse is present;
squeekboard is the OSK. Everything is proven in the existing aarch64 VM first.

**Rationale.** A session switch (logout/login, as in early Phosh/Lomiri attempts)
kills apps, which defeats DeX-style use. The compositor already owns udev, DRM
and libinput, so it is the right place for dock facts.

**Components / files.** `glaze/crates/glaze-dock` (new), `glaze/crates/glaze-touch`
(new, from `tilesde/compositor/src/gesture.rs` and `protocol.rs`),
`glaze-compositor/src/{backend/udev.rs,input.rs,state.rs,control.rs}`,
`glaze-shell-core/src/form_factor.rs`, `glaze/scripts/vm/run-dual.sh` (new; does
not modify `fedeos/run.sh`), `docs/adr/ADR-00xx-convergence-single-compositor.md`,
`docs/screens/convergence/`.

**Work breakdown** (stages from `convergence.md` section 8).

| Stage | Task | Size | Owner |
|---|---|---|---|
| P0 | ADR; dual-head VM (`virtio-gpu-pci,max_outputs=2`, `virtio-multitouch-pci`); verify sysfs connector writes emit a hotplug uevent, else `sim-display` control command | S | sonnet |
| P0 | `glaze-dock` state machine with debounce, per-display memory, user override; unit tests on macOS | M | opus |
| P1 | Output roles + policy trait, `set-output-role` control command | M | opus |
| P2 | Touch in glaze-compositor + `glaze-touch` move (6 recogniser tests) | M | sonnet |
| P3 | Handheld policy; TilesDE runs on glaze-compositor on a portrait head | L | opus |
| P4 | Live profile switch on dock/undock, PIDs unchanged | L | opus |
| P5 | Phone-as-touchpad in compositor; `text-input-v3`, `input-method-v2`, `virtual-keyboard-v1`; squeekboard | M | sonnet |
| P6 | Form-factor hints (`X-Purism-FormFactor`, AppStream `display_length`), compat scaling, shared appearance store via portal `color-scheme`/`accent-color` | M | sonnet |
| P7 | Soak: 50 dock/undock cycles, 5 clients | S | haiku |

**Risks.** virtio-gpu may not emit hotplug events on forced connector writes
(verify in P0, fallback command exists). Two llvmpipe heads load the Mac VM:
keep 1280x800 heads, one soak at a time. squeekboard behaviour across two outputs
unverified (P5).

**Acceptance tests.** As in convergence.md section 8, notably:
1. P4: with `foot` and a GTK4 app running in Handheld, `echo on` to `Virtual-2`; within 1.5 s control `state` reports `profile=desktop`, both windows on head 1, PIDs unchanged; `echo off` returns them to head 0.
2. P7: 50 cycles, zero compositor crashes, constant Wayland client connection count, compositor RSS growth under 10 MB.
3. Desktop VM checks from the existing suite still pass after every stage.

---

### Phase 5: real devices

**Decision.** Bring-up order: H1 an x86_64 touch convertible with HDMI and
`SW_TABLET_MODE` (uses the existing x86 pipeline, real DRM hotplug, also an Nvidia
or Intel test bed); H2a OnePlus 6/6T for Handheld (best Mobian Trixie support);
H2b Fairphone 5 for real USB-C DisplayPort docking. Kiln reuses Mobian device
packages (kernel, firmware, tweaks) from a mirrored, pinned copy of Mobian's repo.
Phone images use the `stable` channel. Droidian/Halium is rejected.

**Rationale.** Mobian is Debian already, so device packages fit the Kiln base
unchanged. Fairphone 5 has mainline DP alt mode patches; OnePlus 6 reportedly
lacks DP alt mode in hardware (verify), so it cannot be the docking target.

**Components / files.** `/usr/share/kiln/devices/<compatible>.toml` (built-in key
lists, panel info), arm64 image flavour in `packaging/kilnlinux/image/` with a
device package list, Mobian mirror in the aptly setup (`mobian-trixie` snapshot,
pinned by origin), x86 convertible flavour in `stage-kiln/build`.

**Work breakdown.**

| Task | Size | Owner |
|---|---|---|
| Buy hardware (owner decision): x86 convertible, OnePlus 6/6T, Fairphone 5 | S | owner |
| Mobian repo mirror + pin, licence/attribution notes | S | sonnet |
| H1 device TOML, x86 image with TilesDE + Glaze + squeekboard | M | sonnet |
| H2a arm64 image with SDM845 device packages, boot to TilesDE | L | opus |
| H2b Fairphone 5 image, DP alt-mode docking test | L | opus |
| Phone power profile (`glaze-power`, power-profiles-daemon), suspend/resume | M | sonnet |

**Risks.** Fairphone 5 is "limited support" in Mobian; docking works only if the
shipped kernel carries the DP series. Lomiri's history shows unplug crashes:
unplug soak before any device release. Supply-chain dependency on Mobian: mirror
and pin.

**Acceptance tests.**
1. H1: tablet mode gives Handheld, unfold with HDMI gives Desktop; 20 cycles, no client dies.
2. H2a: boots to TilesDE lock screen; touch and OSK work; 10 suspend/resume cycles.
3. H2b: USB-C dock with HDMI, keyboard and mouse shows the Glaze desktop on the monitor within 3 s; unplug returns to TilesDE with the same apps.

---

## 5. Merged apt priority table (rulings R3, R4)

| Priority | Origin | Applies to |
|---|---|---|
| 1000 | `packages.mozilla.org` | `firefox*` |
| 990 | `o=Kiln` (both suites) | `kiln-*`, `glaze*`, `tilesde*` |
| 700 | `o=Kiln` `non-free` | `nvidia-*`, `libnvidia-*`, `firmware-nvidia-*`, `kiln-nvidia-*`, `libcuda*`, `libnv*` (generated list) |
| 600 | `o=Kiln,c=fresh` | names in `fresh-set.list` (fresh channel only) |
| 600 | `trixie-backports` | explicit list: Mesa set, LibreOffice set, pipewire set (fresh only). Not kernel metapackages. |
| 500 | Debian trixie, trixie-security, trixie-updates | everything else |
| 100 | `trixie-backports` | default for anything else; exact kernel ABI packages resolve from here through `kiln-kernel` |
| -1 | forky, sid | never configured |
| (phone) 990 | mirrored Mobian, by origin | device packages only |

## 6. Debian branding sweep checklist

Basis: Debian's trademark policy allows a derivative to say it is based on
Debian but not to present itself as Debian or use the logo as its own mark.
Copyright notices and licences are never removed. (Owner: confirm the policy text
on debian.org before release; this checklist is not legal advice.)

### Changes (Kiln replaces)

| Item | Where | Change | Package |
|---|---|---|---|
| OS identity | `/usr/lib/os-release` (`/etc/os-release` symlink) | `NAME`, `PRETTY_NAME`, `ID=kiln`, `ID_LIKE=debian`, URLs, `LOGO`; via `dpkg-divert`, not sed | kiln-base |
| Console banners | `/etc/issue`, `/etc/issue.net` | "Kiln GNU/Linux"; divert | kiln-base |
| dpkg vendor | `/etc/dpkg/origins/kiln` (`Vendor: Kiln`, `Parent: Debian`), `default` symlink to it | so `dpkg-vendor --derives-from debian` stays true | kiln-base |
| GRUB menu title | `/etc/default/grub.d/10-kiln.cfg`: `GRUB_DISTRIBUTOR="Kiln"` | menu shows "Kiln GNU/Linux" | kiln-base |
| GRUB theme / background | own theme; do not install `desktop-base` | no Debian swirl | kiln-base, package lists |
| Boot splash | Plymouth `kiln` theme set as default | replaces Debian themes | kiln-base |
| Live boot menu | live-build `--iso-application/--iso-publisher` (done), isolinux/GRUB live menu labels and splash | Kiln text and art | image config |
| Wallpapers, login art | Glaze assets only | no `desktop-base` alternatives | glaze-data |
| fastfetch / neofetch logo | `/etc/xdg/fastfetch/config.jsonc` + `logo.txt` (done, move into package) | Kiln logo | kiln-base |
| Calamares branding | `branding/kiln/branding.desc`, slideshow, product name, URLs | Kiln | calamares config |
| Hostname default | `kiln` (done) | | image |
| Bug reporting | `BUG_REPORT_URL`; `reportbug` not installed by default; Reports tool points to Kiln tracker | users do not file Kiln bugs at Debian | kiln-base, kiln-report |
| Store / Software labels | "System packages (Debian)" | attribution, not identity | kiln-software |
| Firefox | Mozilla build (no Debian ESR branding) | done via hook 0100 | image |
| About pages | Glaze Settings > About, Welcome: "Kiln GNU/Linux, based on Debian 13" with link | attribution | glaze, kiln-welcome |
| Hook 0200 | delete the os-release sed once kiln-base ships | | image |

### Must stay (licence, compatibility or Secure Boot)

| Item | Why it stays |
|---|---|
| `/usr/share/doc/*/copyright`, `/usr/share/common-licenses/` | licence obligations (GPL, DFSG licences) |
| Package names (`debian-archive-keyring`, `debianutils`, `debconf`, ...) and maintainer fields | renaming breaks dependencies; not branding |
| `debian/` directories in source packages, `~kiln13uN` versions that sort below Debian's | Debian packaging format; clean upgrades to Debian 14 |
| apt sources `deb.debian.org`, `security.debian.org`, `debian-archive-keyring` | the base is Debian; security updates come from there |
| `/etc/debian_version` | many tools read it; it is a data file, not a mark |
| `ID_LIKE=debian`, `VERSION_CODENAME=trixie`, `DEBIAN_CODENAME=trixie` | third-party repos and installers depend on them |
| EFI directory `EFI/debian` and `--bootloader-id=debian` in Calamares/`grub-install` | Debian's signed GRUB looks for its config there; changing it breaks Secure Boot boot. NVRAM entry label can be customised separately (verify) |
| Debian-signed shim, GRUB and kernel; `uname -v` showing a Debian build string | Secure Boot chain; rebuilding the kernel would lose Debian's signature |
| Debian strings inside package descriptions, man pages, changelogs | upstream content; editing is pointless churn |
| Attribution: "based on Debian" in About, Welcome, website | honesty and trademark-policy-compatible use |
| Mobian device packages' own names and copyright (phones) | licence and attribution |

Acceptance for the sweep: a scripted check in CI boots the ISO in QEMU, captures
the GRUB menu, Plymouth frame, tty `/etc/issue`, fastfetch output and a Calamares
screenshot, and greps text for "Debian" outside the allowed "based on Debian"
strings; any hit fails the build.

## 7. Open items needing the owner

1. Repo domain name and R2 account.
2. Release codename (placeholder `ember`) and version scheme.
3. Hardware: one real Nvidia PC (Turing or newer, ideally also a Pascal), x86 convertible, OnePlus 6/6T, Fairphone 5.
4. Licence review: Nvidia redistribution and prebuilt closed modules, codecs (H.264/HEVC in default image), Debian trademark text.
5. Confirm the Calamares work in progress will accept the btrfs layout and the `kilndrivers` module.

## 8. Unverified facts carried from sibling docs

Nvidia branch numbers (580 support window, 595 current production);
whether Debian 550 DKMS fails on kernel 7.3; `grub-btrfs` and `restic` versions in
trixie; virtio-gpu hotplug uevent on forced connector writes; squeekboard on two
outputs; Fairphone 5 DP alt mode in the kernel Kiln would ship; OnePlus 6 DP alt mode
absence; Mozilla repo arm64; GitHub arm64 runners for a private repo; NVRAM label
independence from `--bootloader-id`. Each is checked in the phase that first depends on it.
