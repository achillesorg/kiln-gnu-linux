# Kiln fresh-software strategy

Status: design, 2026-11-18. Nothing here is built or run yet. Not committed.
Scope: how Kiln GNU/Linux (Debian 13 "trixie" base) ships software newer than Debian stable without breaking the base.

Verified data in section 2 was read from tracker.debian.org, backports.debian.org, kernel.org, firefox.com and developers.cloudflare.com on 2026-11-18. Items marked (unverified) came from secondary sources or memory and must be re-checked before they are acted on.

## 0. Principles

1. Debian stable plus its security suite is the trusted base. Kiln never replaces core ABI (glibc, systemd, gtk4, qt6, apt, dpkg).
2. Source order for anything newer: (a) trixie-backports, (b) a Kiln rebuild of a forky/sid source in the Kiln repo, (c) Flatpak, (d) the vendor's own signed apt repo (Mozilla). Never `curl | sh`, never unsigned binaries, never mixing raw sid packages.
3. Kiln-built packages carry a version suffix that sorts below the Debian version that will eventually replace them (`~kiln13u1`), so Debian 14 upgrades cleanly.
4. Every risky update is preceded by a filesystem snapshot (section 10). A previous kernel always stays installed and bootable.
5. Fast-moving user apps live in Flatpak. Hardware stack (kernel, Mesa, Nvidia, firmware, pipewire) lives in deb, because it must match the host.

## 1. Channels

### Decision
Two channels, selected by one metapackage, plus an always-on base lane.

| Channel | Default for | Content |
|---|---|---|
| `stable` | servers, Kiln on old hardware, user opt-in | Debian 13 + stable-security + `kiln-stable` repo (Glaze, kiln-apps, branding, driver packages on the conservative branch). Kernel 6.12 LTS from Debian. |
| `fresh` | desktop ISO (amd64 and arm64) | everything in `stable` plus the "fresh set": backports kernel, Mesa, LibreOffice, pipewire, plus Kiln rebuilds (Nvidia production branch, wlroots/sway, flatpak). |

The base lane (Debian stable-security) is identical in both and never pinned away.
Phone images (TilesDE) use `stable` with a device-specific kernel; phone kernel policy is out of scope here (Mobian/postmarketOS style per-device kernels).

### Rationale
Owner wants newer software and working proprietary drivers. New GPUs need new Mesa and kernel (Debian 13 ships Mesa 25.0.7 and kernel 6.12, Nvidia 550). Userland libraries stay stable because Glaze and TilesDE target one ABI. Hardware-vs-app split limits blast radius: kernel/Mesa/Nvidia can be rolled back as a set; Flatpak apps do not touch the host.

### Components and files
- `kiln-channel-stable`, `kiln-channel-fresh` (conflict with each other). Each ships `/etc/apt/sources.list.d/kiln.sources` (deb822), `/etc/apt/preferences.d/90-kiln-channel`, and `/usr/share/kiln/fresh-set.list` (explicit package names covered by the channel).
- `kiln-archive-keyring` ships `/usr/share/keyrings/kiln-archive-keyring.pgp`.
- `kiln-channel` CLI (shell or Rust, owned by the Update Manager design): `kiln-channel status|switch fresh|stable`. Used by Update Manager GUI.
- Switch fresh to stable: only packages named in `fresh-set.list` are downgraded, using a temporary pin priority 1001 on the Debian suite. The tool snapshots first and refuses if the running kernel is a backports kernel (reboot into the LTS kernel first).

### Work breakdown
- Channel metapackages and pin files: S
- `kiln-channel switch` incl. downgrade path and dry-run: M
- Update Manager UI toggle (cross-doc): S

### Risks
- Downgrade fresh to stable is the weakest path (Mesa 26 to 25 is OK in practice, Nvidia 595 to 580 changes kernel modules). Mitigation: declare it "supported with snapshot restore as the fallback", not magic.
- Two channels double test time. Mitigation: CI installs both in QEMU for every release.

### Acceptance tests
- Fresh install of channel X resolves `apt policy mesa-vulkan-drivers linux-image-amd64` to the expected origin.
- `apt full-upgrade --simulate` on fresh with only Debian + kiln-stable removed gives no removals of kiln-* packages.
- Switch fresh to stable in a VM, reboot, `glaze status` OK.

## 2. Versions today: Debian 13 versus what Kiln ships

Debian columns: "stable" is what a trixie install gets (including stable-security). "bpo" is trixie-backports. "sid" is unstable. Upstream from kernel.org/Mozilla/NVIDIA/Debian tracker notes.

| # | Package | Debian 13 stable | Newer Debian source today | Upstream today | Kiln channel and source |
|---|---|---|---|---|---|
| 1 | linux kernel | 6.12.117-1 (security) | bpo 7.3.4-1~bpo13+1 | 7.3.5 stable, 6.18.64 and 6.12.118 LTS, 7.4-rc3 | fresh: backports 7.3.x. stable: Debian 6.12 LTS. Keep LTS kernel installed in fresh too (rescue). |
| 2 | mesa | 25.0.7-2+deb13u1 | bpo 26.1.6-1~bpo13+1; sid 26.2.6-1 | 26.2.x | fresh: backports 26.1.6 now; Kiln rebuild of 26.2.x once its build-deps verify on trixie. stable: Debian 25.0.7. |
| 3 | Nvidia proprietary driver | 550.163.01-2 (also bpo and sid: still 550.163.01) | none newer in Debian | production 595.112.07, feature 615.78.08 (unverified, secondary press), 580.x for Maxwell/Pascal/Volta (unverified) | both: Kiln-built `kiln-nvidia-595` (fresh) and `kiln-nvidia-580` (stable and legacy GPUs). Debian's 550 is too old for new GPUs and for kernel 7.x. See 8.3. |
| 4 | Firefox | firefox-esr 153.7.0esr-1~deb13u1 (security) | testing 153.5.1esr | Firefox 160.0.1 release, ESR 153.7.0 | both: Mozilla's signed apt repo (release), pinned 1000 over Debian firefox-esr (already in hook 0100). Debian ESR remains as fallback. Verify arm64 availability on that repo. |
| 5 | Thunderbird | 140.12.0esr in base, 153.7.0esr via security | sid 153.7.0esr-2 | release branch is newer than 153 (unverified exact) | both: Debian security deb. Flathub release build available to users. |
| 6 | LibreOffice | 4:25.2.3-2+deb13u6 | bpo 4:26.8.2.2-2~bpo13+1; sid 4:26.8.3.1-3 | 26.8.x | fresh: backports. stable: Debian 25.2. |
| 7 | pipewire | 1.4.2-1 | bpo 1.6.9-2~bpo13+1 | 1.6.11 | fresh: backports. stable: Debian. |
| 8 | sway | 1.10.1-2 | testing 1.12-1; sid 1.12-2 | 1.12 | fresh: Kiln rebuild 1.12 (fallback compositor for Glaze). stable: Debian. |
| 9 | wlroots | 0.18.2-3 (open CVE: out-of-bounds read, fixed in 0.20.2) | sid 0.20.2-1 | 0.20.2 | fresh: Kiln rebuild `libwlroots-0.20` (versioned package name, installs beside 0.18). stable: Debian plus CVE patch. |
| 10 | flatpak | 1.16.6-1~deb13u3 | sid 1.18.4-1 | 1.18.x | fresh: Kiln rebuild 1.18. stable: Debian. Sandbox-escape CVEs are frequent: track closely. |
| 11 | gtk4 | 4.18.6+ds-2 | sid 4.24.1+ds-1 | 4.24.x | neither: not backported. Glaze apps must build and run on 4.18. Newer GTK reaches apps only through Flatpak runtimes. |
| 12 | qt6-base | 6.8.2+dfsg-9+deb13u2 (6.8 LTS) | sid 6.11.2+dfsg-8 (bpo has only qt6-webengine 6.10.2) | 6.12.0 | neither: TilesDE targets Qt 6.8 LTS. Revisit only if QML features are needed. |
| 13 | systemd | 257.13-1~deb13u1 | sid 262.2-1 | 262.x | neither: core, never backported by Kiln. |
| 14 | rustc/cargo | 1.85.0+dfsg3-1 | bpo 1.96.1+dfsg1-2~bpo13+1; sid 1.98.1 | 1.98.x | build chroot only (fresh toolchain for Glaze CI). Not installed on user systems by default. |
| 15 | Chromium | 150.0.7871.100 base, 156.0.8078.79 via security | sid 156.0.8078.79 | 158.0.8127.36 | both: Debian security deb. Users wanting newest: Flatpak. |

Also noted: git is 1:2.47.3 in trixie versus 2.55.0 in sid; no action.

Observations that drive the design:
- Debian's Nvidia packaging is stalled at 550.163.01 in every suite. Kiln must package Nvidia itself.
- Backports track forky, so the backports kernel is a non-LTS rolling kernel (7.2 already EOL upstream on 2026-11-01). It only stays safe because Debian rebases it. Therefore the LTS kernel stays installed as fallback.
- Debian security already moves Firefox, Thunderbird, Chromium to current majors. Do not duplicate that work.

### Work breakdown
- Automated "version drift" report (script over tracker.debian.org JSON or `rmadison`), published weekly: S
- Maintain this table as `docs/design/fresh-software.md` appendix or generated file: S

### Acceptance test
- CI job prints the table for each release and fails if any `fresh-set.list` package is older than Debian stable.

## 3. apt source order and pinning

### Decision
Priority bands (higher wins). Debian default is 500.

| Priority | Origin | Rule |
|---|---|---|
| 1000 | `packages.mozilla.org` (Firefox only) | `Package: firefox*` pin, also blocks Debian firefox-esr from replacing it |
| 990 | `kiln-stable` repo (`o=Kiln,c=stable`) | all packages named `kiln-*`, `glaze*`, `tilesde*` only |
| 600 | `kiln-fresh` repo (`o=Kiln,c=fresh`) | channel fresh only, only names in `fresh-set.list` |
| 600 | `trixie-backports` | channel fresh only, explicit `Package:` list (kernel, mesa, libreoffice, pipewire) |
| 500 | Debian trixie and trixie-security | default |
| 100 | `trixie-backports` | the default for any package not in the explicit list (apt already gives backports 100 via NotAutomatic and ButAutomaticUpgrades) |
| -1 | `forky`, `sid` | never configured on user systems |

Example `/etc/apt/preferences.d/90-kiln-channel` (fresh):
```
Package: linux-image-amd64 linux-image-arm64 linux-headers-amd64 linux-headers-arm64 mesa-* libgl1-mesa-dri libgbm1 libegl-mesa0 libglx-mesa0 libreoffice* pipewire* libpipewire-* wireplumber
Pin: release n=trixie-backports
Pin-Priority: 600

Package: *
Pin: release o=Kiln,c=fresh
Pin-Priority: 600
```
`apt` does not allow `Package: *` plus restriction by list, so the generated file enumerates `fresh-set.list` explicitly. A test asserts that file and list agree.

Package version rule for Kiln rebuilds: `<debian-version>~kiln13u<N>` (example `26.2.6-1~kiln13u1`). Never use epochs.
Fresh-set additions must also bring their reverse dependencies' versions (example: mesa and libgbm1, wlroots and sway); pins are by source package glob via a generated list, not by hand.

### Rationale
Pin by explicit package list, not by suite, because pinning a whole suite at 600 pulls unwanted upgrades and partial upgrades. Backports at 100 is Debian's own safe default.

### Components and files
`packaging/kilnlinux/apt/` (new): templates for `kiln.sources`, preferences, `fresh-set.list`, generator script `gen-pins.sh`.
Live-build: add to `packaging/kilnlinux/image/config/includes.chroot/etc/apt/` and a package-list entry for `kiln-channel-fresh`.

### Work breakdown
- Pin generator and tests (`apt-cache policy` assertions in a container): M
- Add to live-build config and hook ordering relative to Mozilla hook 0100: S

### Risks
- A backported library pulled in by pin but with mismatched reverse deps (libgbm1 vs mesa) breaks login. Mitigation: pin globs by source package, run `apt-get -s dist-upgrade` in CI and require zero "kept back" for fresh-set.
- Backports are NotAutomatic: security fixes in backports arrive late. Mitigation: Update Manager shows channel provenance; fresh set stays small.

### Acceptance tests
- `apt policy` golden-file test per channel in CI (amd64 and arm64 containers).
- Simulated `apt full-upgrade` does not install any package from `sid`/`forky`.
- Firefox candidate always from Mozilla origin; `apt install firefox-esr` still possible but not default.

## 4. Kiln apt repository

### Decision
Use aptly (snapshots, multiple distributions, atomic publish, scriptable). Publish static files to Cloudflare R2 (S3-compatible) behind a custom domain `repo.<kiln-domain>` (domain TBD, owner action). Suites: `trixie-stable` and `trixie-fresh` (codenames avoid collision with Debian's), components `main` and `non-free` (for Nvidia and any firmware repacks), architectures `amd64`, `arm64`, `all`. Source packages also published. `Acquire-By-Hash: yes`.

### Rationale
- reprepro is simpler (no snapshots, no daemon) but has no snapshot/rollback of the whole repo; a bad Mesa upload with no instant rollback is the main risk. aptly snapshots give "republish previous snapshot" in one command.
- Free hosting options compared:
  - Cloudflare R2: 10 GB-month storage, 1M class A and 10M class B ops per month free, egress free (verified on Cloudflare docs). Nvidia debs are large (hundreds of MB each per arch), egress-free is decisive.
  - GitHub Pages: 1 GB published-site limit, 100 GB/month soft bandwidth, 10 minute deploy timeout (verified on GitHub docs). Fits only a small repo (Glaze, kiln-apps); not Nvidia or Mesa. Good for the first bootstrap.
  - Cloudsmith: has an open-source offering, but its pricing page was rate-limited when checked: terms (unverified). Revisit if R2 operations become a chore.
- aptly can publish straight to S3 but R2 compatibility needs testing; plan is `aptly publish` to a local dir then `rclone sync` to R2, pool before dists.

### Components and files
`scripts/repo/` (new): `aptly.conf`, `repo-init.sh`, `repo-add.sh <changes>`, `repo-publish.sh <suite>`, `repo-rollback.sh <suite> <snapshot>`, `rclone-sync.sh`.
Snapshots named `trixie-fresh-YYYYMMDD-N`. Retention: 10 snapshots per suite, 2 versions of each large package.

### Work breakdown
- aptly setup, scripts, local test with `apt` in a container: M
- R2 bucket, custom domain, cache rules (short TTL on `dists/`, immutable on `pool/` and `by-hash/`): S
- Bootstrap fallback on GitHub Pages (stable only): S

### Risks
- Single maintainer: key and bucket credentials are a single point of failure. Mitigation: section 4.1 and a documented recovery runbook.
- Repo hosting costs beyond 10 GB: R2 pay-as-you-go storage (price unverified, ~cents per GB-month). Mitigation: retention policy; only Nvidia branches and kernel-less rebuilds live here.
- Sync order issue (dists before pool) gives clients 404 mid-sync. Mitigation: script uploads pool, then by-hash, then Release files last.

### Acceptance tests
- Fresh VM: add the repo, `apt update` is clean (no warnings), `apt install kiln-apps` works.
- `repo-rollback.sh` returns a deliberately broken upload to the previous snapshot within 5 minutes; client `apt update && apt full-upgrade` returns to prior versions.
- `apt update` while `rclone sync` is in progress never fails hash verification.

### 4.1 Signing key handling
- Decision: two-tier OpenPGP keys. Offline primary key (ed25519, certify only, stored on a hardware token or an encrypted offline drive, revocation certificate printed and stored separately). One signing subkey (ed25519, 1-year expiry) for the repo. Debian 13 apt uses sqv, so RSA-1024/SHA1 would be rejected; ed25519 or RSA-4096 are fine (test on trixie before committing).
- Phase 1: owner signs on his own machine with `aptly publish` (aptly runs on macOS). No CI secret. Safe and simple.
- Phase 2: CI signs in a GitHub Actions environment `release` with required reviewer; the subkey (not the primary) is a secret `KILN_APT_SIGNING_KEY` stored as an encrypted secret; passphrase in a second secret.
- Distribution: `kiln-archive-keyring` package ships the public key; `kiln.sources` uses `Signed-By: /usr/share/keyrings/kiln-archive-keyring.pgp`. Never `apt-key`, never `trusted=yes`.
- Rotation: publish keyring update with both keys 3 months before expiry; ISO ships the keyring.
- Work: M. Acceptance: `apt update` with a tampered Release fails; expired subkey fails; keyring package update installs a second key.

## 5. Packaging Glaze and kiln-apps as .deb

### Decision
One source package per upstream, `3.0 (quilt)` format, built with `dpkg-buildpackage` in a trixie container (sbuild later). All files install to `/usr` (not `/usr/local`). Rust code vendored.

Source packages:
- `glaze`: binaries `glaze-session`, `glaze-compositor`, `glaze-shell` (GTK4 apps under `apps/`), `glaze-cli`, `glaze-data` (themes, assets, i18n, schemas), `glaze` (meta). Arch-dependent except `glaze-data`.
- `kiln-apps`: per `kiln-apps/PACKAGING-TODO.md`: `kiln-icon-theme`, `kiln-files`, `kiln-terminal`, `kiln-text`, `kiln-calculator`, `kiln-monitor`, `kiln-store`, meta `kiln-apps`.
- `tilesde` (Qt/QML): later, same layout.
- `kiln-base` (branding, os-release, fastfetch, plymouth, default apt channel, mimeapps), `kiln-archive-keyring`, `kiln-channel-*`.

debian/ layout (per source package):
```
debian/control  changelog  copyright  rules  source/format  watch
debian/*.install  *.postinst  *.triggers
debian/patches/series          # only for forked upstreams (kiln-files/kiln-text from Thunar/GNOME)
debian/cargo-config.toml       # vendored-sources replacement
```
Cargo vendoring: `cargo vendor --locked` run by `scripts/deb/make-orig.sh`, output as additional orig tarball `glaze_<ver>.orig-vendor.tar.xz` (3.0 quilt supports component tarballs). Extracted into `vendor/` before build. `debian/rules`:
```
export CARGO_HOME=$(CURDIR)/debian/cargo_home
export CARGO_INCREMENTAL=0
export RUSTFLAGS=--remap-path-prefix=$(CURDIR)=. -C strip=none
override_dh_auto_build:
	cargo build --release --locked --frozen --offline --workspace
```
`dh-cargo` from Debian is not used because Glaze's dependency graph (Smithay etc.) is not in Debian.
Rust toolchain: trixie's rustc is 1.85.0; the CI chroot adds trixie-backports for rustc 1.96.1 (check each crate's MSRV; Glaze `Cargo.toml` is `edition = "2021"`, no pinned `rust-version` found, so add one).
Copyright: aggregate third-party crate licences with `cargo about` into `debian/copyright` appendix; Kiln forks keep upstream GPL/MIT notices.
Versioning: upstream `1.0.0`, Debian `1.0.0-1~kiln13u1` for builds in trixie.

Reproducible builds: fixed `SOURCE_DATE_EPOCH` from `debian/changelog`, fixed build path (`/build/<pkg>-<ver>`), `dh_strip_nondeterminism`, `CARGO_INCREMENTAL=0`, no `build.rs` timestamps. CI builds twice and compares with `diffoscope`; failure blocks publication. `.buildinfo` files stored next to packages in the repo.

### Rationale
Currently Glaze and apps travel as staged files under `/usr/local` (see image README "Known gaps"). Without debs there is no upgrade path, no Update Manager integration, no rollback of apps. Vendoring keeps builds offline and reproducible.

### Components and files
New: `packaging/kilnlinux/deb/<pkg>/debian/`, `scripts/deb/make-orig.sh`, `scripts/deb/build-in-container.sh`. Modify: `packaging/kilnlinux/image/stage.sh` (stop copying `/usr/local`, install from repo instead), live-build package lists (add `glaze`, `kiln-apps`).

### Work breakdown
- `glaze` source package with vendoring, local build on arm64 VM: L
- `kiln-apps` set (7 binary packages, GTK/Python, schema triggers): L
- `kiln-base` + keyring + channel metapackages: M
- Reproducible check (twice build + diffoscope): M
- `debian/watch`, `copyright` automation: S

### Risks
- Vendor tarball size (Smithay, GTK-rs): maybe 100+ MB. Mitigation: `cargo vendor` with `--versioned-dirs` off, xz, drop unused platform crates; store as GitHub release asset, not in git.
- Lintian noise (vendored code, non-Debian crates). Mitigation: override list, treat lintian errors as blockers only for `E:`.
- Python app `kiln-store` mixes `/usr/lib/python3` rules: use `dh-python`, `pybuild`.

### Acceptance tests
- `dpkg-buildpackage -b` on arm64 VM and in the amd64 container produces `glaze_*.deb`, `lintian` has no `E:`.
- Install on a clean trixie VM: `glaze-session run --backend sway` starts; `glaze status` OK; nothing under `/usr/local`.
- Two builds produce bit-identical `.deb` content (`diffoscope` clean).
- `apt remove glaze` leaves no stray files (`dpkg -L` check, `debsums`).

## 6. CI on GitHub Actions (amd64 and arm64)

### Decision
Matrix: `ubuntu-24.04` (amd64) and `ubuntu-24.04-arm` (arm64; free for public repos, unverified, confirm for private repos). Each job runs inside a `debian:trixie` container, installs build-deps with `mk-build-deps`, builds, runs `lintian`, uploads `.deb`/`.changes`/`.buildinfo` as artifacts. A separate `publish` job (manual trigger or tag) downloads artifacts, runs `aptly` add and publish to a staging suite, runs the install smoke test in QEMU (existing x86 pipeline `stage-kiln/build`), then promotes.

Stages: build, lint, reproducibility rebuild, install smoke test (container with `systemd-nspawn` or QEMU boot of the live ISO), publish staging, promote to `trixie-fresh` or `trixie-stable`.
Caching: `actions/cache` for the vendor tarball and cargo target (target dir excluded from the release build to keep it reproducible).

### Work breakdown
- Workflow `deb-build.yml` (matrix, artifacts): M
- Smoke test job using QEMU and the existing x86 stage: L
- Publish job with approvals (phase 2 of 4.1): M

### Risks
- arm64 runner availability or pricing change. Mitigation: arm64 builds also run on the authoring Mac's arm64 VM (existing workflow) as a fallback.
- Full QEMU boot test in CI is slow (llvmpipe). Mitigation: nspawn smoke test for every push, QEMU boot nightly.

### Acceptance tests
- Push to main produces amd64 and arm64 debs in under 30 minutes (target; unmeasured).
- A deliberately broken `debian/control` fails the pipeline before publish.
- Secrets are never exposed to pull-request builds from forks (workflow permissions reviewed).

## 7. Flatpak and Flathub, and the Kiln Store

### Decision
Flathub is the default Flatpak remote, installed system-wide (`flatpak remote-add --system flathub`), already done in hook 0200. Policy: any GUI application that moves faster than monthly or ships its own dependency tree goes in Flatpak (Steam, Discord, OBS, GIMP, Inkscape, Krita, VLC, Spotify, VS Code, Chromium for users wanting newest). Debs are for: system, drivers, Glaze, kiln-apps, LibreOffice (backports), Firefox (Mozilla deb), Thunderbird (Debian). Kiln Store (`kiln-apps/kiln-store`, Python with PyGObject) lists three sources in this order: Kiln curated, Flathub, Debian (through PackageKit).

Implementation notes:
- Store talks to Flatpak through GObject introspection (`gir1.2-flatpak-1.0`) and AppStream metadata (`gir1.2-appstream`); no CLI scraping.
- Debian packages via PackageKit D-Bus with polkit; Update Manager owns apt-side updates, Store only installs.
- Kiln curated list: JSON in `kiln-base` (`/usr/share/kiln/store/featured.json`) mapping AppStream id to source preference.
- Flatpak 1.18 (Kiln rebuild in fresh) is wanted for newer features including preinstall support (unverified: confirm "flatpak preinstall" availability version) so the installer can drop an offline set of apps.
- Runtimes pulled automatically; Store shows the runtime size before install; Update Manager runs `flatpak update` on the same schedule as apt.

### Rationale
Flatpak apps are independent of the Debian base, give current versions, and are sandboxed. This is how Mint, Pop!_OS and Vanilla OS ship fast apps, without the Snap store lock-in.

### Work breakdown
- Store Flatpak backend (list, install, uninstall, update) against libflatpak: M
- Store PackageKit backend for debs: M
- Curated list and AppStream search merge: M
- Flatpak rebuild 1.18 for fresh: S
- Preinstall set in ISO: S

### Risks
- Disk use of runtimes (several GB). Mitigation: show size, dedupe runtimes via the curated list.
- Flathub is third-party; some apps unverified. Mitigation: default filter to "verified" in Store, label unverified.
- Flatpak CVEs (sandbox escape) require quick base updates. Mitigation: flatpak in the fresh set, tracked by the drift report.
- PackageKit apt backend quirks on Debian. Mitigation: fallback to a `kiln-install` helper (`apt-get` via polkit).

### Acceptance tests
- On a clean ISO: Store lists Flathub apps with icons; install and launch an app from Store; window appears under Glaze.
- `flatpak update` and `apt upgrade` both run from Update Manager once.
- Offline install of the preinstalled set works without network.

## 8. Kernel, Mesa, Nvidia and Firefox/LibreOffice policy

### 8.1 Kernel
- Decision: fresh uses backports (7.3.x now, follows forky). stable uses Debian 6.12 LTS. fresh keeps `linux-image-6.12*` installed as boot fallback (meta `kiln-kernel-lts`, Recommends only). GRUB shows both. Keep at least the previous kernel.
- Work: S for metapackage and `grub` default order; M for tests.
- Risk: new kernel vs Nvidia module build. Mitigation: kernel update blocked if the DKMS build for the installed Nvidia branch fails (hold in apt hook, see 8.3).
- Test: boot both kernels in QEMU; DKMS builds on both; `kiln-channel status` shows current and fallback.

### 8.2 Mesa
- Decision: backports 26.1.x in fresh now; add Kiln rebuild of 26.2.x only after a trixie sbuild passes (LLVM version in trixie may not satisfy; unverified). Keep Mesa and libgbm/libegl matched via the source-package pin (section 3).
- Work: S now, L later for the rebuild.
- Risk: Mesa 26.x dropping hardware (old Radeon, Intel gen) vs 25.0. Mitigation: stable channel is the documented fallback.
- Test: `glxinfo`/`vulkaninfo` and Glaze compositor start on Intel, AMD and virtio-gpu (llvmpipe in VM).

### 8.3 Nvidia
- Decision: Kiln packages Nvidia itself, base on Debian's `nvidia-graphics-drivers` source packaging (unstable 550.163.01-6) rolled to the latest upstream tarball: branch 595 (production) for fresh and 580 for legacy GPUs (Maxwell, Pascal, Volta) and for stable. Binary names compatible with Debian (`nvidia-driver`, `nvidia-kernel-dkms` or `nvidia-open-kernel-dkms`, `libnvidia-*`) with `Conflicts/Replaces` so Debian packages are cleanly replaced. Open kernel modules default for Turing and newer, required for Blackwell. DKMS builds with Secure Boot MOK flow (owner/Driver Manager design handles UI, see the sibling driver-manager design doc). `non-free` component. arm64: NVIDIA ships aarch64 Linux drivers but phones do not use them; ship arm64 only for SBSA/desktop.
- Rationale: Debian's 550.163.01 is the same in stable, backports, testing and unstable; it lacks Blackwell and kernel 7.x support (unverified: confirm DKMS build failure on 7.3). Without Kiln packaging, Nvidia on fresh is broken by design.
- Legal: NVIDIA's licence permits redistribution of the unmodified driver as Debian does; get a short licence review before publishing in the Kiln repo. Kernel module is not redistributed pre-built (DKMS).
- Work: L for the first branch (595), M for each additional branch, M for the apt hook that holds kernel upgrades when the DKMS build fails.
- Risks: NVIDIA branch churn (monthly) needs a bot (`uscan` plus a scheduled workflow to open a PR); kernel API breaks; Wayland and GBM issues in compositor (Glaze must test with `nvidia-drm.modeset=1`, `fbdev=1`, explicit sync).
- Tests: on a real Nvidia machine (owner hardware, not in VM): `nvidia-smi`, Glaze session on Wayland via Smithay and via Sway fallback, suspend/resume, DKMS rebuild after kernel upgrade, Secure Boot enabled boot.

### 8.4 Firefox, Thunderbird, Chromium, LibreOffice
- Firefox: Mozilla signed apt repo, key verified by hook 0100 (keep). Pin 1000 for `firefox*` from `packages.mozilla.org`. Debian firefox-esr is installed as fallback only.
- Thunderbird and Chromium: Debian security debs. Debian already tracks ESR 153 and Chromium 156; Flathub for newest.
- LibreOffice: fresh uses backports 26.8.x (note: sid has 26.8.3.1, backports 26.8.2.2); stable uses Debian 25.2.x. Backports ships with a large dependency set; run `apt -s` in CI.
- Work: S each.
- Test: `firefox --version` shows Mozilla build; `apt policy libreoffice-core` origin matches channel.

## 9. Version-skew risks (summary)

| Risk | Where | Mitigation |
|---|---|---|
| Partial upgrade leaves mismatched Mesa/libgbm/egl | fresh | source-package glob pins, CI `apt -s` check, Update Manager does `full-upgrade` only |
| Backports kernel vs Nvidia DKMS | fresh | held kernel update when DKMS fails, LTS kernel fallback |
| wlroots 0.18 (sway in stable) vs 0.20 (fresh) | Glaze fallback compositor | versioned `libwlroots-0.20` package installs beside 0.18; Glaze's Smithay compositor does not link wlroots |
| GTK 4.18 in base but apps built against newer | kiln-apps | build in trixie, minimum GTK 4.18 declared in `meson.build`/CI |
| Qt 6.8 (TilesDE) vs Qt 6.12 apps in Flatpak | phone/desktop convergence | TilesDE stays on 6.8 LTS; Flatpak apps carry their own Qt |
| Backports rebase to forky's next versions (kernel 7.4, Mesa 26.2) | fresh | `fresh-set.list` plus CI before each promotion; staging suite |
| Debian 14 release (expected 2027, unverified) | everything | `~kiln13u1` versions sort below Debian's; plan rebase window in release process |
| Mozilla repo disappearing or key change | Firefox | verify key fingerprint in CI, fallback to Debian firefox-esr |
| Flathub removal or app abandonment | Flatpak | Store shows maintainer status; curated list |

## 10. Rollback: btrfs and snapper, Timeshift fallback

### Decision
Installer default (Calamares): btrfs with subvolumes `@` (root), `@home`, `@snapshots`, `@var_log`, zstd compression, `/boot` inside `@` so kernels and root roll back together; EFI on a separate FAT partition. Snapper manages root snapshots. apt hooks create pre and post snapshots:
```
// /etc/apt/apt.conf.d/80kiln-snapper
DPkg::Pre-Invoke  { "kiln-snapshot pre"; };
DPkg::Post-Invoke { "kiln-snapshot post"; };
```
`kiln-snapshot` skips when the transaction is small (no kernel/driver/mesa/glaze) and labels each snapshot with the apt command. `grub-btrfs` adds a "Kiln snapshots" boot menu (arm64 GRUB EFI as in the current image). Update Manager shows "Restore point" list and calls `snapper rollback` through polkit, then asks to reboot.
Non-btrfs installs (and phones on ext4): Timeshift rsync mode, same hook, user-chosen schedule, snapshots only for `/` and not `/home`. Flatpak data under `/var/lib/flatpak` is excluded from snapshots to avoid size blowup (apps can be reinstalled).
Dpkg state, kernel, and initramfs roll back together; user data is untouched.

### Rationale
Mint drives users to Timeshift before updates; snapper plus btrfs is faster and atomic. Kiln fresh channel makes a safety net mandatory.

### Components and files
`kiln-snapshot` (shell script) and apt hook inside `kiln-base`; Calamares partition module config in the installer design; `grub-btrfs`, `snapper`, `btrfs-progs` in package lists; Update Manager UI (cross-doc).

### Work breakdown
- `kiln-snapshot` and hook, retention config (10 pre/post, 5 timeline): M
- Calamares btrfs layout, snapper init at first boot: M
- grub-btrfs on arm64 and amd64, boot test of rollback: M
- Timeshift fallback profile for ext4: S
- Update Manager restore UI: M

### Risks
- Snapper on `/` with a big `/var/lib/flatpak`: mitigated via separate subvolume excluded.
- Rollback with DKMS modules: modules live in `@`, roll back consistently. Secure Boot MOK state lives in firmware and is unaffected.
- Disk space: snapshots of kernel upgrades are small; retention caps disk use. Mitigation: warn at 15 percent free.
- Phones on flash storage: write wear; use timeline off, pre/post only.

### Acceptance tests
- VM: install a deliberately broken Mesa from a test repo, reboot into the previous snapshot from GRUB, desktop works.
- `snapper list` shows pre/post pair labelled with the apt command after `apt install`.
- `kiln-snapshot` skipped for a trivial install (one package, no triggers), executed for kernel install.
- Rollback leaves `/home` files unchanged (checksum test).

## 11. First 10 concrete steps

1. (S) Create `kiln-archive-keyring` and generate the OpenPGP primary (offline) and ed25519 signing subkey; test `apt update` against a throwaway aptly repo on trixie to confirm sqv accepts the key.
2. (M) Set up aptly locally (`scripts/repo/`), suites `trixie-stable` and `trixie-fresh`, components `main` and `non-free`, publish to a local directory, test from the arm64 dev VM.
3. (S) Register a domain, create the R2 bucket, upload via rclone, point `kiln.sources` at it. Keep GitHub Pages as a stable-only fallback.
4. (L) Write the `glaze` source package: vendored orig tarball, `debian/` with `dh` and `cargo build --locked --frozen --offline`, build on the arm64 VM, install on a clean trixie VM, confirm `glaze-session` starts and nothing lands in `/usr/local`.
5. (L) Write the `kiln-apps` source package per `kiln-apps/PACKAGING-TODO.md` (store, files, terminal, text, calculator, monitor, icon theme, meta), plus `kiln-base` (branding, os-release, mimeapps) replacing hook 0200 content.
6. (M) Add the GitHub Actions matrix (amd64 and arm64, `debian:trixie` container, lintian, artifacts) and the twice-build reproducibility check.
7. (M) Add channel metapackages and pin generator (`kiln-channel-fresh/stable`, `fresh-set.list`), with golden `apt policy` tests; switch live-build package lists from `/usr/local` staging to repo installs.
8. (L) Package `kiln-nvidia-595` and `kiln-nvidia-580` from Debian's packaging plus the new upstream tarballs; test on a real Nvidia machine with Wayland, Secure Boot off, then on.
9. (M) Backports pins for kernel, Mesa, LibreOffice, pipewire; Kiln rebuilds for wlroots 0.20, sway 1.12 and flatpak 1.18 via sbuild in a trixie chroot; add `~kiln13u1` versions and publish to `trixie-fresh`.
10. (M) Snapper and grub-btrfs with the apt hook in the Calamares layout; rollback demo in the VM; then Store Flatpak backend wired to Flathub.

## 12. Open items and unverified claims

- Nvidia branch numbers (production 595.112.07, feature 615.78.08, 580 as legacy branch) came from press reports; confirm on nvidia.com/Download/ before packaging.
- Whether Debian's Nvidia 550 DKMS fails on kernel 7.3 (expected, not tested).
- Mozilla apt repo arm64 support; the existing hook 0100 implies it works, confirm.
- GitHub free arm64 runners for the exact repo visibility.
- Flatpak `preinstall` minimum version.
- Cloudsmith free open-source terms and R2 paid storage price.
- Mesa 26.2 build dependencies on trixie.
- Debian 14 release timing and trixie-backports lifetime into LTS.
- Owner decisions needed: domain name for the repo, whether `fresh` is truly the ISO default, Nvidia licence review, hardware for Nvidia testing.
