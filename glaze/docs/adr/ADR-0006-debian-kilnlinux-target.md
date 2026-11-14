# ADR-0006 — Debian 13 / KilnLinux target and the tty1 session path

- **Status:** Accepted
- **Date:** 2026-11-11
- **Deciders:** the user (docs/decisions.md Q8), implemented in M0
- **Supersedes:** the Fedora and Windows-host assumptions in ADR-0004 and
  ADR-0005 (their portability split still holds; only the platform names and
  commands change)

## Context

Glaze was specified for Fedora and authored on a Windows host. The real target
is now **KilnLinux**, a hobby OS based on **Debian 13 (trixie), arm64**, run in
QEMU with HVF on an Apple Silicon **macOS** host (`$HOME/fedeos/run.sh`).
The desktop keeps the name Glaze. In the VM, `virtio-gpu-pci` has no virgl, so
wlroots renders with llvmpipe (`kms_swrast`); input is a USB keyboard and
tablet.

The VM logs user `glaze` in on tty1 (getty autologin drop-in). Before M0,
`~/.bash_profile` ran `exec dbus-run-session glaze-session run --backend sway`.
`dbus-run-session` starts a **private** session bus. That bus cannot activate
`org.freedesktop.systemd1`, so the `dbus-update-activation-environment
--systemd` call in `/etc/sway/config.d/50-systemd-user.conf` failed on every
login (`Activated service 'org.freedesktop.systemd1' failed`), and anything
D-Bus-activated (portals, notification daemons) ran without
`WAYLAND_DISPLAY`. Sway also read `/etc/sway/config`, so its `bar {}` block
started a stray `swaybar` plus `swaybg`, and the shell never received
`SWAYSOCK`, so the taskbar could not list windows.

## Decision

1. **Platform.** Debian 13 arm64 (KilnLinux) is the primary target; x86_64
   Debian 13 is secondary and unverified. Fedora is no longer a target.
   Packaging moves to `.deb` packages or an image recipe (M7);
   `packaging/fedora/` is kept for reference only. Development happens on the
   macOS host (portable crates, `cargo test`, clippy, fmt) and in the VM (GTK
   apps, the session, screenshots).
2. **Session bus.** Use the **systemd user session** that `pam_systemd` already
   starts on the tty1 login, with its bus from `dbus-user-session`
   (`$XDG_RUNTIME_DIR/bus`). Drop `dbus-run-session`. The tty1 profile is
   versioned in `packaging/kilnlinux/bash_profile` and installed with
   `scripts/vm/install-login.sh`.
3. **Activation environment.** `glaze-session` itself, once the compositor is
   ready, runs `dbus-update-activation-environment --systemd` with explicit
   `WAYLAND_DISPLAY`, `SWAYSOCK`, `XDG_CURRENT_DESKTOP=Glaze`,
   `XDG_SESSION_TYPE=wayland` and `XDG_SESSION_DESKTOP=glaze`. This is best
   effort with a 5 s limit, and the result is logged.
4. **Glaze owns sway's config.** `glaze-session run --backend sway` starts sway
   with `-c ~/.config/glaze/sway.conf` (source `packaging/sway/sway.conf`,
   installed by `scripts/install-session.sh`): no `bar {}`, `swaybg_command -`,
   floating by default, Glaze border colours, `$mod+Return`/`Ctrl+Alt+T` for
   foot. It does not include `/etc/sway/config.d/*`.
5. **Children get the session identity explicitly.** The supervisor passes
   `XDG_SESSION_TYPE=wayland` (the getty login starts with `tty`) and the other
   session variables to every child, then `WAYLAND_DISPLAY` and `SWAYSOCK`
   (found as `$XDG_RUNTIME_DIR/sway-ipc.<uid>.<sway pid>.sock`) once sway is
   up.

A display manager (greetd or similar) and a full `systemd --user` unit graph
for Glaze components are **not** adopted now. They can be revisited with the
native compositor (M4) or packaging (M7).

## Consequences

- After a VM reboot, `session.log` shows `activation environment updated`
  instead of the systemd1 activation failure, and
  `systemctl --user show-environment` contains `WAYLAND_DISPLAY`, `SWAYSOCK`
  and `XDG_SESSION_TYPE=wayland` (verified in M0, see `PROJECT_STATUS.md`).
- No stray swaybar or swaybg. The wallpaper is drawn by the Glaze desktop layer.
- Login configuration of the VM (`~/.bash_profile`) is now part of the repo's
  documented setup. The getty autologin drop-in itself remains a manual VM
  setting outside the repo.
- `GLAZE.md`, `AGENTS.md`, `GOAL.md`, `START_PROMPT.md` and the
  `PROJECT_STATUS.md` header were rewritten for this target in M0.
