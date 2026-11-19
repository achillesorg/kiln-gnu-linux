# Kiln legal and branding checklist (F13)

Not legal advice; lawyer review before 1.0. Source: `docs/design/kiln-features.md` section 14.
Automated: `packaging/kilnlinux/tests/check-branding.sh [--rootfs DIR] [--iso-tree DIR]` (CI), `check-telemetry.sh static DIR`.

| # | Item | Gate | Status |
|---|---|---|---|
| L1 | Debian marks: "Kiln GNU/Linux, based on Debian" only; never "Debian Kiln"; no swirl; no `desktop-base` | `check-branding.sh` (packaging, rootfs, ISO boot menus); add website + ISO volume label | script done; `docs/legal/TRADEMARKS.md` TODO |
| L2 | "Kiln" name search (USPTO/EUIPO/WIPO, classes 9, 42) | manual, before first public ISO | open |
| L3 | os-release: `ID=kiln`, `ID_LIKE=debian`, `VERSION_ID`, `HOME_URL`, `SUPPORT_URL`, `BUG_REPORT_URL`, `LOGO=kiln-logo`, no `CPE_NAME`, keep `VERSION_CODENAME=trixie` | `check-branding.sh --rootfs` | script done |
| L4 | Codecs: no `x265`, `libfdk-aac`; counsel on HEVC | `docs/legal/CODECS.md` | TODO |
| L5 | Non-free firmware/Nvidia licence notice before install | Driver Manager | open |
| L6 | Fonts OFL only, no Microsoft fonts | package-list review | open |
| L7 | Third-party names only factual | copy review | open |
| L8 | SBOM (`syft` -> `kiln-<ver>.spdx.json`), `reuse lint`, `THIRD-PARTY.md` | CI `sbom.yml` | open |
| L9 | `/source-offer.txt` in ISO | `check-branding.sh --rootfs` warns if missing | script warns |
| L10 | Privacy page: no telemetry, mirrors log IPs, no account | website; policy in `deb/kiln-base/usr/share/kiln/telemetry-policy.toml` | open |
| L12 | `SECURITY.md`, `security.txt`, 7-day own-package CVE process | website | open |
| L13 | Signed ISO: `SHA256SUMS` + `.gpg` | release job | open |

Allowed Debian text: "based on Debian" attribution, `ID_LIKE=debian`, `VERSION_CODENAME`, apt mirror hostnames, package names.
