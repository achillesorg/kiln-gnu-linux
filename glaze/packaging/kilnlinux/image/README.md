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
- `config/hooks/normal/`: 0100 Firefox from Mozilla's repo, 0200 branding + Flathub, 0300 finish Kiln apps/Glaze + Plymouth theme.
- `config/includes.chroot/`: fastfetch, mimeapps, Qt theme env, tty1 autologin drop-in, `/etc/skel/.bash_profile` (starts the Glaze session on tty1). `stage.sh` adds `usr/local` (Kiln apps + Glaze), Plymouth theme and sway.conf.

## Known gaps
- No `.deb` for Glaze or the Kiln apps: they travel as staged files under `/usr/local` (see `../../kiln-apps/PACKAGING-TODO.md`).
- No installer (Calamares) yet: live session only.
- Boot test in QEMU (UEFI `edk2-aarch64-code.fd`) not done; Plymouth in the initramfs not verified.
