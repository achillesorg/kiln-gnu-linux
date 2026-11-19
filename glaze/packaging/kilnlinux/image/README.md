# Kiln GNU/Linux ISO (live-build recipe)

Status: **recipe written, not yet built or booted.** Do not run until the lead says so. Run on the arm64 dev VM only.

## What it produces
`kiln-gnulinux-arm64.iso`: a bootable UEFI arm64 live image of Kiln GNU/Linux (Debian 13 base) with the preinstalled apps from
`config/package-lists/kiln.list.chroot` (same list as `../provision/packages.list` plus live/boot basics), regular Firefox from
Mozilla's repo (hook 0100, key fingerprint verified), Flathub configured, Kiln branding (no "Debian" in `os-release`/fastfetch),
and the Kiln fastfetch logo.

## Build (on the VM)
```bash
sudo apt install live-build                 # one-off, ~60 MB
scripts/vm/sync-build.sh --install          # Glaze session into ~/.local (existing workflow)
~/kiln-apps/build-all.sh                    # Kiln apps into /usr/local (only if not already installed)
packaging/kilnlinux/image/stage.sh          # copy those into config/includes.chroot (gitignored)
packaging/kilnlinux/image/build.sh --slim   # or without --slim for the full app set
# result: ~/kiln-iso-out/kiln-gnulinux-arm64[-slim].iso (+ .sha256); work dir is deleted afterwards
```
`build.sh` takes `flock /tmp/heavy-build.lock`, checks free disk first (8 GB slim, 16 GB full), runs `lb build` under `nice`,
and removes `~/kiln-iso-work` on exit (success or failure; `--keep` to inspect).

## Resource estimate (not measured yet)
| | slim | full |
|---|---|---|
| download | ~0.9 GB | ~2.5 GB |
| peak disk (chroot + apt cache + squashfs + iso) | ~7 GB | ~14 GB (does NOT fit in the VM's 12 GB free) |
| ISO size | ~1.3 GB | ~2.8 GB |
| time (4 vCPU, shared VM) | ~30-50 min | ~60-100 min |
The slim variant fits the VM now; the full ISO needs more disk (grow the qcow2 or build elsewhere).

## Layout
- `auto/config`: live-build options (trixie, arm64, grub-efi, iso-hybrid, username `glaze`).
- `config/package-lists/`: `kiln.list.chroot` (base apps, live bits, Glaze runtime), `kiln-extra.list.chroot` (heavy apps, dropped by `--slim`).
- `config/hooks/normal/`: 0100 Firefox from Mozilla's repo, 0200 branding + Flathub, 0300 finish Kiln apps/Glaze + Plymouth theme,
  0400 Calamares checks (modules present, YAML parses, `calamares -d` dry parse) and enables the live install hint.
- `arch/<arch>.list`: per-architecture packages (kernel, GRUB: `grub-efi-arm64`, or `grub-efi-amd64` + `grub-pc-bin`); `auto/config`
  copies the one for `KILN_ARCH` to `config/package-lists/arch.list.chroot`. `config/bootloaders/`: the three amd64/arm64 boot entries.
- `config/includes.chroot/`: fastfetch, mimeapps, Qt theme env, tty1 autologin drop-in, `/etc/skel/.bash_profile` (starts the Glaze session on tty1). `stage.sh` adds `usr/local` (Kiln apps + Glaze), Plymouth theme and sway.conf.

## Known gaps
- No `.deb` for Glaze or the Kiln apps: they travel as staged files under `/usr/local` (see `../../kiln-apps/PACKAGING-TODO.md`).
- Installer (Calamares) written but not yet run from a booted ISO (see "Installer").
- Boot test in QEMU (UEFI `edk2-aarch64-code.fd`) not done; Plymouth in the initramfs not verified.

## Installer
"Install Kiln GNU/Linux" (Calamares 3.3, Debian's `calamares` package with Kiln's own settings; `calamares-settings-debian` is not used).

- Packages: `config/package-lists/calamares.list.chroot` (calamares, QML modules for the slideshow, parted/kpartx/gdisk, ext4/btrfs/xfs/vfat
  tools, cryptsetup, squashfs-tools, rsync, efibootmgr, os-prober, user-setup, sudo, pkexec). GRUB for the target is in `arch/<arch>.list`,
  so it is already inside the squashfs that gets copied.
- Settings: `config/includes.chroot/etc/calamares/` - `settings.conf` (sequence), `branding/kiln/` (Glaze Clay look: ink sidebar, paper pages,
  terracotta current step and primary button; `show.qml` slideshow; logo/welcome images from `brand/kiln`), `modules/*.conf`.
- What the install does: partition (erase disk or manual; GPT + 512 MiB ESP; ext4 default, btrfs/xfs; optional LUKS1 full-disk encryption;
  swap file), `unpackfs` copies `/run/live/medium/live/filesystem.squashfs`, `users` creates the chosen user (in `sudo`; root stays locked;
  hostname `kiln`), `packages` removes live-boot/live-config/live-tools and calamares from the copy, `shellprocess@kiln`
  (`/usr/lib/kiln/calamares-finalize`) removes the live tty1 autologin drop-in, the live user `glaze` and the installer files, keeps Kiln in
  `os-release`/`issue` and gives the new user the Glaze session (`~/.bash_profile` from `/etc/skel` starts `glaze-session` on tty1).
  The "log in automatically" box writes a tty1 autologin drop-in for the new user (`contextualprocess` + `calamares-autologin`).
  No display manager: the `displaymanager` module is left out on purpose. Then initramfs, `grubcfg`, `bootloader` (GRUB EFI, id `kiln`,
  plus the removable-media fallback `BOOTAA64.EFI`/`BOOTX64.EFI`; amd64 BIOS installs use grub-pc).
- Launch: `kiln-install.desktop` (Start menu, System; and a desktop icon in the live session) runs `/usr/bin/kiln-install`, which starts
  Calamares as root on the current Wayland display (`sudo -n` for the live user, else `pkexec`) and hides the live `/etc/fstab` meanwhile.
- First boot in live: `kiln-live-hint.service` (systemd user unit, only when `/run/live/medium` exists) adds the desktop shortcut to
  `~/.config/glaze/desktop.toml` and shows one notification pointing at the installer.
- Checks done (2026-11-19): YAML of every config parses (PyYAML); shellcheck 0.11 clean on hook 0400, `kiln-install`,
  `calamares-finalize`, `calamares-autologin`, `kiln-live-hint`, `auto/config`. In the arm64 dev VM (Debian 13, `calamares` 3.3.14, Qt6)
  `calamares -d -c <copy of etc/calamares>` loaded the branding, all 8 view steps and every job module of the sequence with no config errors,
  ran the requirement checks and loaded `show.qml`; on a headless Sway it showed the Kiln-branded welcome page
  (`docs/screens/installer/cal-welcome.png`; it reports "no partitions" because the VM's only disk holds `/`).
- Not testable without booting the ISO: partitioning, unpackfs from the real live medium, the GRUB install on real/virtual firmware,
  LUKS unlock at boot, the first boot of the installed system into Glaze, and the desktop icon/notification in a live session.
