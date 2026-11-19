# Kiln GNU/Linux: first-class driver support

Status: design proposal. Nothing here is implemented. Facts marked **(verify)** were not confirmed against a primary source and need a check before work starts. Package versions were read from tracker.debian.org on 2026-11-18.

Scope: proprietary and non-free hardware support on a Debian 13 (trixie) base, with a newer-than-stable software track. Covers Nvidia, AMD/Intel firmware, Wi-Fi, Bluetooth, fingerprint, printers/scanners, webcams, codecs, microcode, an offline payload on the ISO, hardware detection, the Kiln Driver Manager, Calamares integration, live-session boot behaviour, and kernel updates.

Primary arch for Kiln is arm64 (phone, VM), but proprietary desktop drivers matter on x86_64. Everything below is x86_64 unless stated. The phone image (TilesDE) ships only per-device firmware, owned by the device port, see section 12.

## 0. Ground facts that drive the design

| Fact | Source / status |
|---|---|
| Debian trixie `nvidia-graphics-drivers` is 550.163.01-2. trixie-backports has 550.163.01-4~bpo13+1. Testing and unstable are 550.163.01-6. Debian has no 570/580/590 packages in any suite. | tracker.debian.org, read today |
| Debian `nvidia-open-kernel-dkms` is in `contrib`, also 550.x. | tracker.debian.org |
| trixie kernel is 6.12.x. trixie-backports and testing carry 7.3.4. | tracker.debian.org `linux` |
| Forum reports: backports kernel 6.17 builds the 550 closed flavour but not the open flavour, and 6.16+ needs 580+. | NVIDIA dev forum thread, anecdotal **(verify by CI build)** |
| Sway 1.10 / wlroots 0.18 need `--unsupported-gpu` on the proprietary driver. 555+ added `linux-drm-syncobj-v1` explicit sync. | Sway docs, Phoronix **(verify exact trixie Sway version)** |
| Reports of Sway/Hyprland regressions on 580.x with explicit sync (Xid 79 on Pascal, `__NV_DISABLE_EXPLICIT_SYNC` workaround in egl-wayland). | forum reports, anecdotal |
| `packaging/kilnlinux/image/auto/config` already enables `main contrib non-free non-free-firmware`. | repo |

Consequence: "newer than Debian" and "first-class Nvidia" are the same project. Stock Debian cannot pair its newest kernel with its Nvidia driver. Kiln must own an Nvidia packaging pipeline and a kernel/driver pairing gate.

## 1. Policy and decisions summary

1. Kiln ships its own apt repository, component `kiln-nonfree`, built from Debian's source packaging with newer upstream tarballs (Nvidia) and the same package names plus a `+kiln` version suffix, pinned above Debian.
2. Kiln tracks one Debian kernel, trixie-backports, through a gated metapackage `kiln-kernel`. A new kernel ABI reaches users only after Kiln CI has built and boot-tested the driver modules for it.
3. Nvidia: open kernel modules for Turing and newer, closed modules for Maxwell/Pascal/Volta, nouveau/NVK for Kepler and older. Prebuilt modules signed by a Kiln module-signing key, DKMS with a per-machine key as fallback.
4. Secure Boot: keep Debian's signed shim, GRUB and kernel unchanged. Kiln adds one MOK enrolment (Kiln module-signing certificate), done in the installer or at first use of the Driver Manager.
5. The live session defaults to open drivers (nouveau/NVK, amdgpu, i915/xe). Proprietary Nvidia in live is offered only when Secure Boot is off. The installer installs proprietary drivers into the target from an offline payload.
6. A single hardware-to-package mapping table (`kiln-hwdb`) drives the Driver Manager, the Calamares page, the first-login notifier and CI.
7. Nothing legally doubtful goes on the ISO (libdvdcss, HPLIP plugin, Epson vendor debs). They are opt-in, user-initiated downloads with a notice.

## 2. Repository and package layout

**Decision.** Three apt sources on the installed system, in this priority order:

| Source | Content | Pin |
|---|---|---|
| `kiln` (kiln-repo.example) | Kiln-built: `kiln-*` tools, `nvidia-*` newer branches, `kiln-kernel`, per-ABI module packages | 700 |
| `trixie-backports` | kernel, Mesa, Firefox ESR alternatives, other newer packages if present (see the newer-software design doc) | 100, opt-in per package via `-t` or Kiln pin files; kernel and Mesa pinned to 600 |
| `trixie` + `trixie-security` + `trixie-updates` | base | 500 |

Rationale: Debian's backports policy (`NotAutomatic: yes`, `ButAutomaticUpgrades: yes`) already gives security-tracked newer packages. Nvidia is not in backports at a useful version, so Kiln builds it.

**Nvidia packaging.** Base on Debian's `nvidia-graphics-drivers` source, bump upstream. Binary packages keep Debian names (`nvidia-driver`, `libnvidia-*`, `nvidia-kernel-dkms`, `nvidia-open-kernel-dkms`, `nvidia-vaapi-driver` is separate main package) with versions like `580.x-1+kiln1`, so `apt` upgrades replace Debian's.

New per-ABI prebuilt module packages: `kiln-nvidia-modules-580-open-6.x.y-z-amd64`, `kiln-nvidia-modules-580-closed-...`. Each `Depends: linux-image-<abi>` and the matching `nvidia-*` userspace of the exact same driver version. A virtual package `kiln-nvidia-modules` is provided by whichever is installed; `kiln-kernel` `Recommends` it.

Branch policy:

| Branch | For | Notes |
|---|---|---|
| 580 (production, long-lived) **(verify support window)** | Maxwell, Pascal, Volta (closed modules) and Turing+ (open modules) | last branch with Maxwell/Pascal/Volta **(verify)** |
| Latest production (590/595 class) **(verify current)** | Turing+ only, open modules | offered as "newer" choice in Driver Manager, not default |
| nouveau + NVK (Mesa) | Kepler and older (Debian dropped legacy 390/470 branches **(verify)**), and any Nvidia GPU as a free alternative | no proprietary code |

Default branch is the long-lived one. The newest branch is selectable. Only one branch is installed at a time.

Files and components:

- `/Users/xavi/fedeos/glaze/packaging/kilnlinux/repo/` (new): reprepro or aptly config, signing key, suite `trixie-kiln`.
- `packaging/kilnlinux/nvidia/` (new): packaging overlay, CI scripts, `supported-gpus.json` import.
- `/Users/xavi/fedeos/stage-kiln/build` x86 pipeline: add repo to `--archive-areas`/`--apt-source-archives` and `config/archives/kiln.list.chroot` plus key.

Work breakdown:

| Task | Size |
|---|---|
| Stand up repo, signing key custody, publish pipeline | M |
| Nvidia source package bump pipeline (watch upstream, rebuild, lint) | L |
| Per-ABI prebuilt module build farm (chroot per kernel ABI, sign, publish) | L |
| `kiln-kernel` gating metapackage and CI gate | M |
| Pin files, `kiln-archive-keyring`, `kiln-sources` | S |

Risks: build and key infrastructure is ongoing operational load for a hobby project. Mitigation: start with one driver branch, one kernel, x86_64 only; GitHub Actions or a single self-hosted builder.

Acceptance:
- Fresh install: `apt policy nvidia-driver` shows the Kiln version as candidate.
- `apt-get install kiln-kernel` on a machine with both repos never selects a kernel for which `kiln-nvidia-modules-*` is missing (CI test with a deliberately withheld ABI).
- Repo metadata signature verifies with `kiln-archive-keyring` only.

## 3. Nvidia

### 3.1 Open vs closed modules, DKMS vs prebuilt

**Decision.**
- Turing (GTX 16xx, RTX 20xx) and newer: open kernel modules (`nvidia-open`). Blackwell (RTX 50) works only with open modules. Closed userspace libraries are the same either way, and the open driver needs the GSP firmware blob (`firmware-nvidia-gsp`, non-free-firmware).
- Maxwell, Pascal, Volta: closed modules, branch 580.
- Kepler and older: nouveau.
- Distribution: prebuilt signed modules per kernel ABI are the default. DKMS (`nvidia-open-kernel-dkms` / `nvidia-kernel-dkms`) is the fallback for custom or unsupported kernels, selectable in Driver Manager Advanced.

Rationale: prebuilt means no compiler or headers on end-user machines, no 1 to 3 minute DKMS build during kernel update, and a single trust root (one MOK enrolment) for all future modules. DKMS means a per-machine key and a build that can break on any new kernel. Ubuntu uses prebuilt `linux-modules-nvidia-*` and DKMS fallback; Fedora/RPM Fusion uses akmods; Pop!_OS ships prebuilt. Kiln follows the Ubuntu shape.

The PCI-ID to flavour decision comes from `supported-gpus.json`, shipped inside each upstream driver release, imported into `kiln-hwdb` by CI. This is the same source `nvidia-detect` uses, so Debian parity is preserved.

Config shipped by `kiln-nvidia-config`:

```
# /etc/modprobe.d/kiln-nvidia.conf
blacklist nouveau
options nouveau modeset=0
options nvidia-drm modeset=1 fbdev=1
options nvidia NVreg_PreserveVideoMemoryAllocations=1 NVreg_TemporaryFilePath=/var/tmp
# laptops with Turing+ dGPU only (written by udev helper, not globally):
# options nvidia NVreg_DynamicPowerManagement=0x02
```

Plus enabling `nvidia-suspend.service`, `nvidia-hibernate.service`, `nvidia-resume.service`, and `nvidia-powerd.service` on Ampere+ laptops. `update-initramfs -u` after blacklist change. `nvidia-drm.modeset=1` is required for GBM/KMS. `fbdev=1` fixes the console on kernels >= 6.11 and is default from driver 570 **(verify)**; we set it explicitly.

Work:

| Task | Size |
|---|---|
| `kiln-nvidia-config` package (modprobe, services, udev runtime PM) | M |
| `supported-gpus.json` to hwdb generator | S |
| Module-signing pipeline (sign-file with Kiln key, offline key or HSM) | M |
| DKMS fallback tested against each kernel ABI in CI | M |

### 3.2 Secure Boot and MOK

Constraint: under Secure Boot the Debian kernel enters lockdown and refuses unsigned modules. Debian's shim, GRUB and kernel are signed by Debian and trusted via the Microsoft-signed shim. Kiln cannot sign a bootloader or kernel that boots on stock firmware (that needs a shim-review submission and Microsoft signing, an L-size, out-of-scope item). Kiln therefore keeps Debian's signed `shim-signed`, `grub-efi-amd64-signed`, `linux-image-*-signed` binaries unmodified and trusts its own modules via MOK.

Key design:
- One Kiln module-signing certificate (`kiln-module-signing.der`), shipped in `kiln-secureboot-keys` at `/usr/share/kiln-drivers/`. Its X.509 extendedKeyUsage is the module-signing OID (1.3.6.1.4.1.2312.16.1.2) only, so the key can sign kernel modules but is not a general code-signing CA.
- Private key offline or in an HSM, signing on the build farm through a short-lived signing request. Key rotation policy: 2 year validity, new cert enrolled by `kiln-drivers` on update with the user's consent.
- Risk statement: a compromised Kiln key allows ring-0 code on every user that enrolled it. MOK has no remote revocation. Mitigation is custody, rotation, and the per-machine-key alternative for users who prefer it.

Installed-system flow (Calamares or Driver Manager):

1. Detect `mokutil --sb-state` is "SecureBoot enabled". If disabled, skip all of this.
2. If `mokutil --test-key kiln-module-signing.der` says already enrolled, skip.
3. UI asks the user to choose a one-time password (8 to 16 chars) and explains the blue MokManager screen: "Enroll MOK", "Continue", "Yes", enter password, "Reboot". Screenshot-style illustrated steps in the dialog. MokManager shows a US keyboard layout, so the UI warns to avoid non-ASCII.
4. Run `mokutil --import /usr/share/kiln-drivers/kiln-module-signing.der` (password via stdin to a hash file, not on a command line).
5. UI: "Restart now" button. After reboot the Driver Manager checks `mokutil --list-enrolled` and `modinfo -F sig_id nvidia`, then confirms. If the user skipped MokManager, driver install stays complete but modules do not load; the Driver Manager shows "Secure Boot key not enrolled" with a retry button, and the session falls back to open drivers via the guard in section 8.4.
6. DKMS fallback key path: `dkms` generates `/var/lib/dkms/mok.key` and `mok.pub` (Debian's dkms does this automatically when Secure Boot is on); the same MOK flow is run with `mokutil --import /var/lib/dkms/mok.pub`.

Live-session: enrolment writes NVRAM on the user's machine and persists. We do not do it from the live session by default. Instead live uses open drivers when Secure Boot is on, and the installer performs enrolment into the installed system (that also persists in firmware, which the user is told).

Work:

| Task | Size |
|---|---|
| `kiln-secureboot-keys`, key generation runbook, EKU | S |
| `kiln-drivers mok {status,enrol}` backend | M |
| Illustrated MokManager guide in UI, translations | M |
| Enrolment test matrix on 3 firmwares (OVMF, AMI, Insyde) | M |
| shim-review path as separate future project | L (not scheduled) |

Branding constraint (cross-reference the branding doc): `shim-signed`, `grub-efi-*-signed` and signed kernels keep "Debian" strings (SBAT entries, signed image names). Kiln may set `grub-install --bootloader-id=kiln` (ESP directory `EFI/kiln/`; shim signature is independent of the path) and use its own GRUB theme and menu text, but must not repack or re-sign these binaries. Tests must cover the renamed ESP path booting under Secure Boot.

### 3.3 Wayland, GBM, explicit sync

State of play:
- GBM with `nvidia-drm.modeset=1` is supported since 495. Explicit sync (`linux-drm-syncobj-v1`) arrived with 555. Nvidia's Wayland support improved in 570 and 580, with the 580.x regressions above noted by users.
- Sway 1.10 / wlroots 0.18 (trixie) support explicit sync. Sway refuses the proprietary driver without `--unsupported-gpu`.
- Smithay (Glaze compositor) uses GBM + EGL (and a Vulkan path is not yet available) and has `linux-drm-syncobj-v1` state in its protocol set **(verify in the pinned Smithay version)**.

**Decision.**
- Sway fallback backend: `glaze-session run --backend sway` adds `--unsupported-gpu` automatically when `/proc/driver/nvidia/version` exists, and sets `WLR_NO_HARDWARE_CURSORS=1` only if cursor corruption is detected via the guard (not by default).
- Glaze compositor on Nvidia: requirements to treat as first-class acceptance items (all in `glaze-compositor`):
  1. Open the primary GPU via logind and pick it with a deterministic rule: `boot_vga` flag, then DRM device that owns the outputs, overridable by `GLAZE_DRM_DEVICES` (colon list, same semantics as `WLR_DRM_DEVICES`).
  2. Allocate scanout buffers with GBM using format modifiers from the `IN_FORMATS` plane property; fall back to linear on failure.
  3. Advertise `linux-dmabuf-v1` with the device's tranche list, `wp_linux_drm_syncobj_v1` when `DRM_CAP_SYNCOBJ_TIMELINE` is present, and honour `GLAZE_DISABLE_EXPLICIT_SYNC=1` for debugging.
  4. Atomic KMS with `DRM_CLIENT_CAP_ATOMIC`; do not assume `DRM_CAP_ADDFB2_MODIFIERS` on old driver builds.
  5. Software cursor fallback (`GLAZE_SOFTWARE_CURSOR=1`) when the hardware cursor plane fails an atomic test commit.
  6. Multi-GPU: render on the primary, copy to outputs on the secondary GPU through dmabuf import, falling back to a CPU copy.
- Environment exported to the user session on Nvidia: `LIBVA_DRIVER_NAME=nvidia` (only when `nvidia-vaapi-driver` is the chosen VA-API backend), `NVD_BACKEND=direct`, `GBM_BACKEND` is NOT set globally (it breaks Mesa clients on hybrid laptops). `__GLX_VENDOR_LIBRARY_NAME` is NOT set globally.
- Firefox: set `media.ffmpeg.vaapi.enabled=true` and `media.rdd-ffmpeg.enabled=true` policy defaults for hardware video decode on Nvidia (NVDEC through `nvidia-vaapi-driver`), and Chromium flags `--enable-features=VaapiVideoDecoder`.

Work:

| Task | Size |
|---|---|
| `glaze-session` Nvidia detection, `--unsupported-gpu`, env policy | S |
| Compositor primary-GPU selection and `GLAZE_DRM_DEVICES` | M |
| Modifier negotiation and explicit-sync advertisement on Nvidia | L |
| Cursor plane fallback and software cursor | M |
| Multi-GPU copy path | L |
| Test rig: RTX 30xx desktop, Pascal laptop | M |

Risks: Nvidia-specific compositor bugs are the largest unknown in this whole document. The Sway fallback must stay shippable throughout. Explicit-sync regressions per driver version are handled by the CI gate in section 3.6 and the `GLAZE_DISABLE_EXPLICIT_SYNC` escape hatch.

Acceptance (on real Nvidia hardware, never claimed from the VM):
- `glaze-session run --backend sway` reaches the desktop with `nvidia-drm modeset=Y` (`cat /sys/module/nvidia_drm/parameters/modeset`), `swaymsg -t get_outputs` lists the panel at native mode.
- Same with `--backend glaze` (own compositor); `glaze status` reports renderer `GBM/EGL nvidia`, explicit sync on.
- `vkcube` and `glxgears` via Xwayland at monitor refresh with no tearing; `eglinfo -B` shows the Nvidia vendor.
- Suspend and resume 10 times in a row without a black screen (`NVreg_PreserveVideoMemoryAllocations=1` active).

### 3.4 Optimus and PRIME

**Decision.** Three GPU modes, default "Hybrid":

| Mode | Behaviour | Implementation |
|---|---|---|
| Hybrid (default) | iGPU drives the panel and the compositor; dGPU powers up on demand for offload | `switcheroo-control` (main) D-Bus service; Glaze launcher "Run with dedicated GPU"; `nvidia` runtime PM `NVreg_DynamicPowerManagement=0x02` on Turing+ laptops |
| Discrete | dGPU primary; iGPU off or unused. Needed for panels or HDMI wired to the dGPU, and MUX laptops | `GLAZE_DRM_DEVICES=/dev/dri/by-path/<dgpu>` set via `kiln-gpu-mode`, plus `options nvidia-drm modeset=1` |
| Integrated | dGPU powered down, Nvidia modules blacklisted | modprobe blacklist plus udev `power/control=auto` or ACPI off |

Offload env (for non-`switcherooctl` launching): `__NV_PRIME_RENDER_OFFLOAD=1 __GLX_VENDOR_LIBRARY_NAME=nvidia __VK_LAYER_NV_optimus=NVIDIA_only __EGL_VENDOR_LIBRARY_FILENAMES=/usr/share/glvnd/egl_vendor.d/10_nvidia.json`. For AMD/Intel/nouveau dGPUs: `DRI_PRIME=1`. The launcher reads `.desktop` keys `PrefersNonDefaultGPU=true` and `X-KDE-RunOnDiscreteGraphics=true` (both used by existing apps) and calls `switcherooctl launch`. Mode switching changes the session environment, so the Driver Manager asks to log out, not to reboot (except Integrated, which reboots).

Components: `kiln-gpu-mode` CLI (shell, root helper via polkit), a Glaze launcher integration, Glaze panel indicator showing the active GPU, `kiln-hwdb` flags `hybrid=true`.

Work: `kiln-gpu-mode` M; launcher integration and `.desktop` handling M; panel indicator S; AMD+Nvidia and Intel+Nvidia laptop tests M.

Acceptance: on a hybrid laptop, `switcherooctl list` shows both GPUs; `glxinfo -B` inside "Run with dedicated GPU" reports the Nvidia renderer while the plain session reports Intel/AMD; `cat /sys/bus/pci/devices/<dgpu>/power/runtime_status` is `suspended` while idle in Hybrid mode.

### 3.5 Hardware video acceleration (NVDEC and others)

- Nvidia: `nvidia-vaapi-driver` (Debian main) provides VA-API over NVDEC. Needs `nvidia-drm modeset=1`. It decodes only; encode uses NVENC through ffmpeg directly. Known limits: no VP8 encoding, some browsers need explicit flags, Firefox sandbox needs `MOZ_DISABLE_RDD_SANDBOX=1` on some builds **(verify on current Firefox ESR)**.
- Intel: `intel-media-va-driver` (main, Broadwell and later), `i965-va-driver` for older. `intel-media-va-driver-non-free` (non-free) enables HuC-dependent low-power encode; offered in Driver Manager as optional, in ISO payload (it is redistributable).
- AMD: `mesa-va-drivers` (radeonsi VCN). Whether Debian's Mesa includes patent-encumbered H.264/HEVC in VA-API **(verify; Fedora disables them, Debian historically does not)**. If disabled in trixie Mesa, Kiln rebuilds Mesa from backports with them on, which is a legal-review item (section 11).
- `vainfo` and `vdpauinfo` shipped in the live image for support.

Acceptance: `vainfo` lists H264/HEVC/AV1 profiles per GPU class; `mpv --hwdec=vaapi` shows `Using hardware decoding (vaapi)`; Firefox `about:support` "Hardware Video Decoding" is "Yes".

### 3.6 Nvidia and kernel/driver CI gate

For each new upstream Nvidia release and each new Debian kernel ABI:
1. Build open and closed prebuilt modules and DKMS in a clean chroot. Fail the gate on any build failure.
2. Boot test in a VM with GPU passthrough on the Nvidia test rig (a real GPU; VMs without passthrough cannot test this), load module, `nvidia-smi`, start Glaze and Sway, 30 s `vkcube`.
3. Only if all pass, publish modules and bump `kiln-kernel` and `kiln-nvidia-modules` to the new ABI.

Work: L. Risks: needs real GPUs in CI. Acceptance: the gate blocks a deliberately broken driver/kernel pair.

## 4. AMD, Intel, microcode and general firmware

**Decision.** Ship all of `non-free-firmware` that Debian ships for hardware enablement, on the ISO and in the installed system, plus microcode. Firmware is data, not code run by the kernel, and Debian's own installer images include it since 12.

Packages (trixie names, **verify** each against `apt-cache` on the build host, Debian split `firmware-misc-nonfree` several times):

| Class | Package(s) | Notes |
|---|---|---|
| AMD GPU | `firmware-amd-graphics` | required for amdgpu |
| Intel GPU | `firmware-intel-graphics` (HuC/GuC/DMC) | |
| Intel misc and BT | `firmware-intel-misc`, `firmware-iwlwifi` | |
| Intel audio | `firmware-sof-signed` | SOF DSP laptops, many 2019+ models |
| Realtek | `firmware-realtek` | Wi-Fi, BT, Ethernet |
| Atheros | `firmware-atheros` | |
| Broadcom FullMAC | `firmware-brcm80211` | brcmfmac, Macs and Chromebooks |
| Mediatek | `firmware-mediatek` | mt7921, mt7922 |
| Other | `firmware-misc-nonfree`, `firmware-linux-nonfree` meta, `firmware-nvidia-gsp` | |
| Microcode | `intel-microcode`, `amd64-microcode` | loaded early from initramfs, tracked via `trixie-security` |

All are in `package-lists/` of the image recipe (`packaging/kilnlinux/image/config/package-lists/kiln-firmware.list.chroot`, new). Installed target gets them from the offline payload (section 6). Microcode is installed unconditionally (Intel or AMD per CPU vendor via hwdb) because of security fixes.

Kernel-side: `firmware_class.path` default is fine. `dmesg | grep -i firmware` health check in `kiln-drivers doctor` flags "firmware: failed to load" lines and maps the filename to the package through `apt-file`-style data shipped in hwdb (`firmware-map.json`, generated from Debian `Contents` for the firmware packages).

Work: package list S; `kiln-drivers doctor` firmware map M. Risks: Debian splits firmware packages between releases; mitigate by CI check that every package name in the list resolves. Acceptance: on an Intel+Wi-Fi laptop and an AMD APU, `dmesg --level=err,warn | grep -i "firmware.*failed"` is empty after boot; `grep microcode /proc/cpuinfo` shows the updated revision.

## 5. Peripheral drivers

### 5.1 Wi-Fi

| Chip family | Default driver | Fallback in hwdb |
|---|---|---|
| Intel (iwlwifi) | kernel + `firmware-iwlwifi` | none |
| Realtek RTL88xx (rtw88/rtw89) incl. 8821CE, 8822CE, 8852AE | kernel `rtw88_*` / `rtw89_*` + `firmware-realtek` | `rtl8821ce-dkms` listed only for kernels where in-tree driver is broken for that ID; keep as "experimental" **(verify DKMS package exists in trixie)** |
| Realtek USB (8812AU, 8821AU, 8811CU...) | in-tree if present for kernel | DKMS from Kiln repo only on explicit user action (out-of-tree, unmaintained quality) |
| Broadcom FullMAC (brcmfmac, e.g. 43430, 4350) | kernel + `firmware-brcm80211` | none |
| Broadcom BCM4311/4312/4321/4322/43224/43225/43227/43228/4331/4360/43142 (PCI `14e4:*`) | `broadcom-sta-dkms` (`wl`, non-free) via Driver Manager; `b43` + `firmware-b43-installer` (contrib, downloads) for older chips | table from `broadcom-sta` README.Debian, generated into hwdb **(verify the exact ID list and the 4360 `14e4:43a0` entry)** |
| Mediatek mt76 | kernel + `firmware-mediatek` | none |
| Qualcomm Atheros | ath9k/10k/11k + `firmware-atheros` | none |

**Decision.** `broadcom-sta` is shipped as a prebuilt signed module in `kiln-nonfree` for the current kernel ABI as well as DKMS, and appears on the offline payload (Wi-Fi is the one device class needed to get online, so it must work offline). Conflicts (`wl` vs `b43`, `bcma`, `ssb`, `brcmsmac`) are handled by the package's modprobe blacklist, listed in hwdb `conflicts`.

Work: broadcom-sta prebuilt pipeline reuses the Nvidia module farm, S after section 2; hwdb table M; Wi-Fi test on three machines M. Acceptance: BCM4360 or BCM4352 MacBook/PC card connects to a WPA2 network from the installed system after offline driver install, with Secure Boot on and enrolment done.

### 5.2 Bluetooth

`bluez` (main), `pipewire` BT audio profiles (A2DP, LC3 where available), firmware via `firmware-realtek` (`rtl_bt/*`), `firmware-atheros`, `firmware-intel-misc` (`intel/ibt-*`), `firmware-mediatek`. Combo Wi-Fi/BT cards share firmware packages already installed. Driver Manager shows a Bluetooth row with a state and a "dongle detected, no firmware" error mapped from dmesg (`Bluetooth: hci0: ... firmware ... failed`). Work S. Acceptance: pair a headset, play audio at 48 kHz A2DP; mouse reconnects after suspend.

### 5.3 Fingerprint

`fprintd`, `libpam-fprintd` (main). Standard drivers in `libfprint-2-2`. Vendor "TOD" blobs (Goodix, Broadcom, ELAN variants) come as separate non-free packages; availability in Debian trixie is partial **(verify list via `apt-cache search libfprint-2-tod`)**. Not on the ISO if licence prohibits (check per package copyright). hwdb entries by USB ID, for example Goodix `27c6:*`, Validity/Synaptics `06cb:*`, ELAN `04f3:*`, Broadcom `0a5c:5834`. Driver Manager offers "Install fingerprint support" and links to the Settings enrol page. PAM changes (`pam-auth-update`) are done by the package; Kiln never edits PAM files by hand. Work M (UI plus hwdb). Acceptance: enrol, unlock lock screen and `sudo` with a finger on a ThinkPad (Synaptics) and a Goodix laptop; password still works.

### 5.4 Printers and scanners

**Decision.** Prefer driverless: `cups`, `cups-browsed`, `ipp-usb`, `sane-airscan`, `libsane`, `avahi`. They cover most devices made since about 2015 and require no proprietary code. Vendor packages:

| Vendor | Package | Licence / ISO |
|---|---|---|
| HP | `hplip` (main, GPL) with `hp-plugin`, which downloads a proprietary plugin from HP after accepting an HP licence | plugin is not redistributable; never on the ISO; Driver Manager runs `hp-plugin -i` with a notice. Some printers need only the free part |
| Epson | `printer-driver-escpr` (Epson ESC/P-R, in Debian main) and `epson-inkjet-printer-escpr` variants | free or redistributable parts only on ISO |
| Epson scanners | iscan / Epson Scan 2 are vendor debs | not on ISO; the Driver Manager links to Epson's download page and shows `sane-airscan` as the first try |
| Brother, Canon, Samsung/HP-legacy | Debian packages where they exist (`printer-driver-brlaser`, `cnijfilter` is vendor-only) | link only, plus `printer-driver-all` meta on payload (free ones) |

Glaze Settings Printers page uses `cups` plus `system-config-printer` D-Bus API; the Driver Manager row only handles plugins the printer needs. Work: hplip plugin flow M; Epson flow S; hwdb rows by USB/IPP ID S. Acceptance: HP LaserJet (driverless) prints with no driver install; HP OfficeJet needing a plugin prompts for licence acceptance and then scans; Epson inkjet prints over USB and Wi-Fi.

### 5.5 Webcams

Standard UVC works. Hardware-specific: Intel IPU6 MIPI cameras (Lunar Lake / Alder Lake laptops) need `intel-ipu6` user-space stack and `v4l2-relayd`; not packaged in Debian trixie **(verify)**; track, do not promise. libcamera for Qualcomm/ARM boards belongs to the phone design doc. Driver Manager shows a "Camera not supported yet" row with a link rather than failing silently. Work: detection S; IPU6 packaging L, deferred. Acceptance: UVC webcam works in Kiln Cheese-equivalent app and WebRTC in Firefox.

### 5.6 Codecs and media

**Decision.** Debian's `ffmpeg`, `libavcodec`, `gstreamer1.0-libav`, `gstreamer1.0-plugins-{good,bad,ugly}` (main) cover MP3, AAC, H.264, HEVC, AV1 decode and encode in software, so there is no separate "codec pack" step, unlike Ubuntu's restricted-extras and Mint's multimedia codecs. Kiln installs these by default, same as Debian does. Patent exposure is the same as Debian's and is recorded in section 11 for legal review before a public release.
- `libdvdcss2` (needed for encrypted DVDs): never on the ISO or in Kiln's repo. Debian carries `libdvd-pkg` (contrib), whose installer downloads and builds libdvdcss from VideoLAN on the user's machine. Driver Manager offers "DVD playback" as an explicit action with a notice. Disabled entirely in the build variant for jurisdictions where distribution is a problem, via `kiln-hwdb` regional override file.
- Hardware decoding (VA-API/NVDEC): section 3.5.
- Fraunhofer AAC, Widevine (DRM in browsers): Widevine is a proprietary blob downloaded by the browser itself, so Kiln does nothing special beyond not blocking it.

Work: default package list S; DVD action S. Acceptance: play H.264/HEVC/AV1 files and a DVD with the opt-in action; fresh live session plays MP3, AAC and H.264 with no extra step.

## 6. Offline driver payload on the ISO

**Decision.** The ISO carries a local apt repository at `/kiln-drivers/` on the medium (visible at `/run/live/medium/kiln-drivers/` in live and mounted for Calamares). It contains every package the installer may need to install proprietary drivers without network. A flat repo with `Packages.gz`, `Release`, `Release.gpg` (Kiln key), generated at image build time by the `stage-kiln` pipeline from the real hwdb package set.

Contents (x86_64, sizes are estimates **(verify at first build)**):

| Group | Contents | Est. size |
|---|---|---|
| Firmware | all of section 4 | 500 to 700 MB if full `firmware-misc-nonfree`+`linux-firmware` set; 350 MB trimmed to hwdb-reachable |
| Nvidia | branch 580 userspace + open and closed prebuilt modules for the ISO kernel ABI + DKMS and headers for the ISO kernel + `nvidia-vaapi-driver` + `firmware-nvidia-gsp` | 350 to 450 MB |
| Broadcom | `broadcom-sta-dkms` + prebuilt module + `dkms` + headers + `b43` firmware installer | 40 MB |
| Microcode | `intel-microcode`, `amd64-microcode` | 10 MB |
| Intel media | `intel-media-va-driver-non-free` | 5 MB |
| Printing/scan free | `printer-driver-escpr`, `hplip` (not plugin), `sane-airscan` | 60 MB |
| Total | | about 1.0 to 1.3 GB extra |

ISO size policy: Kiln has two images. `kiln-<ver>-amd64.iso` (default, full payload, about 4 to 5 GB, USB only) and `kiln-<ver>-amd64-lite.iso` (open drivers and firmware only, no Nvidia/Broadcom payload; fetches the rest online). The payload can also be shipped as a separate downloadable `kiln-drivers-<ver>.sqfs` that the Driver Manager can use from a USB stick, for network-less machines installed from the lite image.

Rules:
- Payload is rebuilt every image build from the hwdb so that ISO and Driver Manager never disagree.
- Payload kernel ABI must equal the ISO kernel ABI. After the first `apt upgrade` the target uses the online repo or DKMS; stale prebuilt modules are not installed over a different kernel.
- Payload repo is used as `deb [signed-by=/usr/share/keyrings/kiln-archive-keyring.gpg trusted=no] file:/run/live/medium/kiln-drivers ./`; removed from target apt sources at the end of install by the Calamares job.

Files: `packaging/kilnlinux/image/config/hooks/live/` (payload step, new), `stage-kiln/build/build-all.sh` (add payload build and ISO layout), `kiln-drivers payload build` subcommand.

Work: payload builder M; ISO layout + lite variant M; size budgeting S; CI check that the payload satisfies `apt-get -s install` for each hwdb row offline in a clean chroot M. Risks: ISO size and licence compliance (section 11). Acceptance: install in a VM with the NIC removed, tick "Install proprietary drivers", reboot, `dpkg -l | grep nvidia` shows the expected packages on a machine with an Nvidia card (hardware test) and firmware packages on a VM (VM test).

## 7. Hardware detection and the mapping table

**Decision.** One data format, one Rust crate (`kiln-hwdb`), three consumers: the daemon, the Calamares module, CI. Debian already offers `isenkram` (AppStream `<provides><modalias>` matched against `/sys/**/modalias`) and `nvidia-detect`; Ubuntu `ubuntu-drivers` matches `Modaliases:` fields in package metadata. Kiln adopts the same primitive (modalias globs) so that it can interoperate with and import from those sources, but keeps its own TOML table because it needs more fields (licence class, preference, conflicts, kernel range, Secure Boot behaviour, test status).

Detection inputs, in order:
1. `/sys/bus/*/devices/*/modalias`: `pci:v000010DEd00002504sv*sd*bc03sc00i*`, `usb:v27C6p*`, `acpi:`, `of:` (arm).
2. `lspci -nn`-equivalent via the crate reading sysfs directly (no external tool needed).
3. DMI (`/sys/class/dmi/id/`) for laptop-specific quirks (hybrid, MUX).
4. CPU vendor and family from `/proc/cpuinfo` for microcode.
5. `mokutil --sb-state`, `systemd-detect-virt`, installed packages, loaded modules, `modinfo`.

Table format (`/usr/share/kiln-drivers/hwdb.d/*.toml`, signed with the rest of the data package `kiln-hwdb`):

```toml
[[driver]]
id = "nvidia-580-open"
class = "graphics"
match.modalias = ["pci:v000010DEd0000*sv*sd*bc03sc0*i*"]   # refined by id list below
match.pci_ids_file = "nvidia-580-open.ids"               # generated from supported-gpus.json
packages = ["nvidia-driver", "kiln-nvidia-modules-open", "firmware-nvidia-gsp", "nvidia-vaapi-driver", "kiln-nvidia-config"]
licence = "proprietary"          # free | firmware | proprietary | restricted
preference = "recommended"       # recommended | optional | experimental | legacy
alternative = "nouveau"          # free option shown next to it
conflicts = ["nvidia-580-closed", "nouveau"]
kernel_min = "6.12"
secure_boot = "needs-mok"        # none | needs-mok | needs-dkms-key
reboot = true
tested = ["rtx3060-desktop", "rtx4060-laptop-hybrid"]
```

Other rows follow the same schema: `firmware-*` by modalias, `broadcom-sta` by PCI ID list, `fprintd` vendor modules by USB ID, `hplip-plugin` by USB IDs or IPP make/model, microcode by CPU vendor.

Seeding the table:
- Nvidia PCI ID lists from `supported-gpus.json` per driver branch (CI).
- Firmware modalias to package from Debian `firmware-*` `Contents` and `modinfo` `firmware:` lines (CI, `kiln-drivers hwdb generate`).
- Broadcom from `broadcom-sta` README.Debian, manual review.
- AppStream modaliases collected from `isenkram` data where available.

CLI (also the machine interface for the UI and Calamares):

```
kiln-drivers detect [--json]       # devices, current driver, candidate rows
kiln-drivers plan [--json]         # what Install would do, size, reboot, MOK need
kiln-drivers apply <row-id>... [--offline] [--yes]
kiln-drivers remove <row-id>
kiln-drivers mok status|enrol
kiln-drivers doctor                # firmware errors, module signing, GPU mode
kiln-drivers hwdb generate|verify  # CI only
```

Daemon `kiln-driversd` on the system bus (polkit action `org.kiln.drivers.apply`) performs privileged work. It calls `PackageKit` for installs so the Software app and the update tooling share locks and progress. In the live and installer environment (no dbus-polkit session) the CLI calls `apt-get` directly.

Work:

| Task | Size |
|---|---|
| `kiln-hwdb` crate, TOML schema, matcher, unit tests with recorded sysfs fixtures | M |
| Generators (Nvidia ids, firmware map) | M |
| `kiln-drivers` CLI | M |
| `kiln-driversd` D-Bus daemon plus polkit | M |
| Fixture collection from real machines (sysfs tarballs) | S, ongoing |

Risks: stale or wrong IDs install the wrong driver. Mitigation: CI replays fixtures for every tested hardware class and every row has a `tested` list; rows without tests are `experimental` and never preselected. Acceptance: for each fixture in `tests/fixtures/<machine>/`, `kiln-drivers detect --json` output equals the golden file; unknown devices produce no install suggestion.

## 8. Kiln Driver Manager

### 8.1 Behaviour

- Native Glaze/GTK app in `/Users/xavi/fedeos/kiln-apps/kiln-drivers/` (GUI), running as the user and talking to `kiln-driversd`. Launched from the Start-equivalent menu "System > Driver Manager" and from Settings > Hardware.
- Modeled on the Mint Driver Manager (a front end over the Ubuntu driver tooling, with one section per device and a radio choice) and Windows Device Manager grouping, to suit the Windows-switcher goal: grouped device list, status icons, details pane, no jargon in the default view.
- Works offline from the payload (section 6) when no network. Shows the source being used ("Local drivers (USB)", "Kiln repository").
- Never installs anything automatically except microcode and free firmware. Proprietary drivers always need a visible user action or the installer checkbox.

### 8.2 UI

Single window, 760 x 540 minimum; mobile layout not needed (desktop tool). Phone shell (TilesDE) shows read-only status only.

1. Header banner: green "All hardware is using working drivers", amber "2 devices can use better drivers", red "Secure Boot key not enrolled: proprietary driver installed but not loaded".
2. Device groups (left or accordion): Graphics, Network, Bluetooth, Audio, Fingerprint, Printers and scanners, Camera, Firmware and microcode, Media.
3. Each device row: icon, device name (from hwdb or `ID_MODEL_FROM_DATABASE`), current driver and status. Expand to show choices as radio buttons:
   - "Open-source driver (nouveau/NVK)": Free, no extra install.
   - "NVIDIA 580 (open modules), recommended": Proprietary, tested on this model class.
   - "NVIDIA 580 (proprietary modules)".
   - "NVIDIA 595 (newer, less tested)": Experimental, hidden unless "Show newer drivers" is on.
   - "Use DKMS (build on this PC)": under Advanced.
4. Badges: Free, Proprietary, Firmware, Experimental, Tested. Licence link per row.
5. "Details" button opens a side sheet: packages, download size, installed size, licence text, kernel compatibility, source repository, Secure Boot behaviour.
6. Footer: "Apply Changes" (disabled until a choice differs), "Revert to open drivers", progress, log viewer.
7. Secure Boot flow dialog (section 3.2) appears on Apply when needed.
8. Advanced tab: GPU mode (Hybrid, Discrete, Integrated), kernel list with the one gated by driver status, DKMS status, "Rebuild modules", "Export diagnostics" (writes `kiln-drivers doctor` output to a file; does not upload).
9. Reboot-required state is shown as a persistent footer with "Restart now" and "Later".

### 8.3 First-run and hardware change notifier

`kiln-drivers-notify` (user service, runs at login, no root): compares the hash of the detected hardware set with `/var/lib/kiln-drivers/seen`. On change shows one Glaze notification "New hardware needs a driver: Wi-Fi adapter". Click opens the Driver Manager at that row. Nothing popup-spams on a machine with nothing to offer. Also hooks into the Kiln Welcome app (separate design) as the "Drivers" step. Respects a "Don't ask again for this device" entry in `~/.config/kiln/drivers.toml`.

### 8.4 Safety net

- `kiln-gpu-guard.service` (system): the Glaze session writes `/run/kiln/session-ok` after the first frame; the guard counts failed boots (no marker within 60 s after `graphical.target`) in `/var/lib/kiln-drivers/boot-count`. After 2 consecutive failures the next boot sets `kiln.gpu=open` (blacklists `nvidia*`, forces nouveau/modesetting) and a notification explains "Proprietary graphics driver was disabled automatically", opening Driver Manager on Graphics.
- GRUB menu (installed): "Kiln", "Kiln (open drivers only)" with `modprobe.blacklist=nvidia,nvidia_drm,nvidia_modeset,nvidia_uvm`, "Kiln (safe graphics)" with `nomodeset`, and the usual recovery entry. Entries are written by `/etc/grub.d/42_kiln_gpu` which is part of `kiln-gpu-guard`.
- Every Apply creates a transaction log `/var/log/kiln-drivers/<ts>.json` with previous package versions, so "Revert" can restore. For graphics changes it also snapshots `/etc/modprobe.d/kiln-*` and the initramfs.
- Removal path: `kiln-drivers remove nvidia` purges Nvidia packages, removes blacklist, runs `update-initramfs -u`, switches to nouveau.

### 8.5 Work

| Task | Size |
|---|---|
| UI shell, device list, details sheet | L |
| Apply/progress/log through `kiln-driversd` | M |
| Secure Boot enrol dialog | M |
| GPU mode page | M |
| First-run notifier | S |
| `kiln-gpu-guard`, GRUB entries | M |
| Strings and a11y pass | S |

Risks: UI promises "recommended" for a combination we haven't tested. Mitigation: only `tested` rows may be `recommended`.

Acceptance: on a machine with an Nvidia GPU, a user switches from nouveau to Nvidia 580 open entirely through the UI, reboots, and `glaze status` shows the Nvidia renderer; "Revert to open drivers" returns to nouveau and the session still starts. Simulated guard test: break the Nvidia module (`mv nvidia.ko`) and confirm automatic fallback to the desktop on the second boot.

## 9. Calamares integration

**Decision.** Add a "Drivers" step between Partition and Users in the Calamares sequence, plus install-time jobs, as a custom module `kilndrivers` (QML view + Python job).

Page (QML, same look as Glaze):
- Title: "Hardware support". Text: "Install proprietary drivers and firmware found for this computer".
- Checkbox "Install proprietary drivers and firmware", default checked when the plan contains any row with `preference=recommended` and `licence` proprietary or firmware, unchecked when only free items, always unchecked and hidden for virtual machines with nothing to install.
- Below the checkbox, live list from `kiln-drivers plan --json` for this machine, for example: "NVIDIA GeForce RTX 4060 Laptop: NVIDIA 580 (open modules), 410 MB", "Broadcom BCM4360 Wi-Fi: broadcom-sta, 3 MB", "Intel microcode". Each item has its own checkbox; Nvidia row has an alternative "Use open-source driver (nouveau/NVK)".
- Secure Boot info panel when `mokutil --sb-state` says enabled and the plan contains a module that needs MOK: "Your computer uses Secure Boot. You must enrol a key on the first restart" and fields for a one-time password (confirm field). The password is stored only in Calamares globalstorage, written to a temp file in the target with mode 0600, and removed after `mokutil --import`.
- A short notice and licence links: "These drivers are proprietary and are licensed by their vendors. Kiln only redistributes them under those licences."
- If no network and the plan item is not in the offline payload, show it greyed with "Needs network".

Jobs (exec order, all after `unpackfs` and before `bootloader`/`grubcfg`):
1. `kilndrivers-prepare`: bind-mount `/run/live/medium/kiln-drivers` into the target at `/run/kiln-payload`, write `/etc/apt/sources.list.d/kiln-payload.list`; bind `/sys/firmware/efi/efivars` if EFI.
2. `kilndrivers-install` (in chroot): `kiln-drivers apply <plan> --offline --yes` using `apt-get install` with the payload as the only source. The job reuses the Calamares `packages` module semantics for progress and error reporting but with its own runner since it needs hwdb logic.
3. `kilndrivers-mok` (chroot): `mokutil --import` using the password file.
4. `kilndrivers-finish`: remove payload source and mount, run `update-initramfs -u -k all`, enable services; ensure `kiln-gpu-guard` is enabled. Failure in any step is non-fatal to the install: the job logs, the installer shows "Some drivers could not be installed; you can retry from Driver Manager" and the system boots with open drivers.
5. Bootloader step must run after drivers so that `grub-mkconfig` sees Nvidia kernel parameters and the extra GRUB entries (`42_kiln_gpu`).

Files: `packaging/kilnlinux/installer/modules/kilndrivers/{module.desc,kilndrivers.conf,main.qml,kilndrivers_job.py}` (new, next to the Calamares config being added), `settings.conf` sequence updated.

Work:

| Task | Size |
|---|---|
| QML page + hwdb plan binding | M |
| Python jobs (prepare, install, MOK, finish) | M |
| Error handling, retry, log capture to `/var/log/kiln-install-drivers.log` | S |
| VM test with a fake `nvidia` row (tiny stub package) so CI can run without a real GPU | M |
| Test on real EFI Secure Boot machine | M |

Risks: Calamares QML module API differs by version (Debian trixie Calamares 3.3.x **(verify)**); efivarfs not mounted in chroot; `mokutil` import failing on firmware without MOK support. Mitigation: skip MOK on error and show retry in Driver Manager.

Acceptance: install in QEMU/OVMF with Secure Boot on and a stub driver row: checkbox on, installation completes offline, first boot shows MokManager, after enrolment the stub module loads (`modinfo` signer is Kiln); checkbox off: no payload packages installed except microcode and free firmware. Real hardware: Nvidia RTX laptop installs and boots into Glaze on Nvidia.

## 10. Live-session handling

**Decision.** Live defaults to open drivers everywhere. Rationale: the live image must boot on everything with zero risk, MOK enrolment from a live session persists in the firmware of a machine the user has not committed to, and prebuilt proprietary modules cannot load under Secure Boot without enrolment. Nvidia Turing+ with nouveau uses GSP firmware (`firmware-nvidia-gsp`) and NVK in Mesa, which is adequate for installing.

Boot menu (BIOS syslinux and UEFI GRUB, from `live-build` `binary_*` templates in `packaging/kilnlinux/image/config/bootloaders/`):

| Entry | Kernel parameters |
|---|---|
| Kiln Live (default) | `boot=live quiet splash` |
| Kiln Live (proprietary Nvidia graphics) | `... kiln.nvidia=1 modprobe.blacklist=nouveau nvidia-drm.modeset=1`. Shown always, but `kiln-live-gpu.service` refuses and shows a message if Secure Boot is on (module cannot load) or no Nvidia GPU exists |
| Kiln Live (safe graphics) | `... nomodeset kiln.renderer=pixman`: Glaze/Sway with software renderer (`WLR_RENDERER=pixman`, `LIBGL_ALWAYS_SOFTWARE=1`) |
| Kiln Live (open drivers, no firmware) | `... firmware=never`-style param handled by Kiln, for machines where firmware loading hangs; also helps testing |
| Install Kiln | same as default, auto-launches the installer |
| Memory test, firmware setup | UEFI only |

Runtime behaviour:
- `kiln-live-gpu.service` (early, before the display session): runs `kiln-drivers detect`; if `kiln.nvidia=1` and `mokutil --sb-state` is disabled, runs `modprobe nvidia-drm` from the signed prebuilt module in the payload kernel ABI; on failure leaves nouveau.
- Auto fallback: `glaze-session` is supervised; if the compositor exits within 15 s twice, the session retries with `renderer=pixman`, writes `/run/kiln/gpu-fallback`, and shows a notice "Using safe graphics" on the second try. Same guard path as 8.4 so the code is shared.
- The default boot parameters never contain `nomodeset` (it disables KMS and makes Wayland unusable); it exists only in the safe entry.
- Persistence: none; the live session runs entirely from squashfs and the overlay.

Files: `packaging/kilnlinux/image/config/bootloaders/`, `config/includes.chroot/etc/systemd/system/kiln-live-gpu.service`, `glaze-session` supervisor change.

Work: boot entries M; `kiln-live-gpu.service` S; supervisor fallback M; tests across GPU classes M. Risks: some machines hang at KMS handoff with nouveau on very new Nvidia; mitigated by the safe entry and an on-screen hint in the boot menu. Acceptance: on the test rig (section 13), each entry boots to the live desktop; with `nvidia` GPU and Secure Boot off, the proprietary entry shows `nvidia-smi` working; with Secure Boot on it prints the explanation and still boots open drivers.

## 11. Kernel updates

**Decision.**
- Kiln tracks trixie-backports kernel via `kiln-kernel`. Plain `linux-image-amd64` is not installed by default so Debian's meta does not race past Kiln's gate.
- `kiln-kernel` depends on `linux-image-<abi>` and `kiln-nvidia-modules-*` of the same ABI. CI (section 3.6) publishes a new `kiln-kernel` version only when drivers are ready for that ABI. The Update Manager (separate design) shows the kernel as an update only when `kiln-kernel` moves.
- Security-only kernel bumps on the same ABI (`linux-image-<abi>` with a new Debian revision) flow through immediately, because module packages `Depends: linux-image-<abi>` and ABI is constant across revisions in the same upstream version.
- The previous two kernel ABIs stay installed (Debian's default `apt` autoremove retains the running and previous kernel; Kiln config forces `APT::Install-Recommends` and `APT::NeverAutoRemove` for the running plus one previous).
- DKMS fallback rebuilds in `postinst` of the kernel via the standard `/etc/kernel/postinst.d/dkms`. Failed DKMS builds make `kiln-kernel` upgrade fail early with a clear message (`dkms` `autoinstall` return code check in `kiln-kernel` `preinst`), leaving the old kernel in place, instead of a boot with no graphics.
- Secure Boot: the modules' signatures are independent of the kernel revision; the Kiln cert stays valid until rotation; rotation is announced in the Update Manager and requires another MOK enrolment dialog, optional and not forced until expiry.
- Kernel rollbacks: GRUB lists previous kernels; modules for them stay installed.
- Initramfs: `nvidia` modules are not added to the initramfs (not needed for KMS handoff with simpledrm fallback). `nouveau` blacklisting requires the initramfs rebuild, which `kiln-nvidia-config` does in `postinst`.

Work: `kiln-kernel` metapackage and CI gate M (shared with 3.6); preinst DKMS check S; Update Manager integration S. Risks: Debian backports kernel ABI moves frequently (7.x series bumps), each needing a module rebuild; the CI gate may delay kernel updates, which are security fixes. Mitigation: lag target documented (24 hours for rebuilds), and a one-click "install latest kernel anyway (open drivers only)" in Advanced. Acceptance: a simulated new ABI with missing modules is not offered by apt; on publication it installs and `uname -r` plus `modinfo nvidia` match after reboot; with a failing DKMS build, the old kernel remains the default.

## 12. Phone (TilesDE) and convergence notes

- The phone image carries no proprietary GPU driver stack. Per-device firmware (Wi-Fi, modem, GPU microcode, DSP) lives in device packages owned by the device port (postmarketOS/Mobian practice: a `firmware-<device>` package, often sourced from the device's vendor partition). It is outside this document except for these requirements: the Driver Manager shows firmware status read-only on TilesDE and `kiln-hwdb` supports `of:` modaliases.
- Docking to a monitor and starting Glaze on a phone SoC uses open GPU drivers (Mesa freedreno, panfrost, etc.), so no driver switching happens on dock. If a particular SoC needs a vendor blob, it is a device-port item, not generic Kiln.
- Hybrid GPU modes (section 3.4) never appear on the phone.

## 13. Legal and redistribution constraints

This is engineering guidance, not legal advice. A lawyer should review items marked (L) before a public release.

| Item | Position | ISO allowed |
|---|---|---|
| Debian `non-free-firmware` packages | Debian's installer and live images include them since Debian 12 under the 2022 vote; each package carries its licence text under `/usr/share/doc/<pkg>/copyright`. Firmware blobs may be redistributed unmodified; Kiln must keep copyright files and `WHENCE`, and must not repack or patch binaries | Yes, with licence texts, and a "Licences" screen in the installer/Welcome |
| `non-free` packages (nvidia-driver, broadcom-sta) | Redistributable under their vendor licences in Debian's archive; terms differ per package (NVIDIA licence requires the licence file to travel with the software, no reverse engineering). Kiln rebuilds NVIDIA from upstream tarballs and must read the current NVIDIA licence (L) | Yes after review; keep licence files in packages and in `kiln-licences` |
| NVIDIA open kernel modules | Dual MIT/GPL, free; GSP firmware is separate non-free | Yes |
| Kiln-signed modules | Signed with Kiln's key. Signing someone else's closed-source module is a repackaging step to review (L); we do not modify binaries other than adding a signature | Yes |
| `libdvdcss` | CSS circumvention. US DMCA section 1201, EU InfoSoc Art. 6 implementations. Debian does not host it; `libdvd-pkg` downloads from VideoLAN at user request | No. Opt-in only, user-initiated, with notice. Region switch to disable |
| Patent-encumbered codecs (H.264, HEVC, AAC, AC-3...) | Debian's ffmpeg ships them in main; patent licensing is a risk in some countries. Fedora withholds, Mint and Ubuntu ship them behind an installer checkbox | Matches Debian; Kiln owner must accept this risk or switch to a "codecs on demand" flow (L). hwdb marks them `restricted` so the behaviour can be flipped without code changes |
| HPLIP plugin | Proprietary HP licence, user must accept at download | No; downloaded by user action |
| Epson Scan 2 / iscan, other vendor debs | Vendor terms, not redistributable without agreement | No; link only |
| Widevine | Browser downloads it | Not Kiln's concern |
| Trademarks | "NVIDIA", "Broadcom", etc. are used nominatively in UI. Debian trademark: Kiln must not imply it is Debian, but "based on Debian" is fine. Debian's signed binaries keep Debian names | n/a |
| Secure Boot | Kiln does not hold a Microsoft-signed shim; it depends on Debian's | n/a |

ISO notice requirements: bundle `/usr/share/doc` copyright files (already present), add a top-level `LICENSES/` on the medium listing every non-free component, version and source URL, and publish corresponding source for GPL parts (Debian source packages, plus Kiln's rebuilt sources for the `kiln-nonfree` component) with a written offer or hosted sources.

Work: `LICENSES/` generator from `dpkg-query` and the payload M; legal review S for the owner (external); region override file S. Acceptance: for every file in the payload, `LICENSES/index.md` has a licence and source link (CI fails otherwise).

## 14. Driver matrix

Legend: OK = in tree or Debian main works; PB = Kiln prebuilt signed module; DK = DKMS fallback; FW = firmware package; ? = unknown until tested on hardware.

| Device class | Examples | Default driver | Proprietary option | Packages | Secure Boot | Notes |
|---|---|---|---|---|---|---|
| Nvidia Blackwell | RTX 50xx | nouveau/NVK **(verify NVK coverage)** or Nvidia open | Nvidia latest branch only (open modules required) | nvidia-open, GSP FW | MOK | closed modules unsupported by hardware |
| Nvidia Ada/Ampere/Turing | RTX 40/30/20, GTX 16 | NVK | Nvidia 580 open (recommended) | PB open | MOK | explicit sync, NVDEC, PRIME |
| Nvidia Volta/Pascal/Maxwell | GTX 10, 9xx, Titan V | nouveau (no reclocking, slow) | Nvidia 580 closed | PB closed | MOK | last branch, no newer |
| Nvidia Kepler and older | GTX 6xx/7xx | nouveau | none | none | none | legacy branches dropped |
| AMD GPU | RX 5000 to 9000, APUs | amdgpu + radeonsi + RADV | none needed | `firmware-amd-graphics` | none | VA-API via mesa |
| Intel GPU | UHD, Iris Xe, Arc | i915 or xe + iris/anv | media non-free HuC | `firmware-intel-graphics`, `intel-media-va-driver(-non-free)` | none | |
| Hybrid laptop | Intel+Nvidia, AMD+Nvidia | iGPU | Nvidia offload | switcheroo-control | MOK | 3 modes |
| Wi-Fi Intel | AX200/AX210/BE200 | iwlwifi | none | `firmware-iwlwifi` | none | |
| Wi-Fi Realtek | 8821CE, 8822CE, 8852AE | rtw88/rtw89 | rtl8821ce-dkms (exp) | `firmware-realtek` | DK key | in-tree first |
| Wi-Fi Broadcom old | BCM4360, 4352, 4331, 43142 | none or b43 | broadcom-sta (wl) | `broadcom-sta-dkms`+PB | MOK | offline payload required |
| Wi-Fi Broadcom FullMAC | Macs, Pi-like | brcmfmac | none | `firmware-brcm80211` | none | |
| Wi-Fi Mediatek | mt7921/7922 | mt76 | none | `firmware-mediatek` | none | |
| Bluetooth | Realtek, Intel, Atheros | btusb + bluez | none | firmware packages | none | |
| Fingerprint | Synaptics, Goodix, ELAN | libfprint | vendor TOD where available | `fprintd`, `libfprint-2-tod*` | none | ? per model |
| Printers | HP, Epson, Brother, Canon | IPP everywhere | HP plugin, escpr | `cups`, `hplip`, `printer-driver-escpr` | none | |
| Scanners | Epson, HP | sane-airscan, sane | iscan (vendor) | `sane-airscan`, `libsane` | none | |
| Webcam | UVC | uvcvideo | none | `v4l-utils` | none | IPU6 not supported |
| Audio | Intel SOF, HDA | snd_* | SOF FW | `firmware-sof-signed` | none | |
| CPU microcode | Intel, AMD | microcode | n/a | `intel-microcode`, `amd64-microcode` | none | security |
| Media codecs | AAC, H.264, HEVC, AV1 | ffmpeg in main | libdvdcss opt-in | `ffmpeg`, `gstreamer1.0-*` | none | legal (L) |

## 15. Test plan: real hardware classes

Rule from repo: never claim validation without running on real hardware. Each class below needs a machine, an owner and a log directory `docs/hw-tests/<date>-<machine>.md` containing `kiln-drivers detect --json`, `dmesg`, `glaze status`, `swaymsg -t get_outputs` or `wlr-randr`, screenshots (`grim`), Secure Boot state.

| Class | Representative machines to acquire or borrow | Tests |
|---|---|---|
| Nvidia desktop, Turing+ | RTX 3060 desktop + monitor on dGPU | live nouveau boot; installer with drivers; Glaze + Sway; explicit sync; NVDEC; suspend/resume x10; kernel update gate |
| Nvidia laptop hybrid, Intel+Nvidia | Ampere/Ada laptop (MUX optional) | Hybrid/Discrete/Integrated modes, runtime PM suspend, external HDMI on dGPU, offload launch, battery drain idle |
| Nvidia Pascal | GTX 1050/1060 laptop or desktop | closed module path, 580 explicit sync regression check with `GLAZE_DISABLE_EXPLICIT_SYNC` |
| Nvidia Kepler | GTX 650-era card | nouveau only, offered no proprietary row |
| AMD APU + dGPU | Ryzen laptop, RX 6000 desktop | amdgpu firmware, VA-API, no extra drivers needed |
| Intel integrated, Gen9 to Xe | ThinkPad, Intel Arc A380 | i915/xe, HuC, VA-API |
| Secure Boot variants | OVMF (QEMU), one AMI and one Insyde laptop | MOK flow from installer, from Driver Manager, key already enrolled, user skips MokManager (guard fallback), cert rotation |
| Secure Boot off | same machines | live proprietary entry, DKMS fallback |
| Wi-Fi Broadcom | MacBook Air 2013 to 2015 or a BCM4360 PCIe card | offline install, connect after reboot, SB on |
| Wi-Fi Realtek | laptop with 8821CE or 8822CE, USB 8812AU | in-tree first, DKMS option |
| Bluetooth | Intel AX200 combo, Realtek USB dongle | pair, audio, suspend/resume |
| Fingerprint | ThinkPad (Synaptics), Goodix laptop | enrol, lock screen, sudo |
| Printers/scanners | HP LaserJet (driverless), HP OfficeJet (plugin), Epson EcoTank | print, scan, plugin licence flow |
| Webcam | UVC USB, IPU6 laptop | UVC works, IPU6 shows "not supported" row |
| Offline install | any x86 machine, NIC removed or air-gapped | payload install, Calamares checkbox, first boot has working Wi-Fi where applicable |
| VM (smoke only) | QEMU/HVF virtio-gpu (current VM) and QEMU/OVMF x86_64 | Calamares jobs with a stub driver package, MOK mechanics with OVMF, no claims about Nvidia |

Automated tests (CI, no GPU): hwdb fixture replays; payload completeness (`apt-get -s install` offline in a chroot); licence index; boot guard simulation; Calamares job dry run; module signature check on payload (`modinfo -F signer`, `kmodsign --verify`).

## 16. Phased plan

| Phase | Deliverable | Size |
|---|---|---|
| P0 | Firmware and microcode in image package list; `non-free-firmware` hygiene; broadcom-sta DKMS available in live image; licence index skeleton | S |
| P1 | `kiln-hwdb` + `kiln-drivers detect/plan/apply` CLI; Driver Manager read-only view | M |
| P2 | Kiln repo + Nvidia 580 packaging + one-kernel prebuilt modules, signed; MOK flow; Driver Manager apply | L |
| P3 | Offline payload on ISO, Calamares `kilndrivers` module, boot menu entries, guard | L |
| P4 | Glaze compositor Nvidia work (primary GPU, modifiers, explicit sync, PRIME), GPU mode page | L |
| P5 | Kernel gate, CI on real GPUs, newest-branch option, hplip/fingerprint/printer rows | L |
| P6 | Legal review, region override, release gate | S (owner work) |

Ordering rationale: P0 to P3 deliver "install works offline with drivers" which the owner asked for; P4 is the biggest unknown and runs in parallel with Sway as the fallback; P5 turns it into a sustained process.

## 17. Open questions for the owner

1. Accept Debian-parity patent risk for codecs (section 13), or a codecs-on-demand flow?
2. Single Kiln MOK key (convenient, central risk) versus per-machine DKMS keys only (safer, slower)? This document picks single key with DKMS option.
3. Who hosts the Kiln repo and the signing key, and where does the offline key live?
4. Can hardware (an Nvidia desktop card and a hybrid laptop) be provided for CI? Without it, section 3 is unverifiable.
5. x86_64 first for drivers while arm64 stays the primary target elsewhere: confirm.
