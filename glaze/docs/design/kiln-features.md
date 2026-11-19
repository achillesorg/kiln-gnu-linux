# Kiln GNU/Linux: additional feature plan

Status: proposal, 2026-11-19. Nothing here is built or committed.
Scope: features NOT already planned in `KILN-PLAN.md`, `drivers.md`, `fresh-software.md`,
`tool-suite.md`, `windows-switcher.md`, `convergence.md`. Where a feature touches a
planned tool, this doc only specifies the delta and links to the owner section.
Size: S under 2 days, M 2-7 days, L over a week. Priority: P0 blocks release, P1 launch
quality, P2 post-launch polish, P3 nice to have. Unverified facts are marked (verify).

## 0. Already planned (not redone)

Update Manager levels and snapper, Driver Manager and Nvidia/MOK, Welcome wizard, Sources, Reports, Backup, Software, fresh channel, aptly+R2 repo, deb CI, Debian branding sweep, Windows UX, convergence.

Gaps this doc fills (grep of all six docs finds no mention): firewall, AppArmor policy,
zram/earlyoom, fwupd, journald/fstrim hygiene, popcon/telemetry stance (only a one-line
mention in tool-suite), openQA-style UI tests, accessibility stack, devshell profiles,
laptop power tuning beyond `power-profiles-daemon`, self-heal, hardware first-boot pass.

## 1. Feature index

| # | Feature | Pri | Effort | Risk | New crate / package |
|---|---|---|---|---|---|
| F1 | kiln-setup: first-boot hardware pass | P0 | L | M | `kiln-setup` (Rust) |
| F2 | Update Manager delta: apt-hook snapshot gate, "known-bad" feed, rollback UX | P0 | M | M | `kiln-snapshot` (Rust port), data feed |
| F3 | Welcome/Tour delta: "Tour" mode, tips, switcher recipes | P1 | S | L | in `kiln-welcome` |
| F4 | kiln-care: self-heal and hygiene daemon | P0 | M | M | `kiln-care` (Rust) |
| F5 | Kiln Play: gaming profile and session | P1 | M | M | `kiln-play`, `kiln-profile-gaming` |
| F6 | Profiles: creator, dev, office, school, server-lite | P1 | M | L | `kiln-profile-*` metas |
| F7 | Laptop power and thermal tuning | P1 | M | M | `kiln-power` (Rust), `kiln-power-data` |
| F8 | Unified apt+Flatpak store delta: one transaction queue | P2 | M | M | in `kiln-software` |
| F9 | Backups delta: restic engine, cloud targets, restore drills | P2 | M | M | in `kiln-backup` |
| F10 | Security defaults: AppArmor, nftables, SB/MOK, updates, lock | P0 | M | M | `kiln-security-defaults` |
| F11 | Accessibility stack | P1 | M | L | `kiln-a11y-defaults`, Orca, glaze-a11y |
| F12 | i18n and l10n pipeline | P1 | M | L | Weblate, `kiln-l10n-*` |
| F13 | Legal and branding checklist delta | P0 | S | H | docs + CI check |
| F14 | CI and ISO pipeline: reproducible live-build, nightly, QEMU smoke | P0 | L | M | `.github/workflows`, `scripts/` |
| F15 | Telemetry stance: none, opt-in crash text only | P0 | S | L | `kiln-base` policy file |
| F16 | Phone-desktop continuity services (clipboard, files, notifications) | P2 | M | M | `kiln-link` |
| F17 | Memory and storage defaults (zram, earlyoom, fstrim, swap) | P1 | S | L | `kiln-tuning` |
| F18 | Firmware via fwupd in Update Manager | P1 | S | M | `fwupd`, Update Manager group |

Build order is in section 19.

## 2. F1 kiln-setup: first-boot hardware pass

**What.** One post-install run (first login, before Welcome) that probes hardware and
applies every safe fix in one pass, with one summary screen: GPU driver, firmware,
microcode, codecs, printers, Bluetooth, fingerprint, webcam, Wi-Fi, sensors. Mint makes
the user visit Driver Manager, Welcome, Sources separately; Kiln does it once.

**Why differentiates.** Proprietary drivers first-class means "zero questions": the
laptop works after first boot. Windows switchers expect OEM first-run "getting devices
ready". Runs identically on phone (compact UI, fewer rows).

**Not duplicating.** `drivers.md` 7 defines the detection table and 8 the Driver Manager
UI; `kiln-setup` is the orchestrator that calls the same `kiln-hwdb` and
`kiln-helperd` methods for all device classes in one transaction and handles the
post-install Calamares offline payload.

| Step | Probe | Action (through `kiln-helperd`, polkit once) |
|---|---|---|
| 1 GPU | `kiln-hwdb` PCI match | install profile (Nvidia branch, or Mesa only), MOK prompt queued if Secure Boot |
| 2 Firmware | `dmesg`/journal `firmware: failed to load` lines, `lspci`/`lsusb` ids | install `firmware-*` from the ISO payload, else apt |
| 3 Microcode | `/proc/cpuinfo` vendor | `intel-microcode` or `amd64-microcode` |
| 4 Codecs and media | none (always) | ensure Kiln codec meta, `libavcodec` full, VA-API driver by GPU (`intel-media-va-driver-non-free`, `mesa-va-drivers`) |
| 5 Printers | `ipp-usb` + `avahi` browse 5 s | list found devices; user taps to add (never auto-add) |
| 6 Bluetooth | `rfkill`, `bluetoothctl` | enable service, fix `rfkill` soft block, load `btusb` quirks from table |
| 7 Fingerprint | `lsusb` match vs `libfprint` supported list | install `fprintd` + PAM profile, offer enrol |
| 8 Webcam, audio | `v4l2-ctl`, `wpctl` | install IPU6 stack if listed (verify), set default sample rate quirks |
| 9 Laptop | DMI chassis type | enable F7 power profile and touchpad gesture defaults |
| 10 Report | all of the above | write `/var/lib/kiln/setup-report.json`, show summary |

Implementation:
- Crate `crates/kiln-setup` (bin + lib), reuses `kiln-hwdb` quirk TOML in
  `/usr/share/kiln-hwdb/devices.d/*.toml`; each step is a trait `SetupStep { probe, plan, apply, verify }` so unit tests run on JSON fixtures of `lspci -nn`, `lsusb`, DMI dumps from real machines.
- Unit: `kiln-setup.service` (Type=oneshot, `ConditionPathExists=!/var/lib/kiln/setup.done`, `After=network-online.target`, `Before=display-manager.service`) runs the probe and non-interactive actions headless; GUI summary page launched by `kiln-welcome --first-run` reads the report. Offline works: payload on ISO first, apt only if online.
- Re-runnable: `kiln-ctl setup --rerun` and a "Check my hardware again" button in Reports.
- Safety: every step is idempotent, snapshotted (one pre snapshot "kiln-setup"), and a failing step never blocks login; failures appear in Reports with a copyable log.
- Package: `kiln-setup` in repo `main`; Calamares `kilnsetup` job writes the "run on first boot" flag and copies the payload.

Risk: wrong driver on exotic hardware; mitigated by table-driven allow-list plus the
drivers.md auto-fallback guard. IPU6 webcam support is unstable (verify). Acceptance:
fixture tests for 10 machine classes; VM boot test shows `setup.done` and a report.

## 3. F2 Update Manager delta: snapshot gate and known-bad feed

**What.** Extra safety on top of tool-suite 5 and fresh-software 10, no atomic OS needed.
(a) `kiln-snapshot` apt hook decides snapshot scope by transaction risk and refuses to
proceed on a low-space volume; (b) a signed "known-bad" feed that holds back specific
package versions for everyone (the Mint "level 4/5" trick, but server-driven); (c)
rollback UX for the common failure "it will not boot to desktop".

| Item | Design |
|---|---|
| Snapshot gate | Rust port of the shell `kiln-snapshot`; reads `apt-get -s` plan; snapshot always if kernel, firmware, mesa, nvidia, systemd, libc6, glaze, or more than 40 packages change. Pre/post pair labelled with the apt command line. Config `/etc/kiln/snapshot.toml` (`min_free_gb = 5`, `keep_pre_post = 10`, `keep_timeline = 5`). |
| Space guard | If free space on btrfs < `min_free_gb`, call F4 cleanup first, then retry; if still low, block the risky upgrade and tell the user which category to clear. |
| Known-bad feed | `https://repo.mandrusia.tech/kiln/holds.json` (signed with the repo key, detached `.sig`). Example entry: `{"package":"linux-image-amd64","version":"6.17.2-1","reason":"nvidia 580 oops","until":"6.17.3"}`. Update Manager fetches via `apt update` hook `Acquire::PDiffs`-like cron; applies `apt-mark hold` (marker `kiln-hold-` in `/var/lib/kiln/holds.d/`) and shows a "held by Kiln, reason" row. Released automatically when `until` is installable. |
| Boot-failure rescue | grub-btrfs entry "Kiln snapshots" (already planned); add `kiln-boot-counter.service` (systemd `boot-complete.target` + `systemd-boot-check-no-failures` style): after an upgrade snapshot, if three boots do not reach `graphical.target` + `glaze-session` healthy flag, `grub-reboot` next boot into the pre-upgrade snapshot and show "Kiln restored your last working state" on login. |
| Rollback UX | Update Manager > "History": list with date, command, packages changed, size; buttons "Undo this update" (rollback + reboot) and "Keep this" (delete pair). Plain language only; no "snapper" in UI. |
| Non-btrfs | rsync/Timeshift engine already planned; boot-counter rescue not available there, so show a banner "Rollback needs btrfs". |

Repo bits: `holds.json` generated by CI nightly job `.github/workflows/holds.yml` (manual
edit through a PR to a `holds/` dir, human reviewed); published beside aptly output on R2.
Effort M (feed S, gate S, boot counter M). Risk M: wrong hold blocks security fixes;
mitigations: `holds` may never hold a package whose version appears in a Debian
security advisory (CI check against `security-tracker` JSON), max hold 14 days, one-click
"install anyway". Pri P0 because `trixie-fresh` is unsafe without it.

## 4. F3 Welcome and Tour delta

Tool-suite 7 is a one-time wizard. Delta: a permanent **Tour** (`org.kiln.Welcome --tour`)
that teaches by showing the real UI, plus rotating tips.

| Piece | Detail |
|---|---|
| Tour overlay | Glaze shell layer-shell overlay dims screen and highlights Start, taskbar, tray, search; 5 stops, skippable, keyboard operable. Implemented as a `glaze-shell` mode driven by `/usr/share/kiln-welcome/tour.toml` (stop id, widget accessible name, text per locale). Uses `glaze-a11y` roles to find widgets, no pixel coordinates. |
| Where-is-X search | Start menu search recognises Windows names ("Control Panel", "Task Manager", "Device Manager", "Registry") and offers the Kiln tool; data `data/windows-names.toml`, shared with the cheat sheet in windows-switcher 14. |
| Tips | One tip per day in Welcome and the lock screen (off by default on lock screen); 60 tips, translatable; file `tips.toml`. |
| Phone | Tour has 3 gesture stops (swipe up, notification shade, dock hint). |
| Reopen | Start menu "Take the tour", Settings > About. |

Effort S-M. Risk L. Pri P1.

## 5. F4 kiln-care: self-heal and hygiene

**What.** A small root daemon + user tray notifier that detects and fixes the common ways
a Debian desktop gets stuck, with plain-language explanations. Linux users on Mint ask
forums; Kiln fixes it for them. "Care" is a tab in Reports, also `kiln-ctl care`.

| Problem | Detection | Fix | Auto or ask |
|---|---|---|---|
| Broken dpkg (interrupted) | `dpkg --audit`, `/var/lib/dpkg/updates/` non-empty, lock file stale (no holder in `fuser`) | `dpkg --configure -a`, `apt-get -f install` | auto if no apt process running; ask on removals |
| Held packages hiding updates | `apt-mark showhold` minus F2 holds | show list with reason; "Release hold" | ask |
| Half-installed driver / DKMS fail | `dkms status` error, missing `nvidia` module for running kernel | reinstall prebuilt module package for ABI or boot previous kernel; link to F2 rollback | ask |
| Missing third-party key / sources broken | `apt update` error classes (`NO_PUBKEY`, 404 on mirror) | Sources tool repair, switch to `deb.debian.org` | ask |
| Full disk (root < 2 GB or < 5 %) | `statvfs` every 10 min via timer | cleanup order below | notify at 10 %, auto-safe-clean at 5 % |
| Journal bloat | `journalctl --disk-usage` > 500 MB | `journalctl --vacuum-size=200M`; ship drop-in `SystemMaxUse=200M` | auto |
| Old kernels | more than 2 kernels installed besides running | `apt-get autoremove --purge` for `linux-image` excluding running and newest | ask once, then auto |
| Snapshot bloat | btrfs `qgroup` or `snapper` space | prune timeline per `snapshot.toml` | auto |
| Flatpak unused runtimes | `flatpak uninstall --unused` dry run | run | auto weekly |
| Core dumps / apt cache | `/var/lib/systemd/coredump`, `/var/cache/apt/archives` > 1 GB | `coredumpctl` vacuum, `apt-get clean` | auto |
| Failed systemd units | `systemctl --failed` | show; offer restart or "Report" | ask |
| Clock/time wrong | `timedatectl` NTP unsynced | enable `systemd-timesyncd` | auto |
| Locale / keyboard missing | `locale -a` lacks configured locale | `locale-gen` | auto |

Disk-full cleanup order (safe to risky, stop when 3 GB free): `apt clean`, journal
vacuum, coredumps, `flatpak --unused`, old kernels, snapshot pruning (never the newest
pre/post pair), thumbnail cache, then show a Files "what uses space" view (`kiln-du`
wrapper over `dua`-style scan; implement in Rust with `walkdir`).

Implementation:
- Crate `crates/kiln-care`: `checks/*.rs` each `Check { id, detect() -> Finding, fix() -> Plan }`; same trait/fixture approach as F1; findings serialised to `/run/kiln/care.json`.
- Units: `kiln-care.timer` (`OnCalendar=hourly`, `RandomizedDelaySec=10m`, `Persistent=true`), `kiln-care.service` (Type=oneshot, `Nice=19`, `IOSchedulingClass=idle`, `ProtectSystem=strict`, `ReadWritePaths` for dpkg/apt/journal), `kiln-care-boot.service` (runs dpkg audit before `display-manager`).
- Fixes run through `kiln-helperd` method `CareFix(id)`; each has its own polkit action `org.kiln.care.fix-<id>`.
- Tray: `kiln-care-notify` (user unit) shows a Glaze toast with one button.
- Package `kiln-care` (`main`); depends `kiln-helperd`.

Risk M (a bug here damages systems), mitigated by dry-run mode, snapshot before any
package-state fix, and a hard rule: no `rm -rf` outside an allow-list of cache dirs. Pri
P0 (full disk and dpkg repair are the top Mint-forum issues); effort M.

## 6. F5 Kiln Play: gaming profile and session

Windows-switcher 12 lists packages. Delta: a managed **profile**, a driver profile, and
a console-like session.

| Piece | Detail |
|---|---|
| Profile meta | `kiln-profile-gaming` depends `steam-installer`, `gamemode`, `mangohud`, `gamescope`, `vulkan-tools`, `libvulkan1:i386`, `mesa-vulkan-drivers:i386`, `steam-devices`, `protontricks`(verify in trixie), `lutris`; Recommends `goverlay`. i386 architecture enabled by Kiln action, not by default. |
| Driver profile | Driver Manager profile "gaming" selects newest Nvidia branch (595 per KILN-PLAN, verify), `nvidia-drm.modeset=1`, `nvidia-drm.fbdev=1`, `nvidia-suspend/hibernate/resume` units, `__GL_SHADER_DISK_CACHE_SKIP_CLEANUP=1`; AMD: `mesa` from `trixie-fresh`, `amdgpu.ppfeaturemask` only on user opt-in; Intel Arc: `xe` driver check (verify). |
| Proton management | Flatpak `net.davidotek.pupgui2` (ProtonUp-Qt) preinstalled by profile; Kiln writes `~/.local/share/Steam/compatibilitytools.d` via it; "Compatibility" page lists ProtonDB rating link per title (offline link only, no network calls unless user opens). |
| Presets | `/usr/share/kiln-play/presets/*.toml`: `performance` (MangoHud FPS+frametime, GameMode, `mesa_glthread=true`, `PROTON_ENABLE_NVAPI=1` for Nvidia, `PROTON_ENABLE_WAYLAND=1` experimental off), `quiet` (FPS cap, power-saver profile), `battery`. Applied as Steam launch option templates `kiln-play-run <preset> %command%`. |
| `kiln-play-run` | Rust wrapper: sets env, calls `gamemoderun`, `mangohud`, optionally `gamescope -W -H -r -f --`, switches `power-profiles-daemon` to `performance`, inhibits idle via `systemd-inhibit`, restores on exit. |
| Gaming mode session | Optional login entry "Kiln Game Mode": `gamescope --steam -- steam -steamdeck -steamos3` (verify flags) from `/usr/share/wayland-sessions/kiln-gamemode.desktop`; used when docked with controller (convergence trigger "gamepad attached" is a hint, never automatic). |
| Controllers | `steam-devices` udev, `xpadneo`/`xone` DKMS opt-in (prebuilt where possible), Controller Test page in Settings (SDL2 `gamepad` API via `sdl2` crate or `evtest` wrap). |
| Anti-cheat note | Plain text in Store gaming page: kernel anti-cheat (EAC/BattlEye per-title opt-in) is per game; link ProtonDB and AreWeAntiCheatYet. No promises. |
| Notifications | Do-not-disturb auto while fullscreen (Glaze notification rule). |

Repo bits: `kiln-play` and meta in `main`; gamescope and mangohud rebuilt in `trixie-fresh` if Debian versions are old (check). Effort M. Risk M: Nvidia + Smithay compositor + gamescope nested Wayland interplay (verify), 32-bit multiarch breakage with `fresh` Mesa (must rebuild i386 too; CI gate). Pri P1.

## 7. F6 Profiles: creator, dev, office, school

**What.** One-click meta-packages plus config, offered in Welcome, Software "Collections"
and `kiln-ctl profile install <name>`. Windows switchers want "a video editing setup",
not 14 package names.

| Profile | Packages (apt unless Flatpak noted) | Config / extras |
|---|---|---|
| creator-video | `obs-studio`, `kdenlive`, `ffmpeg`, `handbrake`, `audacity`, `mkvtoolnix-gui`; Flatpak DaVinci Resolve is not packaged (user installs vendor .run; Kiln gives a helper "Install Resolve" doc page) | pipewire low-latency drop-in (`/etc/pipewire/pipewire.conf.d/10-kiln-lowlat.conf`, quantum 256) |
| creator-photo | `darktable`, `rawtherapee`, `gimp`, `digikam`, `inkscape` | color management: `colord`, `displaycal`-equivalent via `argyll` (verify) |
| creator-audio | `ardour`, `lmms`, `qjackctl`, `carla`, `rtkit`, `pipewire-jack` | realtime limits `/etc/security/limits.d/95-kiln-audio.conf`, user in group `audio` |
| creator-3d | `blender`, `freecad`, `openscad` | Blender Flatpak optional for newer; GPU compute: CUDA/OptiX via Nvidia profile, ROCm separate (not shipped, doc only) |
| dev-core | `build-essential`, `git`, `gh`, `ripgrep`, `fd-find`, `jq`, `tmux`, `neovim`, `direnv`, `shellcheck`, `rustup`, `nodejs`+`npm`, `python3-venv`, `pipx` | `/etc/profile.d/kiln-dev.sh` adds `~/.local/bin`, `~/.cargo/bin`; `rustup` installs stable on first run |
| dev-containers | `podman`, `buildah`, `skopeo`, `distrobox`, `docker.io` or Docker CE (user choice) | rootless podman: `/etc/subuid`/`subgid` entries by `kiln-setup`, `loginctl enable-linger` off by default |
| dev-vm | `qemu-system-x86`, `libvirt-daemon-system`, `virt-manager`, `virtiofsd`, `swtpm` | add user to `libvirt`, `kvm`; Windows 11 VM helper wizard ("Run Windows in a window") is in windows-switcher scope? not planned, add as stretch |
| dev-vscode | VS Code via Microsoft repo (not redistributed; user-confirmed "add Microsoft repo" dialog), `codium` Flatpak alternative | GPG key shown in Sources tool |
| dev-wsl-like | `kiln-shell` Distrobox presets: Debian, Ubuntu, Fedora, Arch "terminals" from Terminal profile menu | replaces WSL mental model |
| office | LibreOffice (fresh), `onlyoffice` Flatpak, `thunderbird`, MS fonts mapping (planned in windows-switcher 6/8) | default save as `.docx` preference in LO config |
| school | `gcompris-qt`, `geogebra` Flatpak, `kolourpaint`, `scratch` Flatpak, parental controls via `malcontent` (verify) | age profile Settings page |
| server-lite | `openssh-server`, `cockpit`, `podman`, `unattended-upgrades` | headless-friendly, no Glaze |

Implementation:
- Meta packages `kiln-profile-<name>` (arch `all` where possible, `Depends` for core, `Recommends` for extras) built from `packaging/profiles/<name>/debian/`; list lives as TOML `data/profiles/<name>.toml` (id, name, icon, description translated, apt list, flatpak list, post-install script id) so Software, Welcome and `kiln-ctl` render from one source.
- `kiln-profile-apply` post-step (Rust, in `kiln-ctl`) handles group membership, dotfiles, `rustup` first run; every step has an undo list recorded in `/var/lib/kiln/profiles/<name>.json` so "Remove profile" is clean.
- Install runs through helperd with one snapshot and progress UI from Software.

Effort M (L if each profile gets screenshots, tests). Risk L (package names verify per
trixie; use `apt-cache policy` CI check `scripts/check-profile-pkgs.sh` against trixie
and `trixie-fresh`). Pri P1 for dev-core, creator-*, gaming; P2 rest.

## 8. F7 Laptop battery and thermal tuning

Decision: **`power-profiles-daemon` (PPD) stays the one authority; no TLP.**
Rationale: PPD is already in the image, integrates with Glaze Settings and upower, is
the DBus API apps use (`net.hadess.PowerProfiles`), and conflicts with TLP/`auto-cpufreq`
(both fight over governors). Use `tuned-ppd`? Not in trixie (verify); skip. Instead Kiln
adds a thin, deterministic layer.

| Layer | Package / config | Behaviour |
|---|---|---|
| Profiles | PPD 0.2x (verify trixie version) | Power saver / Balanced / Performance; Glaze tray and Settings; auto "power-saver at 20 %" via upower rule in `glaze-power` crate (existing crate: extend, do not duplicate; new logic in `kiln-power`) |
| Platform tuning | `kiln-power-data` udev rules + `systemd` tmpfiles | PCIe ASPM `powersave` on battery only (`ACTION=="change", SUBSYSTEM=="power_supply"`), USB autosuspend allow-list, SATA `med_power_with_dipm`, audio codec `power_save=1` (`/etc/modprobe.d/kiln-audio.conf`), Wi-Fi `power_save` via NetworkManager `wifi.powersave=3` for Intel only (table-driven) |
| Thermals | `thermald` on Intel (Recommends, only enabled when DMI vendor matches known-good list), `lm-sensors`, `fancontrol` not default | Glaze "Fan and temperature" row in Reports; no fan control UI at launch |
| Charge limit | `kiln-power charge-limit 80` writes `/sys/class/power_supply/BAT0/charge_control_end_threshold` (ThinkPad, ASUS, Framework, Dell via `cros_ec`/`dell-smm` where present); persisted by `kiln-charge-limit.service` | Welcome toggle "Protect battery (80 %)". Windows OEM apps have this; Mint lacks a GUI. |
| Lid/suspend | `logind.conf.d/10-kiln.conf`: lid close suspend on battery, ignore when docked (`HandleLidSwitchDocked=ignore`); `suspend-then-hibernate` after 30 min when swap is large enough (F17) | |
| Nvidia hybrid | `nvidia-powerd` for dynamic boost, `NVreg_DynamicPowerManagement=0x02` on Turing+ (written by udev helper, per drivers.md 3.4) | `switcherooctl` "Run with discrete GPU" in Files/Start menu context |
| Measure | `kiln-power report` runs `powertop --csv` 20 s and `upower` stats, shows top 5 wakeups in Reports | no `powertop --auto-tune` (it breaks USB input); we apply our own rules |

Why not TLP: TLP gives more knobs but needs its own UI and conflicts with PPD; Windows
users want three choices and a battery-health toggle. TLP stays installable from Software
(`tlp` conflicts with `power-profiles-daemon` in `Conflicts`, document it).

Files: crate `crates/kiln-power`; `packaging/kiln-power-data/` udev + modprobe + tmpfiles. Effort M; risk M (device quirks cause missed suspend; data table with per-DMI allow-list and opt-out `kiln.power=default` kernel param). Pri P1.

## 9. F8 Unified apt + Flatpak store delta

`tool-suite.md` 11 plans the Software app and the preference rule. Delta only:

| Piece | Detail |
|---|---|
| One transaction queue | `kiln-software` queues apt, Flatpak and profile installs; helperd serialises apt (`dpkg` lock) while Flatpak runs in parallel; single progress list; failure of one item does not abort others. |
| Origin chips and "why" | each result shows Debian, Kiln, Flathub or Mozilla chip; tooltip explains the difference in one sentence (integration vs. sandbox vs. freshness) from `data/origin-help.toml`. |
| Update parity | apps installed from Flathub use Update Manager Flatpak group (planned); delta: "move this app to the other source" (`kiln-ctl switch-source <app>`) migrates settings from `~/.var/app/` where possible. |
| Dedupe | merge appstream IDs across sources (`org.mozilla.firefox`); prefer curated mapping file, user override remembered. |
| Safety | Flatpak permission summary (filesystem=host warning) in install dialog; flatpak overrides editor via Flatseal bundled as "Permissions" button. |
| Windows apps | "Can this run on Kiln?" entry: for `.exe` search, queries `data/windows-alternatives.toml` (Photoshop -> GIMP/Krita, etc.), then Bottles/Wine path from windows-switcher 7. |

Effort M. Risk M (Flathub quality varies; appstream parse perf). Pri P2.

## 10. F9 Backups delta: restic engine

`tool-suite.md` 10 plans snapshots plus a Files tab and names restic. Delta: concrete
engine, targets, verification.

| Piece | Detail |
|---|---|
| Engine | `restic` (trixie package; verify version, rebuild in `trixie-fresh` if < 0.17), wrapped by `kiln-backup-engine` crate (Rust, spawns `restic --json`, parses status lines). No custom format. |
| Repo location | external drive (auto-detect by label via udisks, mounted by `udisks2`), SMB/NFS via gvfs path, SFTP, and `rclone` backends for cloud (S3, B2, Google Drive, OneDrive); credentials stored in `libsecret`, never in config. |
| Schedule | user units `kiln-backup.service` / `.timer` (`OnCalendar=daily`, `Persistent=true`, `ConditionACPower=true`, `After=network-online.target`), `systemd-inhibit` for suspend, `IOSchedulingClass=idle`, `CPUQuota=30%`. |
| Password | generated recovery key shown once, printable PDF, stored encrypted by TPM/`libsecret`; if lost, backups are unrecoverable and the UI says so before first run. |
| Defaults | `~/Documents`, `~/Pictures`, `~/Desktop`, `~/Music`, `~/Videos`, `~/.config` selected subsets, excludes `.cache`, `node_modules`, `target`, `.var/app/*/cache`, VM images (> 4 GB with checkbox). |
| Restore | browse by date in Kiln Files via `restic mount` (FUSE) read-only; "Restore previous version" in Files context menu; drag out. |
| Verification | weekly `restic check --read-data-subset=5%`; monthly restore drill of 5 random files into a temp dir with checksum compare; report in Reports. |
| Retention | `keep-daily 7 keep-weekly 4 keep-monthly 12`; configurable simple slider. |

Effort M. Risk M (restic repo password loss; prune cost on cloud). Pri P2.

## 11. F10 Security defaults

Debian ships no firewall and AppArmor is enabled in trixie's default kernel (verify
`apparmor` and `apparmor-profiles` install by default, not guaranteed on live-build). Kiln
turns on a sane baseline without breaking home networking.

| Control | Choice | Config |
|---|---|---|
| Mandatory access control | AppArmor on; `apparmor`, `apparmor-profiles`, `apparmor-profiles-extra`, `apparmor-utils`; kernel cmdline `apparmor=1 security=apparmor` from `/etc/default/grub.d/20-kiln-security.cfg`; Flatpak and Firefox Mozilla build run unconfined by profile except snap-style unprivileged userns profile for Firefox (verify `unprivileged_userns` handling in kernel 6.12+; Debian does not restrict by default) | `kiln-security-defaults` package |
| Firewall | `nftables` with a deny-in default ruleset, **no `ufw`/`firewalld`** (smaller, no Python); rules in `/etc/nftables.d/10-kiln.nft`: accept `lo`, established, ICMPv6 NDP, DHCP, mDNS (5353) and KDE-Connect-like `kiln-link` ports on local subnet only; drop else; `kiln-fw` thin GUI toggle ("Public network mode" blocks mDNS too) via NetworkManager connection zone dispatcher in `/etc/NetworkManager/dispatcher.d/90-kiln-zone` | `nftables.service` enabled; GUI: Settings > Network > Firewall (`kiln-fw` crate wraps `nft -j`) |
| Updates | `unattended-upgrades` for **Level 1 only** (Debian-Security + Kiln `kiln-*`): `/etc/apt/apt.conf.d/52kiln-unattended` with `Origins-Pattern` for `origin=Debian,codename=trixie-security` and `origin=Kiln,archive=trixie-stable,component=main`, `Automatic-Reboot "false"`, run via systemd timer, Update Manager shows result; fresh channel never auto-applied | |
| Secure Boot | keep Debian shim chain (KILN-PLAN 6). MOK flow for DKMS modules owned by Driver Manager (drivers 3.2). Delta here: **Secure Boot status page** (`mokutil --sb-state`, enrolled keys list, "Why am I asked for a password at boot?" explainer with screenshots of the blue MOK screen), TPM-measured boot optional, and a `kiln-mok` helper that generates the key pair in `/var/lib/shim-signed/mok` on demand, never during live. | |
| Disk encryption | Calamares LUKS2 option ("Encrypt my disk"), `cryptsetup`, TPM2 auto-unlock `systemd-cryptenroll` as an opt-in in Settings (PCR 7, with passphrase fallback); btrfs on LUKS | |
| Screen lock / sudo | `logind` idle lock 5 min, `pam_faillock`, `sudo` timestamp default; first user is in `sudo` (planned) | |
| Browser | Mozilla Firefox with uBlock Origin preinstalled via enterprise policy `policies.json` (user-removable); Safe Browsing on | `/usr/lib/firefox/distribution/policies.json` |
| Ports/services | `avahi-daemon` only when printer/AirPlay discovery wanted; `cups-browsed` off (already); `ssh` server not enabled by default in installed system (live image has `openssh-server` today: remove from installed image, enable on demand) | |
| Kernel hardening | sysctl drop-in `/etc/sysctl.d/60-kiln-hardening.conf`: `kernel.kptr_restrict=2`, `kernel.dmesg_restrict=1`, `net.ipv4.tcp_syncookies=1`, `kernel.yama.ptrace_scope=1`, `fs.protected_*`; `kernel.unprivileged_userns_clone` left on (Flatpak, Chrome need it) | |
| Audit | Reports > Security tab: firewall, AppArmor, SB, encryption, updates age, open ports (`ss -tulpn`) as a "Security checkup", green/amber/red, copy to clipboard | |

Effort M. Risk M: AppArmor/userns denials break apps (collect `aa-logprof` in a nightly
VM test over the profile app set); nftables default can break local discovery
(mitigated by allow-list and public/home mode). Pri P0 for firewall+AppArmor+unattended
security updates; P1 rest.

## 12. F11 Accessibility

Kiln targets Windows switchers, including those relying on Narrator/Magnifier/High Contrast.

| Need | Windows equivalent | Kiln implementation |
|---|---|---|
| Screen reader | Narrator | `orca`, `at-spi2-core`, `speech-dispatcher`, `espeak-ng`, `piper` voices (Flatpak or Kiln packaged, licence check); keybinding Win+Ctrl+Enter toggles Orca; Welcome offers "Turn on screen reader" and Calamares has a boot menu entry "Kiln Live (screen reader)" with `kiln.a11y=orca` that starts Orca and speaks the menu. Boot beep/speech at GRUB with `accessibility` is limited (verify). |
| Magnifier | Magnifier | Glaze compositor zoom (`glaze-wm`: Win+`+`/`-`, follow focus/cursor/caret); add as compositor feature, not a separate app; effort M inside Smithay. |
| High contrast / large text | Contrast themes | Glaze theme variants `Kiln High Contrast` (light and dark), WCAG AA at least 4.5:1 checked by `glaze-a11y audit` in CI (crate exists with `audit.rs`); text scale 100-300 % via `Settings > Accessibility`. |
| Sticky/Slow/Bounce keys | Filter Keys | compositor keyboard accessibility (`xkb` `AccessX` is X11-only; implement in `glaze-wm` input layer); verify libinput/Smithay hooks. |
| On-screen keyboard | OSK | squeekboard (phone, planned), `onboard` is X11; for desktop use squeekboard in desktop mode (verify). |
| Captions/visual alerts | | visual bell: screen flash option in compositor; subtitle support in `mpv`/`vlc` defaults (P2). |
| Testing | | AT-SPI tree smoke test in CI: `glaze-a11y` checks every first-party app exposes labelled controls; keyboard-only run of Welcome and Update Manager (script with `ydotool`/`dogtail` in QEMU). |

Package `kiln-a11y-defaults` (Orca autostart rules, dconf/gsettings overrides, theme
variants). Effort M (magnifier L if built from scratch). Risk L. Pri P1 for screen reader
at install and live, high contrast, text scale; P2 magnifier and filter keys. Put it in
the P0 list for the ISO smoke test only as "boots with `kiln.a11y=orca`".

## 13. F12 i18n and l10n

| Item | Plan |
|---|---|
| Source of truth | Fluent `.ftl` for Rust apps (`fluent-templates` via `glaze-i18n` crate; exists), gettext `.po` for scripts and Calamares. One Weblate project (self-hosted or hosted.weblate.org libre plan, verify) syncing to a `l10n` branch. |
| Launch set | en, es, ca, fr, de, pt-BR, it, nl, pl, ru, tr, ja, zh-CN, zh-TW, ko, ar (RTL), he (RTL), hi. Ship en/es/ca complete at launch; others as community with 90 % string gate before listed in installer. |
| Packaging | `kiln-l10n-<lang>` language packs pulled by `language-selector`-like step in Calamares and Welcome; also `hunspell-<lang>`, `libreoffice-l10n-<lang>`, `firefox` language pack (Mozilla repo `firefox-l10n-<lang>`), `fonts-noto-*` per script. Installer chooses by locale, Welcome page 1 adds more. |
| RTL | Glaze and TilesDE audited for mirroring (GTK4 handles; layer-shell anchors need mirror logic); screenshot tests in `ar` locale. |
| Input methods | `fcitx5` default for CJK, `ibus` for others; Wayland `text-input-v3` path verify; setup in kiln-setup by locale. |
| Pseudo-loc CI | `scripts/pseudoloc.sh` builds apps with `[!! ... !!]` strings and checks no truncation / hardcoded English via screenshot diff in QEMU smoke (F14). |
| Keyboard layouts | Calamares keyboard step plus the Windows layout names ("US International", "Spanish (Spain)"), mapping table `data/win-keyboard-names.toml`. |
| Search-in-your-language | Where-is-X catalog translated (F3). |

Effort M (infrastructure) + ongoing. Risk L. Pri P1 (es, ca, en first since owner and early users).

## 14. F13 Legal and branding checklist delta

`KILN-PLAN.md` 6 covers Debian marks. Delta: checks that plan does not list. Not legal
advice; owner should have a lawyer review before 1.0.

| # | Item | Action | Gate |
|---|---|---|---|
| L1 | Debian trademark | Read debian.org/trademark policy; never "Debian Kiln", use "Kiln GNU/Linux, based on Debian"; no Debian swirl; "Debian" word only factual. Add a `docs/legal/TRADEMARKS.md` with approved phrasing. | CI grep exists; add also website and ISO volume labels. |
| L2 | "Kiln" name | Search USPTO/EUIPO/WIPO for "Kiln" in class 9 (software) and 42; existing products named Kiln (e.g. Fog Creek's Kiln, kiln.dev) (verify). If conflict, choose "Kiln OS" or alternative before public launch. | before first public ISO |
| L3 | os-release correctness | `ID=kiln`, `ID_LIKE=debian`, `VERSION_ID`, `VARIANT_ID` (desktop, handheld), `HOME_URL`, `SUPPORT_URL`, `BUG_REPORT_URL`, `CPE_NAME` unset, `LOGO=kiln-logo`; `PRETTY_NAME="Kiln GNU/Linux 1 (ember)"`; also `/etc/lsb-release` for `lsb_release` (`lsb-release` package provides from os-release; verify trixie). | `scripts/check-os-release.sh` in CI |
| L4 | Codec patents | H.264/AAC: patents expire in many regions; HEVC, AV1 licensing remains murky. Position: Kiln is distributed from a jurisdiction chosen by owner; ship Debian's `libavcodec`/`ffmpeg` (includes patent-encumbered decoders already in Debian main), Firefox bundles OpenH264 downloaded from Cisco (that covers H.264 for Firefox under Cisco licence); do not ship `libde265` HEVC encoder, `x265`, `libfdk-aac` (non-free, license incompatible with GPL ffmpeg builds); Provide "Install extra codecs" only if the user is in a jurisdiction where permitted (checkbox text). Seek counsel on HEVC/AC-4 decoding bundling. | listed in `docs/legal/CODECS.md` |
| L5 | Non-free firmware/drivers | `non-free-firmware` shipped by Debian on installer since 12; Nvidia proprietary redistribution licence read (KILN-PLAN open item 4); put `NOTICE` file and EULA page in Driver Manager before install (planned partly in drivers 13). | |
| L6 | Fonts | Inter (OFL), Noto (OFL), Liberation (OFL); no Microsoft fonts (`ttf-mscorefonts-installer` downloads, user-run, not shipped); Carlito/Caladea (OFL) metric compatible (planned). | |
| L7 | Third-party names | Steam, Windows, Mint, Ubuntu, Microsoft, Office: never in package names or product copy except factual comparison ("opens .docx"); Welcome cheat sheet use "Windows" factually. | copy review |
| L8 | Licence inventory | Generate SBOM per ISO: `syft` on the rootfs -> `kiln-<ver>.spdx.json`; `reuse lint` for Kiln repos; ship `THIRD-PARTY.md`. Glaze is MIT/Apache; check Smithay (MIT), GTK (LGPL), Qt (LGPL/GPL for TilesDE; dynamic link). | CI job `sbom.yml` |
| L9 | Source offer | GPL source: Debian sources reachable via `deb-src` lines; Kiln rebuilds publish `.dsc` and sources in aptly (planned) and the ISO carries `/source-offer.txt` pointing to both. | |
| L10 | Privacy policy and ToS | One page: no telemetry, mirrors log IPs (R2/CDN, say so), no account. | website |
| L12 | Security contact | `security@` mailbox, `SECURITY.md`, `security.txt` on the website, CVE process: Debian-security covers base, Kiln covers own packages within 7 days. | |
| L13 | Release identity | Signed ISO (`SHA256SUMS`, `SHA256SUMS.gpg` with repo key); mirror/torrent policy. | |

Effort S (docs + scripts) plus lawyer time. Risk H if skipped (name clash, codec). Pri P0.

## 15. F14 CI and ISO pipeline

Existing: deb CI matrix and QEMU smoke are only described in `fresh-software.md` 6.
Delta: the ISO pipeline itself.

| Stage | Detail |
|---|---|
| Builder | `live-build` (Debian 13 container, `--privileged` or `fakechroot`-free runner; `ubuntu-24.04` host runs `debootstrap` in a `debian:trixie` container; verify live-build `trixie` version 20250???) with the recipe in `packaging/kilnlinux/image/` (already has `auto/config`). Entry script `scripts/build-iso.sh` (wraps `lb clean/config/build`, copies artefacts to `out/`). |
| Reproducibility | `SOURCE_DATE_EPOCH` = last commit time; `LB_APT_SECURE`; pin the apt snapshot: build uses `snapshot.debian.org` (`deb [check-valid-until=no] https://snapshot.debian.org/archive/debian/<timestamp>/ trixie main`) AND a frozen Kiln aptly snapshot published as `repo.mandrusia.tech/snapshots/<date>`; `mksquashfs -mkfs-time $SOURCE_DATE_EPOCH -all-time`, `xorriso --modification-date` through `LB_BINARY_IMAGES`; check by building twice and diffing `filesystem.squashfs` hash and `diffoscope` summary (`--reproducible` job weekly, best effort, expect small ISO-header diffs; goal: rootfs reproducible first, ISO second). Honest label: "reproducible rootfs" until proven. |
| Nightly | `cron: 0 3 * * *` workflow `iso-nightly.yml`: builds `trixie-fresh` desktop amd64 ISO, uploads to R2 `nightly/kiln-YYYYMMDD-amd64.iso` (keep 7), posts result on commit status badge. arm64 phone/desktop image weekly (slower). |
| Smoke tests | QEMU `qemu-system-x86_64 -enable-kvm`(CI has KVM on `ubuntu-24.04` runners via `/dev/kvm` verify, else TCG: slow) with OVMF, 4 GB, virtio-gpu; test driver `tests/iso/smoke.py` (Python `pexpect` or Rust `kiln-qa` bin) using QMP `screendump` and the serial console. Cases: S1 BIOS and UEFI live boot reaches Glaze (screenshot hash tolerance, OCR via `tesseract` for "Kiln"), S2 Calamares automated install (btrfs layout, `calamares --debug`, scripted via `kiln-autoinstall.conf` preseed) then reboot into installed system, S3 first-boot `kiln-setup` report exists, S4 `apt update && apt full-upgrade` in the installed VM with snapshot pair created, S5 rollback via grub-btrfs entry, S6 firewall: `nft list ruleset` non-empty, S7 branding grep (KILN-PLAN 6), S8 `kiln-care` check on a deliberately broken dpkg, S9 pseudo-locale launch of Welcome, S10 Secure Boot boot with OVMF `OVMF_CODE.secboot.fd` and Debian shim. |
| UI tests | screenshots through `grim` inside guest via QMP-over-`virtio-serial` agent `kiln-qa-agent`, compared with baseline PNGs (perceptual hash) stored in `tests/iso/golden/`; flaky cases retried once. |
| Hardware gate | manual checklist `docs/release/hardware-matrix.md` run before each release candidate on owner devices (drivers.md 15). |
| Artefacts | `kiln-<ver>-amd64.iso`, `.sha256`, `.sig`, `.spdx.json` (L8), `build.log`, `package-manifest.txt` (from `dpkg-query`); torrent `.torrent` via `mktorrent` with web seed on R2. |
| Release flow | tag `v*` -> build -> smoke -> manual approval -> sign (offline key via `gpg --detach-sign` on owner machine, CI never holds the release key; CI holds only the repo subkey) -> publish to `releases/` and update website `latest.json`. |
| Repo cadence | `trixie-fresh` promoted daily if smoke green; `trixie-stable` weekly after 3 clean nights, aligned with F2 holds feed. |

Effort L. Risk M (KVM availability, reproducibility rabbit hole; bound the time:
rootfs reproducible only, ISO bit-for-bit is stretch). Pri P0 for smoke S1-S3, S7;
P1 for nightly, SBOM, Secure Boot case; P2 diffoscope reproducibility.

## 16. F15 Telemetry stance

Decision: **none**. No phone-home, no popcon, no analytics, no crash upload.

| Rule | Implementation |
|---|---|
| No popularity-contest | `popularity-contest` package not installed; `Conflicts` not needed; CI grep denies it in package lists. |
| Network contact only on user request or apt/Flathub/NTP | Welcome states this on page "Your privacy"; `docs/PRIVACY.md`. Allowed automatic connections: apt mirrors, Kiln repo (`holds.json`, F2), `connectivity-check` is **disabled** (`/etc/NetworkManager/conf.d/20-kiln-connectivity.conf` `[connectivity] enabled=false`; Debian's NM uses Debian's URL otherwise), NTP `time.debian.org` pool (or user choice), `fwupd` LVFS metadata (F18, opt-out), Mozilla's own Firefox updates and telemetry off by enterprise policy (`DisableTelemetry: true`, `DisableFirefoxStudies: true`). |
| Crash reports | Local only (`coredumpctl`); "Report an issue" (tool-suite 9) builds a text bundle the user reads and copies/sends manually; redaction of hostnames/usernames/MAC by default; never auto-uploaded. |
| Update checks | Pull-based from static files on R2; no per-user identifiers; server logs disabled or IP truncation (document). |
| Network connectivity | Settings has "Check for updates automatically: on/off"; all timers visible in Settings > Privacy with "Why does this connect?" explainer. |
| Verifiable | CI test: boot the installed VM idle for 10 min with `tcpdump` on the host bridge; assert only DNS + allowed hosts list `tests/iso/allowed-hosts.txt`. |

Effort S. Risk L. Pri P0 (cheap and a selling point).

## 17. Smaller features (batched)

| ID | Feature | Detail | Effort | Pri |
|---|---|---|---|---|
| F16 | `kiln-link` phone<->desktop continuity | Clipboard, notifications, file drop, "Continue on desktop" using GSConnect-like protocol: ship KDE Connect (`kdeconnect`, `kdeconnectd` with `valent` GTK alternative, verify trixie) first; own protocol later. Firewall opens 1714-1764 on LAN only (F10). Convergence tie: TilesDE notification mirror. | M | P2 |
| F17 | `kiln-tuning` memory/storage | `systemd-zram-generator` (zram = 50 % RAM, zstd, `/etc/systemd/zram-generator.conf.d/10-kiln.conf`), `earlyoom` (`-m 5 -s 5 --avoid '(glaze-compositor|Xwayland)'`) or `systemd-oomd` (choose earlyoom: simpler, per-process), `vm.swappiness=100` with zram, `fstrim.timer` enabled (btrfs `discard=async` mount), `btrfs-scrub` monthly timer with ionice, `scheduler=none` for NVMe udev, `fs.inotify.max_user_watches=524288`, swapfile for hibernation only when user opts in (btrfs swapfile with `btrfs filesystem mkswapfile`). | S | P1 |
| F18 | Firmware updates | `fwupd` + `fwupd-refresh.timer`, Update Manager group "Device firmware" (via `fwupdmgr get-updates --json`), always "Careful" level with AC-power requirement and snapshot-free (firmware is not rollbackable; warn). Secure Boot dbx updates via fwupd. | S | P1 |
| F22 | Easy Windows dual boot | Calamares "Install alongside Windows" (shrink NTFS via `ntfsresize`), `os-prober` enabled in GRUB for Windows entry, BitLocker warning screen before resize (users must back up recovery key). | M | P1 |

## 18. Repo packaging rules (repo.mandrusia.tech)

New packages (`kiln-setup`, `kiln-care`, `kiln-power`, `kiln-play`, `kiln-profile-*`, `kiln-l10n-*`, `*-defaults`, `kiln-power-data`) go to `trixie-stable/main`; rebuilds of stale `gamescope`, `mangohud`, `restic` go to `trixie-fresh` as `~kiln13u1`. Config-only packages are arch all with conffiles for every drop-in; `deb-systemd-helper` only; `Breaks`/`Replaces` explicit with kiln-base; `lintian --fail-on error,warning`.

## 19. Roadmap in three phases

### Phase A: next 4 weeks (foundation and "can I ship a safe ISO")

| Week | Deliverable | Features |
|---|---|---|
| 1 | Legal pass and name clearance (L1, L2, L3), `docs/legal/*`; telemetry policy + `kiln-base` file; remove `openssh-server` from the installed image | F13, F15 |
| 1-2 | `kiln-security-defaults` (AppArmor on, nftables ruleset, unattended-upgrades Level 1, sysctl); `kiln-tuning` (zram, earlyoom, fstrim); `fwupd` | F10, F17, F18 |
| 2 | `scripts/build-iso.sh` + `iso-nightly.yml` + QEMU smoke S1, S2, S3, S7 on self-hosted KVM runner; SBOM job | F14, F13-L8 |
| 2-3 | `kiln-care` v0: dpkg repair, disk-full cleanup, journal limits, held-package list; timer and units; fixture tests | F4 |
| 3 | `kiln-snapshot` Rust port and `snapshot.toml`; space guard; History list in CLI (`kiln-ctl history`) | F2 |
| 3-4 | `kiln-setup` v0: GPU + microcode + firmware + codecs steps (reusing `kiln-hwdb` skeleton), report JSON; no GUI yet (Welcome shows a summary page) | F1 |
| 4 | Release candidate 0 internal ISO: boot, install, `kiln-setup`, upgrade with snapshot, rollback on QEMU; gate review | all |

Exit criteria: nightly ISO builds, installs unattended in QEMU, firewall and AppArmor
active, snapshot pair created on upgrade, rollback works, no telemetry traffic.

### Phase B: next 3 months (friendly and polished)

| Month | Deliverable | Features |
|---|---|---|
| 1 | Update Manager GUI wired to snapshot gate, History and holds feed; boot-counter auto rescue; Kiln Repair boot entry | F2, F20 |
| 1-2 | `kiln-setup` full (printers, Bluetooth, fingerprint, webcam), GUI summary in Welcome; Tour overlay + tips; Where-is-X search | F1, F3 |
| 2 | Profiles: dev-core, dev-containers, creator-video/photo/audio, office as meta packages; `kiln-ctl profile`; Software "Collections" | F6 |
| 2 | Gaming: `kiln-profile-gaming`, `kiln-play-run` presets, Driver Manager "gaming" profile, controller page; Game Mode session behind flag | F5 |
| 2-3 | Laptop power: `kiln-power` crate, charge limit, udev tuning data, Reports power tab; test on 3 laptop classes | F7 |
| 3 | Accessibility: Orca at install + live entry, high contrast themes, text scale, AT-SPI CI checks | F11 |
| 3 | i18n pipeline live: Weblate, es/ca/en complete, RTL smoke, pseudo-loc CI; Secure Boot CI case (S10) | F12, F14 |
| 3 | Beta ISO public (invite): `trixie-fresh` + holds feed in production; privacy page and security.txt live | F13, F15 |

### Phase C: 6 months (depth and ecosystem)

| Month | Deliverable | Features |
|---|---|---|
| 4 | Backup: restic engine, drive and SFTP targets, restore browser, verification job, recovery key PDF | F9 |
| 4 | Unified store queue, source switch, Flatpak permissions summary, Windows-alternatives search | F8 |
| 4-5 | Install alongside Windows (os-prober, ntfsresize, BitLocker warning) and Windows-11-VM helper in dev-vm | F22, F6 |
| 5 | Compositor magnifier and filter keys; remaining profiles (3d, school, server-lite) | F11, F6 |
| 5 | `kiln-link` v1 (KDE Connect/Valent), phone pairing in Welcome; tie to convergence P2+ | F16 |
| 5-6 | Rootfs reproducibility proven (weekly diffoscope), signed release flow on offline key, torrent + web seed | F14 |
| 6 | Kiln 1.0 candidate: hardware matrix run (drivers 15), legal review sign-off (lawyer), security checkup page, 1.0 ISO | F13, F10 |

## 20. Open decisions for the owner

| # | Question | Default if no answer |
|---|---|---|
| 1 | PPD only (no TLP); nftables not ufw; earlyoom not oomd | yes to all |
| 2 | Telemetry: none, or opt-in counting | none; aggregate repo hits only |
| 3 | HEVC decoding in default ISO | yes via Debian ffmpeg; counsel to confirm |
| 4 | Self-hosted KVM runner | Linux PC with KVM as `kiln-iso` runner |
| 5 | Language packs | en/es/ca in ISO, rest on demand |
