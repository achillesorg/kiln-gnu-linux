# Glaze / Kiln roadmap

## Kiln niche plan

Kiln GNU/Linux is Debian 13 made Windows-familiar, phone-to-desktop convergent
(TilesDE on the phone, Glaze when docked, one compositor), with Mint-style
first-party tools, a `fresh` channel newer than Debian stable, and first-class
proprietary drivers (Kiln-packaged Nvidia 580/595, signed modules, offline ISO payload).

Full plan, contradiction rulings, phases and the Debian branding checklist:
[`docs/design/KILN-PLAN.md`](docs/design/KILN-PLAN.md).

Phases, in dependency order:
0. Branding sweep, `os-release` `ID=kiln`, Kiln apt repo and signing key.
1. `.deb` packaging of Glaze, kiln-apps, TilesDE; `fresh` channel; btrfs/snapper rollback; Windows quick wins.
2. Nvidia packaging, `kiln-hwdb`, `kiln-helperd` core, Driver Manager, Calamares driver step.
3. Rest of the tool suite (Update Manager, Welcome, Sources, Reports, Backup, Software).
4. Convergence prototype in the dual-head aarch64 QEMU VM (P0 to P7).
5. Real devices: x86 convertible, OnePlus 6/6T, Fairphone 5.
