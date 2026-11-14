# AI.md: instructions for an AI agent that builds Kiln GNU/Linux

You are given this repository and asked to build a bootable **x86_64 live ISO** of Kiln GNU/Linux (Debian 13 "trixie" with the Glaze desktop environment). Read this file fully before running anything.

## Goal and result

- Output: `kiln-gnulinux-amd64.iso` (hybrid BIOS + UEFI live ISO) and its `SHA256SUMS`, in `./out/`.
- The user will write it to a USB stick and boot real x86_64 hardware. This is a live ISO: it boots without installing.

## Rules

1. Build only inside a Debian 13 container or VM. Never run live-build on the user's host OS directly.
2. Do not edit files outside this repository. Do not push, publish or upload anything unless the user asks.
3. Never enter passwords, tokens or keys. If a step needs credentials, stop and ask the user.
4. Report honestly: say what built, what failed, and what you did not test. Do not claim the ISO boots unless you booted it.
5. Do not delete anything outside the build work directory.

## Repository map

| Path | Purpose |
|---|---|
| `glaze/` | Glaze desktop environment (Rust workspace: compositor, shell, settings, session) |
| `kiln-apps/` | Themed app forks and the Kiln Store; `kiln-apps/build-all.sh` builds them |
| `brand/` | Logos, wallpapers |
| `glaze/packaging/kilnlinux/image/` | live-build recipe (`auto/config`, `config/package-lists`, `config/hooks`, `config/includes.chroot`, `build.sh`, `stage.sh`, README) |
| `build/x86/` | x86_64 build scripts (`build-iso.sh`, `Dockerfile`), if present |
| `.github/workflows/build-iso.yml` | GitHub Actions build, if present |

## Fastest path: use the prepared scripts

If `build/x86/build-iso.sh` exists, read `build/README.md` and follow it. In short:

Windows (Docker Desktop with the WSL2 backend), from the repo root in PowerShell:

```powershell
docker build -t kiln-iso-builder build/x86
docker run --rm --privileged -v "${PWD}:/work" -w /work kiln-iso-builder ./build/x86/build-iso.sh
```

Linux or WSL2 Ubuntu with Docker:

```bash
docker build -t kiln-iso-builder build/x86
docker run --rm --privileged -v "$PWD:/work" -w /work kiln-iso-builder ./build/x86/build-iso.sh
```

GitHub Actions (no local toolchain): run the "build-iso" workflow from the Actions tab and download the artifact.

Expect 1 to 3 hours the first time (Rust compile plus debootstrap) and about 15 GB of free disk space.

## If the prepared scripts are missing: manual procedure

Run inside a `debian:13` amd64 container started with `--privileged`:

1. Install tools: `apt-get update && apt-get install -y live-build debootstrap squashfs-tools xorriso syslinux syslinux-common isolinux grub-efi-amd64-bin grub-pc-bin mtools dosfstools rsync git curl ca-certificates build-essential pkg-config`.
2. Install the Rust toolchain (rustup, stable) and the development libraries needed by Glaze (GTK 4, gtk4-layer-shell, wayland, xkbcommon, libinput, udev, seatd, gbm, EGL, pixman, dbus). Read `glaze/Cargo.toml` and the crates' `Cargo.toml` files to confirm the list; install what the build asks for.
3. Build Glaze in release mode: `cd glaze && cargo build --release --workspace`, then each app under `glaze/apps/*` with `cargo build --release` in its directory.
4. Build the app forks with `kiln-apps/build-all.sh` (read it first; it lists the build dependencies).
5. Stage the binaries into `glaze/packaging/kilnlinux/image/config/includes.chroot/usr/local/bin` (see `stage.sh`; adapt it to the paths where you built).
6. In `glaze/packaging/kilnlinux/image/`, change every `arm64` to `amd64` in `auto/config` and in `config/package-lists/*` (`linux-image-amd64`, `grub-efi-amd64`, `grub-pc`, `syslinux`). Use `--binary-images iso-hybrid --bootloaders "syslinux,grub-efi"`. Keep `non-free-firmware` enabled.
7. Run `lb clean --purge && lb config && lb build` as root in that directory. Fix hook failures at their source.
8. Copy the resulting `*.iso` to `./out/` and write `SHA256SUMS`.

## Verification before reporting

- `file out/*.iso` shows an ISO 9660 image with an x86 boot sector, and the size is plausible (roughly 1 to 3 GB).
- If you can, boot it with QEMU: `qemu-system-x86_64 -m 4096 -cdrom out/kiln-gnulinux-amd64.iso -boot d` (add `-bios /usr/share/ovmf/OVMF.fd` for UEFI). Check that the Glaze desktop appears. Emulated boot is slow; say if you skipped it.

## Writing to USB (tell the user)

Use Rufus (DD Image mode) or balenaEtcher on Windows. Verify the SHA256 first. This erases the USB stick. For boot problems on real hardware: disable Secure Boot, try the "safe graphics" boot entry (`nomodeset`), and collect `journalctl -b` from the live session.

## Known state

- Early beta, developed and tested on aarch64 in QEMU. The x86_64 build is untested on real hardware.
- There is no installer yet; live session only.
- The session starts Glaze on its own compositor and falls back to Sway if it fails.
