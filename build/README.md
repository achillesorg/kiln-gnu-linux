# Building the Kiln GNU/Linux live ISO for real x86_64 hardware

Result: `kiln-gnulinux-amd64.iso` (+ `.sha256`), a hybrid image that boots on **BIOS and UEFI** PCs (Secure Boot must be off).
Debian 13 base, live user `glaze` (autologin on tty1 into the Glaze desktop), nothing is written to the disk, persistence is off.

**Status: written but never run end to end.** The amd64 recipe was only checked with `lb config` (dry run) and `bash -n`;
the Docker image, the Rust/app compile on amd64, the ISO build and the boot on real hardware are untested (see the end).
Expect to have to fix things; the log tells you what (`out/lb-build.log` for stage 2, the console for stage 1).

Variants: full (default, about 2.5-3 GB: LibreOffice, GIMP, Inkscape, VLC, Thunderbird, Firefox...) or `--slim` (about 1.3 GB, no heavy apps).

The three ways below run the same script (`build/x86/build-iso.sh`). Needs: 8 GB RAM (WSL2/Docker VM), 30 GB free disk, internet.
A build takes roughly 1 to 2 hours (Rust + GTK apps compile, then about 700 Debian packages and the squashfs).

## (c) GitHub Actions (easiest: no local toolchain)
1. Open the repo on GitHub, tab **Actions**, workflow **Build live ISO (x86_64)**, **Run workflow** (branch `beta`; tick *slim* for the small image).
2. Wait for the run to finish (about 1.5-2.5 hours on the free runners).
3. Open the finished run, section **Artifacts**, download `kiln-gnulinux-amd64-iso` (a zip with the `.iso`, `.sha256` and `lb-build.log`). The repo is private, so you must be logged in with access.
4. To publish as a release: push a tag, e.g. `git tag v0.1.0-beta && git push origin v0.1.0-beta`. The ISO is attached to a pre-release.

## (a) Windows PowerShell with Docker Desktop
One-off: install Docker Desktop, enable **Use the WSL 2 based engine** (Settings, General) and give it at least 8 GB RAM and 40 GB disk
(`%UserProfile%\.wslconfig`: `[wsl2]` / `memory=8GB`). Install Git for Windows.
```powershell
git config --global core.autocrlf false
git clone -b beta https://github.com/hexsourceorg/kiln-gnu-linux.git
cd kiln-gnu-linux
docker build --platform linux/amd64 -t kiln-builder build/x86
New-Item -ItemType Directory -Force out | Out-Null
docker run --rm --privileged --platform linux/amd64 `
  -v "${PWD}:/src:ro" -v "${PWD}\out:/out" `
  -v kiln-work:/work -v kiln-cargo-registry:/opt/cargo/registry -v kiln-cargo-git:/opt/cargo/git `
  kiln-builder bash /src/build/x86/build-iso.sh --in-container
Get-FileHash out\kiln-gnulinux-amd64.iso -Algorithm SHA256   # compare with out\kiln-gnulinux-amd64.iso.sha256
```
Slim image: add `-e KILN_ISO_VARIANT=slim` before `kiln-builder` (the file is then `kiln-gnulinux-amd64-slim.iso`).
Reset caches: `docker volume rm kiln-work kiln-cargo-registry kiln-cargo-git`. The `kiln-work` volume keeps compiled Rust between runs.
`core.autocrlf false` matters: CRLF line endings in the shell scripts break the build (the repo also ships a `.gitattributes` forcing LF).

## (b) WSL2 Ubuntu (or any Linux host with Docker)
```bash
sudo apt install -y docker.io git            # on WSL2 you can instead use Docker Desktop's WSL integration
sudo usermod -aG docker "$USER" && newgrp docker
git clone -b beta https://github.com/hexsourceorg/kiln-gnu-linux.git
cd kiln-gnu-linux
build/x86/build-iso.sh            # add --slim for the small image
sha256sum -c out/kiln-gnulinux-amd64.iso.sha256
```
Clone inside the WSL filesystem (`~/`), not under `/mnt/c`. On a Debian 13 amd64 machine without Docker you could run
`build/x86/build-iso.sh --in-container` as root after installing the packages listed in `build/x86/Dockerfile`; that path is untested.

## Writing the ISO to a USB stick on Windows
1. Verify the checksum first: `Get-FileHash .\kiln-gnulinux-amd64.iso -Algorithm SHA256` must equal the contents of the `.sha256` file.
2. **Rufus**: select the ISO, partition scheme *MBR* for old BIOS PCs or *GPT* for UEFI PCs, click Start, and choose **Write in DD Image mode** when asked
   (not ISO mode; the image is a hybrid ISO and needs a raw copy). **balenaEtcher** also works (select ISO, select stick, Flash).
3. Use a stick of at least 4 GB (8 GB for the full image with "Live to RAM"). Everything on it is erased.

## Real-hardware checklist
- **Secure Boot: turn it off** in the firmware setup (the GRUB image is not signed with a Microsoft-trusted key). BIOS/CSM or UEFI both work.
- Boot menu key (F12, F10, Esc, F9 depending on the vendor), pick the USB stick (the "UEFI:" entry for UEFI mode).
- Boot menu entries: **Live**, **Live (safe graphics: nomodeset, Sway session)**, **Live to RAM** (copies the system into memory, about 4 GB RAM needed, stick can be removed).
- If the screen stays black or the desktop does not appear: reboot and pick *safe graphics*. Glaze starts its own DRM compositor first and falls back to Sway by itself; safe graphics goes straight to Sway on the simple framebuffer driver.
- Graphics: Intel and AMD (Mesa) and NVIDIA through the open **nouveau** driver only. The proprietary NVIDIA driver is **not supported** in this image.
- Wi-Fi/Bluetooth firmware for Intel, Realtek, Atheros, Broadcom (brcm80211) is included; other chips (e.g. Broadcom wl, some MediaTek) may not work.
- Login is automatic as user `glaze`. `sudo` is available (live-config default password `live`).
- What to expect: Glaze taskbar with the Start button, Kiln Files, Terminal, Text, Viewer, Calculator, Monitor and Store, Firefox. Nothing is installed to disk (no installer yet).
- Reporting problems: say the PC model, CPU/GPU, BIOS or UEFI, which boot entry, and what happened. From the live desktop open Kiln Terminal and collect:
  `sudo dmesg > ~/dmesg.txt; lspci -nnk > ~/lspci.txt; cat ~/session.log > ~/glaze-session.txt; sudo journalctl -b > ~/journal.txt`
  then copy them to a second USB stick, or take phone photos of the screen if the desktop never starts. For a black screen, switch to a text console with Ctrl+Alt+F2 and run the same commands.

## What is (not) verified
Verified here: `bash -n` on all scripts, the workflow YAML parses, `lb config` for `--architectures amd64` with the Kiln recipe in a Debian 13 arm64 VM
(bootloaders `syslinux grub-efi`, per-arch package list generated, custom boot menu files picked up), the apt package names of the amd64 list and the Dockerfile exist in Debian 13 (checked on arm64; the amd64-only ones, e.g. `grub-pc-bin`, `grub-efi-amd64-bin`, `amd64-microcode`, `intel-microcode`, `intel-media-va-driver`, could not be checked there).
**Not verified:** `docker build` of the builder, the amd64 compile of Glaze and the Kiln apps, `lb build` for amd64, the BIOS/UEFI boot menu actually showing the three entries, booting in QEMU (x86 emulation here is far too slow and no amd64 ISO exists yet), booting on real hardware,
the Glaze DRM compositor on real GPUs, Wi-Fi/firmware, `toram`, and the GitHub Actions run. No `shellcheck` was available.

## Files
- `build/x86/Dockerfile`: Debian 13 amd64 builder: Rust (rustup stable, Glaze needs 1.92+), GTK4/layer-shell/wayland/xkbcommon/libinput/udev/seat/gbm dev libs, Kiln apps build-deps (via `apt-get build-dep` of the forked packages), live-build, syslinux, grub-efi-amd64-bin, xorriso.
- `build/x86/build-iso.sh`: host side (docker build + docker run --privileged) and container side (stage 1 compile, stage 2 live-build).
- `glaze/packaging/kilnlinux/image/`: the live-build recipe, shared with arm64 (`KILN_ARCH`, `arch/<arch>.list`, `config/bootloaders/` for the 3 boot entries).
- `.github/workflows/build-iso.yml`: Actions workflow.
