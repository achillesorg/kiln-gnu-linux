# M0 screenshots (Kiln GNU/Linux VM, 1280x800, 2026-11-11)

Captured with `scripts/vm/shot.sh NAME` (QMP `screendump`) unless noted.

| File | State | Scripted check at the time |
|---|---|---|
| `01-login-idle.png` | First login with the new session (before the theme fix). Panel with Start button only. | `pgrep swaybar` empty; fede-shell environ has `SWAYSOCK` |
| `02-foot-taskbar.png` | Two foot windows, one task button; label almost invisible (GTK Adwaita button background/backdrop overriding FeDe CSS) and the first foot overlapping the panel. | `fede status`: `windows : 2` |
| `03-foot-taskbar-clamped.png` | After `floating_maximum_size` in the FeDe sway config: foot no longer covers the panel. Label contrast still broken. | `fede status`: `windows : 1` |
| `10-after-reboot-idle.png` | About 6 s after autologin following a full VM reboot. The panel is not visible yet (taken before it mapped, or a stale QMP frame). Kept for honesty, not as evidence. | — |
| `11-foot-task-button.png` | **Done-check.** After reboot, `swaymsg exec foot`: one floating foot window, one focused (terracotta) task button labelled "foot". | `fede status`: `backend : sway-ipc`, `windows : 1` |
| `12-idle-after-foot-closed.png` | foot killed: task button removed live. (The first capture of this state still showed foot; QMP frame lag, see `shot.sh`.) | `fede status`: `windows : 0` |
| `12b-idle-grim.png` | Same state through the grim-over-ssh fallback, pixel-identical at sampled points. | — |
