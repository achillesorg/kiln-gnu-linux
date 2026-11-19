# Windows switcher UX (Kiln GNU/Linux)

Status: design, not implemented. Scope: make a Windows user productive in the first hour, with no terminal.
Sizes: S = under 2 days, M = 2-7 days, L = over a week.
Repo state was checked 2026-11-18 (`packaging/sway/sway.conf`, `crates/glaze-wm/src/keybindings.rs`, `apps/glaze-shell`, `crates/glaze-shell-core`, `apps/glaze-settings`, `packaging/kilnlinux/image`, `kiln-apps`). "Unverified" means code exists but no VM evidence was seen in `PROJECT_STATUS.md`.

## Big caveat: architecture

Primary arch is arm64 (AGENTS.md). Windows compat (Wine, Steam, Proton) is x86 software.
- x86_64 image: Wine, Steam, Proton work natively (Steam needs `i386` multiarch; Debian puts `steam-installer` in non-free, Wine is `wine` 10.0 in trixie, newer via the newer-software track).
- arm64 image: needs an x86 translation layer (FEX-Emu or Box64). Bottles announced ARM64 support as a test release (67.4, experimental cpak, Flatpak not ready; itsfoss, Sept 2026). FEX wiki documents Proton under FEX. Treat as experimental on arm64.
Every section below marks "x86 only" where this applies. Verify package versions with `apt policy` on the build image; web results for trixie versions were incomplete.

## 1. Keyboard parity

Decision: Super is the Windows key. Bind every common Win shortcut in the compositor keymap (`Keymap::default_glaze`, shared data), mirrored to `sway.conf` while Sway is the fallback. Where Windows and tiling-WM habits clash, Windows wins by default; a "Tiling WM keys" preset restores the old ones.
Rationale: bindings live in one table (keybindings.rs says so); sway.conf is the only thing active on tty1 today.
Files: `crates/glaze-wm/src/keybindings.rs`, `packaging/sway/sway.conf`, `crates/glaze-compositor/src/input.rs`, new `kiln-run`, `glaze-lock`, `glaze-shot` helpers, `kiln-monitor` (Task Manager).

Conflicts to resolve (existing binding vs Windows meaning):
- `Super+1..4` switches workspace today; Windows launches the Nth taskbar app. Decision: Super+1..9 = taskbar app N (Win10 behaviour); workspaces move to Ctrl+Alt+arrows and Ctrl+Super+D/Left/Right (Win10 virtual desktops).
- `Super+Tab` is NextWorkspace today; Windows = Task View. Decision: Task View (overview of windows plus desktops).
- `Super+H` Minimize today; Windows has no such key. Keep. `Super+M` Windows = minimise all; sway uses it for scratchpad. Switch to minimise all.
- `Super+T` tiling toggle: keep (Windows Super+T cycles taskbar; low value).
- `Super+Space` launcher: Windows switches input language. Decision: Super+Space = input language (see section 10); Start already opens on Super tap.

Work:
- S: add missing bindings (table in section 12 marks each).
- M: `kiln-run` (Win+R) small GTK dialog: command box, history, Browse, "run as administrator" via pkexec, accepts `cmd`, `regedit`, `control`, `msconfig`, `notepad`, `calc`, `explorer`, `taskmgr`, `winver`, `devmgmt.msc` and maps each to the Kiln tool (alias file `/usr/share/kiln/run-aliases`). Nice and cheap win.
- M: Win+L lock: needs a locker. Sway fallback: `swaylock`; compositor: ext-session-lock-v1. Auto-lock after idle, lock on lid close.
- S: Win+Shift+S region screenshot: `grim -g "$(slurp)" - | wl-copy` plus notification, plus PrintScreen = full screen to `~/Pictures/Screenshots`. `grim`, `slurp`, `wl-clipboard` are already in the package list. M for a proper Snipping tool UI (rectangle, window, full, delay, annotate).
- S: Ctrl+Shift+Esc launches `kiln-monitor` on its Processes tab; Ctrl+Alt+Del opens a small "Lock / Switch user / Log off / Task Manager" screen (M).
- Also: Win+V (cliphist is already in the package list; needs a picker UI, M), Win+. emoji picker (S, Debian `gnome-characters` or own; Wayland text-input caveats), Win+I Settings, Win+A quick settings, Win+N notification centre, Win+P display switch (M, needs docking logic), Win+Pause About, F2 rename and F5 refresh in Files (verify).

Risks: compositor-global bindings only work in compositor/sway, not in the shell panel (keybindings.rs states this); lone-Super-tap vs Super combos (comment in sway.conf, glaze-shell must ignore combos); GTK apps eat Ctrl+Shift+Esc rarely.
Acceptance: scripted test per shortcut in the VM via `scripts/vm/input.sh` (wtype) with `swaymsg -t get_tree` / process checks; screenshot of Win+R, Win+Shift+S result file present; Win+L blocks input until password.

## 2. Taskbar, Start, tray, Action Centre

Decision: keep the Win10-style bar already built; close gaps rather than redesign. Classic (2000) and Clay (default) themes both keep the same layout so muscle memory transfers.
Rationale: bar already has Start, search button, task button groups with hover, jump-like context menu, tray well, clock (PROJECT_STATUS taskbar table).
Files: `apps/glaze-shell/src/{panel_ui,start_pane,classic_menu,launcher,toast}.rs`, `crates/glaze-shell-core/{taskbar,tray,clock,status_feed,notifications}.rs`.

Done: Start tap-Super; search box; task buttons with focus/minimise/restore, group chooser, middle-click new window, context menu, snap menu; tray well + clock; toasts; Win10-style pinned apps (unverified: pinning persistence).
Missing / unverified:
- M: pin/unpin to taskbar, drag-reorder, Super+N launch pinned (needs section 1 change).
- M: Show desktop sliver at the right edge, Win+D toggling (minimise all + restore).
- L: Task View overview with desktops and window thumbnails (no window thumbnails in hover card today; needs compositor screencopy / foreign-toplevel-capture).
- M: Action Centre / quick settings flyout: Wi-Fi, Bluetooth, volume, brightness, night light, power mode (`power-profiles-daemon`, `brightnessctl`, `network-manager` already in image), plus notification history (notifications.rs exists; history unverified). Calendar flyout from clock (S-M, verify).
- M: tray: StatusNotifierItem host (`tray.rs` exists, verify SNI support for Discord/Steam/Dropbox style icons; Win users live in the tray). Hidden-icons overflow chevron (S).
- S: window list "Taskbar settings" page in glaze-settings (position, combine buttons, small icons).
- Start menu: all-apps list, pinned grid, Power menu with Shut down/Restart/Sleep/Hibernate/Log off. Mostly in `start_pane.rs`; verify power row and "Lock" there.
Risks: SNI needs `libayatana-appindicator` for apps that only speak legacy XEmbed tray (XWayland apps like old Wine tray icons: Wine tray under Wayland needs a bridge; known pain).
Acceptance: VM screenshots per item into `docs/screens/switcher/`; `swaymsg` checks for Show desktop; tray test with a `StatusNotifierItem` sample and with `steam` (x86).

## 3. File explorer (Kiln Files, Thunar 4.20 fork)

Decision: tune Kiln Files defaults to Explorer behaviour, no new file manager. Win+E opens it.
Components: `kiln-apps/kiln-files`, `kiln-apps/icon-theme`, `~/.config/Thunar/` defaults shipped in `/etc/xdg`, gvfs, udisks2.
Behaviours and work:
- S: defaults: single-pane details view, "Name, Date modified, Type, Size" column set, folders first, hidden files off, extensions shown, double-click to open (Thunar default is double click, keep), Backspace = up, Alt+Left/Right back/forward, Alt+Up up, F2 rename, F3 / Ctrl+F search, Ctrl+Shift+N new folder, Delete = trash, Shift+Delete = permanent with confirm, Ctrl+L / Alt+D address bar (verify each, mostly Thunar defaults).
- S: left pane: Quick access (Desktop, Downloads, Documents, Pictures, Music, Videos), "This PC" with drives, Network, Recycle Bin renamed "Recycle Bin" (label only, Trash technically).
- M: breadcrumb path bar by default, search-as-you-type, recent files list.
- M: preview pane (Alt+P) and details pane for images/PDF/Office.
- M: Properties dialog parity: Permissions tab explained in plain words ("who can read / change"), Details tab.
- S: archive handling: right-click Extract here / Compress to ZIP (`thunar-archive-plugin`, `file-roller`, `p7zip-full`, already in list; add `unrar` is non-free, so `unrar-free`/7z RAR read).
- M: "Open with" dialog with "Always use this app" (Thunar has it).
Risks: Thunar fork is pinned at 4.20.2; every change becomes a patch to maintain (kiln-apps DECISIONS). Prefer config over code.
Acceptance: checklist run in the VM with screenshots; each keyboard shortcut scripted via wtype.

## 4. Right-click "New" menu

Decision: ship templates so New > Text Document, Word-compatible Document, Spreadsheet, Presentation, Folder, Shortcut (link) works.
Rationale: Thunar builds "Create Document" from `~/Templates`; GNOME only shows New Document when templates exist. Empty Templates is the common "where is New file" complaint.
Components: `/etc/skel/Templates/` (Text Document.txt, Document.docx, Spreadsheet.xlsx, Presentation.pptx, Shell script), `xdg-user-dirs` update, desktop context menu in `apps/glaze-shell/src/context_menu.rs` (crate `glaze-desktop`), Files context menu.
Work: S templates with empty valid OOXML files generated by LibreOffice; M wire the Glaze desktop right-click menu to the same list and add "New > Shortcut" (creates `.desktop` link) and "Personalise" / "Display settings" entries opening glaze-settings pages. Rename Thunar "Create Document" to "New".
Acceptance: right-click on desktop and in a folder shows New with 5+ entries; created .docx opens in LibreOffice Writer without a repair prompt.

## 5. Drive letters and display names

Decision: show real labels, not letters. For NTFS/FAT/exFAT volumes show "Label (size)" or "Windows (C:)"-style only in a "Windows names" compatibility hint.
Rationale: Linux has no C:. Windows users still ask "where is my C: drive". The mounting path `/media/<user>/<LABEL>` plus label is clear.
Components: udisks2 + gvfs labels, Kiln Files sidebar, Welcome cheat sheet, section 9 wizard.
Work:
- S: sidebar entries "Windows (NTFS, 476 GB)" using label else "Disk 2 partition 3"; internal disks show a drive icon with usage bar.
- S: Welcome text: "Your home folder is your Documents area (like C:\Users\you). The whole system is `/` (like C:\)." plus "File system" entry named "Computer (system files)".
- S: mount NTFS read-only for Windows partitions with Fast Startup/hibernation dirty flag (`ntfs-3g` already; prefer the kernel `ntfs3` driver, check trixie kernel 6.12). Explain "Windows is hibernated" in plain words, offer "Open read-only".
- M: optional DOS-name display for users who want it (off by default).
Risks: writing to a hibernated NTFS can corrupt data; default read-only when flag is set.
Acceptance: plug a labelled USB and an NTFS image; sidebar shows names; hibernated NTFS opens read-only with a dialog.

## 6. Fonts and rendering

Decision: install metric-compatible stand-ins and alias the Microsoft names via fontconfig, so Office documents and web pages keep their layout. Rendering defaults: grayscale antialiasing plus slight hinting, subpixel (RGB) positioning on; "ClearType tuner" page offers LCD subpixel / grayscale.
Mapping (verified metric-compatible families):
| MS font | Stand-in | Debian package (verify with apt) |
|---|---|---|
| Arial, Times New Roman, Courier New | Liberation Sans/Serif/Mono | `fonts-liberation2` (in list) |
| Calibri | Carlito | `fonts-crosextra-carlito` |
| Cambria | Caladea | `fonts-crosextra-caladea` |
| Segoe UI (look only, not metric) | Inter | `fonts-inter` (in list) |
| Verdana, Tahoma | DejaVu Sans / Inter (not metric) | `fonts-dejavu` |
| Consolas | Noto Sans Mono / Fira Code | present |
Components: `/etc/fonts/conf.d/30-kiln-msfonts.conf` aliases, glaze-settings Fonts page, GTK settings (`gtk-xft-antialias`, `hintstyle=hintslight`, `rgba`), Flatpak font access.
Work: S packages + alias file; S default UI font Inter 10/11pt, document default stays Carlito for new LibreOffice docs; M "Install Microsoft fonts" helper that, with user consent, downloads from the user's Windows partition (copy `C:\Windows\Fonts`, licence stays with the user's Windows licence) rather than distributing; M HiDPI fractional scaling defaults from the compositor (already supports scale 1.75 in tests).
Risks: never ship MS fonts (licence); Segoe UI emulation is look-alike only; subpixel RGB on OLED/rotated panels looks wrong, so default to grayscale on arm64 phones.
Acceptance: fc-match "Arial" -> Liberation Sans, "Calibri" -> Carlito, "Cambria" -> Caladea; open a Calibri .docx and compare page count with Word reference; screenshot of a text-heavy page at 100% and 175%.

## 7. .exe handling

Decision: double-clicking a .exe/.msi opens "Run Windows program" dialog (not a hex error, not "no app"). Wine, preconfigured, one prefix per app, managed by Bottles. Games go to Steam/Proton, Lutris-style prefixes optional.
Rationale: Bottles gives per-app "bottles" (prefixes), runners (Soda/Caffe/Wine-GE/Proton), dependency installers, and a Flatpak; running unknown .exe from a user without malware warnings is the actual risk.
Components: `kiln-winrun` (small launcher script plus GTK dialog), MIME handler `application/x-ms-dos-executable`, `application/x-msi` in `mimeapps.list`, Flatpak `com.usebottles.bottles`, apt `wine` (Debian trixie 10.0) as fallback, `winetricks`, `wine-binfmt` for terminal, `lutris` (Flatpak or apt), `steam-installer` (x86), `gamemode`, `mangohud`. Kiln Store category "Windows apps" with known-good entries (compat database of Wine AppDB ratings, verify licence).
Flow ("open .exe"):
1. Dialog: "This is a Windows program. Kiln can run many of them. [Run in new app space] [Run in existing: <list>] [Install to Kiln Store list] [Cancel]". Plain warning: Windows programs can contain viruses; only run from trusted sources. Scan with ClamAV on demand (optional, S).
2. Create prefix `~/.local/share/kiln/windows/<app-name>/` (wine prefix per app, 64-bit by default, 32-bit when installer demands), pick a runner, apply recipes (dotnet, vcredist, dxvk, vkd3d) through Bottles dependency list or winetricks.
3. After install create a Kiln Start menu entry (`.desktop` generated from the Wine Start menu) with icon extracted by `wrestool`/`icoutils`.
4. Uninstall: Kiln Store > Windows apps > Remove (deletes the prefix, one click, asks about saved data).
5. `.msi` via `msiexec`; `.bat` via `wineconsole`; `.lnk` resolved.
x86_64: native. arm64: translation layer (FEX/Box64 + Bottles ARM64 test builds); show "experimental, may be slow" banner and an honest "does not run: 32-bit/anti-cheat/DRM" list.
Work: M `kiln-winrun` dialog + mimeapps; M prefix-per-app wrapper (can reuse Bottles CLI `bottles-cli`); S default Wine settings (Windows 10 version, DPI follows desktop); M known-issues catalog; L arm64 translation integration; S "Windows apps" row in Welcome.
Risks: malware (Wine runs with user rights; consider bubblewrap sandbox or Flatpak Bottles for isolation, but then file access needs `--filesystem` prompts); Wine tray icons; anti-cheat games do not work; Debian's wine lags upstream.
Acceptance: install Notepad++ installer from a USB in the VM (x86) via double-click; icon appears in Start; remove cleans prefix. Test .exe that is a malware sample-like EICAR shows scan warning.

## 8. Office file compatibility

Decision: LibreOffice default, docx/xlsx/pptx default handlers (already in `mimeapps.list`), fonts from section 6, "Save as Word format by default" preset, OnlyOffice and Microsoft 365 web as documented alternatives.
Components: `kiln-extra.list.chroot` (LibreOffice Writer/Calc/Impress, gtk3 UI), `mimeapps.list`, LibreOffice profile defaults in `/etc/libreoffice/registrymodifications.xcu` (save as docx/xlsx/pptx, UI font Carlito, auto-recovery on), templates from section 4, newer LibreOffice via section for newer software (separate doc).
Work: S set defaults and default save format; S add `hyphen-*`/`hunspell-*` for installed languages; M a compatibility note when opening a file with unavailable fonts ("Calibri is shown with Carlito"); M Welcome entry "Using Microsoft 365: open outlook.office.com / office.com in the browser; install as PWA (Firefox/Chromium)". Macro-heavy xlsm will not work: state it.
Risks: DOCX layout drift on complex documents; "slim" build omits LibreOffice (`build.sh --slim`), so the default image must still offer a one-click install in Kiln Store.
Acceptance: corpus of 10 representative docx/xlsx/pptx files round-trips; page counts match Word within 1 page; default save format check; `xdg-mime query default` for the 6 MIME types.

## 9. Windows-to-Kiln migration wizard ("Move from Windows")

Decision: a Welcome/first-run tool, `kiln-migrate`, that imports data read-only from a mounted Windows partition, USB drive, external disk, or a folder; never modifies the source. Runs in the installer end (Calamares) and from Welcome later.
Components: new app in `kiln-apps` (Python GTK4/libadwaita to match `kiln-store`, or Rust), udisks2 for mounting `ro`, `ntfs3`/`ntfs-3g` read-only, optional `chntpw`/`hivexsh` (libhivex) to read the registry hive (`SOFTWARE`, `SYSTEM`) for Wi-Fi? (see below), browser readers.
Steps:
1. Detect: list partitions with `Windows/System32` and `Users/`; show users found.
2. Choose what: Documents, Desktop, Pictures, Music, Videos, Downloads (copy, show size, free-space check, `rsync` with progress and resume, skip `AppData` by default, skip files over a limit, never delete). Also OneDrive folders (local cache only).
3. Browsers: Chrome/Edge (Chromium profile `AppData/Local/Google/Chrome/User Data/Default/Bookmarks` JSON, `Login Data` skipped), Firefox (`places.sqlite` bookmarks, `.json` export). Bookmarks only by default; passwords: show instructions to export CSV from the old browser, do not parse the encrypted DPAPI stores. Import into Firefox via bookmarks HTML written to a file the user imports, or write to profile when Firefox is not running.
4. Wi-Fi: WLAN profiles live in `ProgramData\Microsoft\Wlansvc\Profiles\Interfaces\<GUID>\*.xml` and keys are DPAPI-encrypted when read offline, so passwords cannot be recovered from a cold disk. Offer: import SSID list (names only) and tell user to re-enter passwords; if the user runs `netsh wlan export profile key=clear` on Windows into a USB, import those XML files into NetworkManager (`nmcli`). Document this honestly.
5. Wallpaper: copy `AppData/Roaming/Microsoft/Windows/Themes/TranscodedWallpaper` (and the user's wallpaper path) to `~/Pictures/Wallpapers` and offer to set it (only the user's own image).
6. Mail: Thunderbird profile import from Outlook PST via `libpst`/`readpst` (optional, M); Outlook email on web needs no import.
7. Report: what moved, what was skipped (apps, settings, Office add-ins, game saves), plus a list of Windows apps found in `Program Files` mapped to Kiln equivalents (static mapping table in the wizard and in Welcome).
Work: M detection + folder copy; S wallpaper; M browser bookmarks; M Wi-Fi from exported XML; M Thunderbird/PST; M app mapping report; S Calamares hook to launch it. Total L.
Risks: hibernated NTFS (section 5); BitLocker (cannot read; show "unlock in Windows first or provide recovery key via `cryptsetup` bitlk"; `dislocker`; M); permissions on `Users/<other>`; privacy: never upload anything; tell the user what will be read.
Acceptance: test against an NTFS image with a fake profile (fixture in `tests/fixtures/windows-disk`); checksums match after copy; source unchanged (mounted ro, verified with `mount` flags and hash); BitLocker image gives a clear message.

## 10. Language, locale, IME

Decision: system language chosen in Calamares and in Settings > Language; per-user input sources with Super+Space to switch (Windows Win+Space and Alt+Shift both work); IME via IBus (fcitx5 as option) with Wayland text-input-v3 support; keyboard layout names as Windows shows them ("US", "US International", "Spanish").
Components: Calamares locale module, `glaze-settings` Keyboard/Language pages (not in the catalog today: catalog lists Sound, Network, Windows etc.; Language and Keyboard page not found, so M), `ibus`, `ibus-libpinyin`, `ibus-mozc`, `ibus-hangul`, `ibus-typing-booster`, `fonts-noto-cjk`, `console-setup`, systemd-localed.
Work: M Settings pages for Language & region, Keyboard layouts, input method; M indicator in tray (layout code like "ES"); S Alt+Shift and Super+Space bindings (compositor `xkb_options grp:alt_shift_toggle`); M IME verification on Wayland for GTK4, Qt, Firefox, LibreOffice, XWayland apps (`GTK_IM_MODULE` unset on Wayland; set `QT_IM_MODULE=ibus` for Qt5 where needed); S dead keys / compose / AltGr work like Windows US-International; S date/time/number formats per locale including 12/24 h.
Risks: IME on custom Smithay compositor requires `zwp_text_input_v3` and `zwp_input_method_v2` implemented (verify in `glaze-compositor`; Sway has them).
Acceptance: type Chinese pinyin and Japanese in Kiln Text and Firefox in the VM; layout switcher test; locale pages change `localectl` output.

## 11. WSL-less developer tools

Decision: "it is Linux, no WSL needed" pitch: Kiln Terminal with a Windows-style default (Ctrl+Shift+C/V, tabs), `git`, `curl` present; Developer mode in Welcome installs toolchains from one list.
Components: `kiln-terminal` (foot fork) plus Kiln Store "Developer" category; Podman/Docker: `podman` + `distrobox` for running other distros; `build-essential`, `python3-venv`, `nodejs`, `rustup`; VS Code from Microsoft repo only if user asks (non-free; Flathub as alternative); VSCodium; PowerShell (`powershell` Flatpak/Snap not default; skip) with a "Windows command cheat sheet" (`dir` = `ls`, `cls` = `clear`, `ipconfig` = `ip a`).
Work: S Developer Welcome tab; S `distrobox` + `podman` metapackage `kiln-dev` (optional install); M terminal profile for "bash with Windows aliases" (opt-in aliases `dir`, `cls`, `type`); S Win+X, Win+R and Open terminal here in Files (Thunar custom action).
Risks: container runtimes need rootless setup (`uidmap`, `slirp4netns`); do not preinstall heavy stuff in the default ISO.
Acceptance: fresh install runs `kiln-dev` install; `distrobox create` Ubuntu works in VM; terminal paste shortcut test.

## 12. Gaming and Nvidia

Decision: Steam, Proton and GameMode one-click from Kiln Store "Gaming" page; Nvidia proprietary driver is a Driver Manager feature (separate doc: drivers). Not in the default arm64 image.
Components (x86_64): `steam-installer` (non-free, `dpkg --add-architecture i386`), `gamemode`, `mangohud`, `goverlay` optional, `lutris`, `heroic` Flatpak for Epic/GOG, Proton-GE via `protonup-qt` Flatpak, `libvulkan1:i386`, `mesa-vulkan-drivers:i386`, `nvidia-driver` + `nvidia-driver-libs-i386` (trixie non-free) or newer driver from the newer-software track, `xwayland` with Gamescope (`gamescope`) as session for full-screen games; `steam-devices` udev rules for controllers; `xboxdrv`/xpadneo as options.
Work: M Gaming page in Store with check "Enable 32-bit support" button (pkexec adds architecture and updates); M Driver Manager tie-in (Nvidia detection, secure boot MOK enrollment flow, `nvidia-drm.modeset=1` default, `nvidia-suspend` units); S GameMode integration `gamemoderun %command%`; S refresh-rate and VRR toggles in Display settings (compositor support unverified); L arm64: FEX/Box64+Steam via Valve's Steam on ARM is not supported by Valve; label "experimental, not supported"; M controller test page.
Risks: Nvidia on Wayland needs driver 555+ for explicit sync and GBM; Smithay compositor compatibility with Nvidia (verify; Sway needs `--unsupported-gpu`); Secure Boot blocks unsigned modules (DKMS needs MOK); anti-cheat.
Acceptance: x86 VM with GPU passthrough or real hardware: Steam installs, a Proton title launches, `gamemoded -s` shows active; Nvidia driver install then `nvidia-smi` works after reboot under Secure Boot test.

## 13. Printing and scanning

Decision: print and scan "just work" with driverless IPP for modern devices.
Components: `cups`, `cups-filters`, `cups-browsed` (disabled by default for security reasons, discovery via `avahi-daemon` only for local network and `ipp-usb`), `system-config-printer` equivalent in glaze-settings (page missing: catalog has no Printers page; M), `printer-driver-all`, `hplip` (non-free parts via HP plugin only on demand), `sane-airscan`, `simple-scan` (Kiln-branded), Print to PDF by `cups-pdf`/`printer-driver-cups-pdf`.
Note: package list has an empty "printing" section; nothing installed yet.
Work: S packages + `ipp-usb`; M Printers page (add by IP, test page, queue view, tray printer icon); S simple-scan; M scanner shortcut in Files; S Win+P is display not print (note).
Risks: CUPS network exposure (`cups-browsed` CVEs 2024); keep browsed off by default; vendor drivers non-free (Brother, Epson via Debian `printer-driver-*` or vendor deb).
Acceptance: printer test with a CUPS-PDF and an IPP Everywhere emulator (`ippeveprinter`) in VM; scan through `sane-airscan` emulator if possible, otherwise real hardware.

## 14. Welcome "Where is X?" cheat sheet

Decision: a searchable list in Welcome (also reachable from Help and Win+F1), written in Windows words with a button that opens or runs the thing. Maintain as data (`/usr/share/kiln/welcome/where-is.toml`) so the Welcome app, Start-menu search aliases, and `kiln-run` aliases share one source.
Entries (examples): Control Panel -> Settings; Task Manager -> Kiln Monitor; Device Manager -> Driver Manager (separate doc); Windows Update -> Update Manager; Programs and Features -> Software; Command Prompt/PowerShell -> Terminal; Notepad -> Kiln Text; Paint -> Kiln Viewer/Drawing; Calculator; File Explorer -> Kiln Files; Recycle Bin; Snipping Tool; Disk Management -> Disks (`gnome-disk-utility`, add); Registry Editor -> dconf/gsettings and config files (explain); Services -> `systemctl` via a small GUI; Event Viewer -> Logs; Remote Desktop -> Remote Desktop (RDP client `remmina`) ; Microsoft Store -> Software; OneDrive -> cloud via Nextcloud/Proton/Drive in browser; Edge -> Firefox; Outlook -> Thunderbird; Office -> LibreOffice.
Work: S data file with 60 entries; M Welcome UI with search, buttons "Open it"; S tie into Start-menu search (typing "control panel" finds Settings).
Acceptance: each entry has a target that exists on the image (script `check-where-is.sh` verifies each `.desktop`/command exists); type "task manager" in Start returns Kiln Monitor.

## 15. Top 30 Windows habits and Kiln equivalents

Status: Done = present in repo and seen in `PROJECT_STATUS.md` or sway.conf; Partial = exists with a gap; Missing = not found in repo; Unverified = code exists, no evidence of a run.

| # | Windows habit | Kiln equivalent | Today in Glaze |
|---|---|---|---|
| 1 | Win tap opens Start | Super tap, `glaze-shell --toggle-start` | Done |
| 2 | Type in Start to search | Start search / search button | Done (apps only; files and settings missing) |
| 3 | Win+E Explorer | Kiln Files | Missing binding; app exists |
| 4 | Win+D show desktop | minimise all / restore | Missing (Super+M used for scratchpad) |
| 5 | Win+L lock | lock screen | Missing |
| 6 | Win+R Run | `kiln-run` | Missing |
| 7 | Alt+Tab switch window | Alt+Tab (WmAction::CycleWindows) | Done in keymap; compositor switcher UI `alt_tab.rs` exists; unverified under sway |
| 8 | Win+arrows snap | `glaze-snap` | Done (verified in VM) |
| 9 | Win+Shift+S screenshot | grim+slurp helper | Missing (tools installed) |
| 10 | PrtSc | full screenshot to Pictures | Missing |
| 11 | Ctrl+Shift+Esc Task Manager | Kiln Monitor | Missing binding; app exists |
| 12 | Win+Tab Task View | overview | Missing (Super+Tab is workspace next) |
| 13 | Win+1..9 launch pinned | taskbar app N | Conflict (Super+1..4 = workspaces) |
| 14 | Win+I Settings | Glaze Settings | Missing binding; app exists |
| 15 | Win+A / Win+N | quick settings / notifications | Partial (toasts exist; flyouts missing) |
| 16 | Win+V clipboard history | cliphist picker | Missing UI (cliphist installed) |
| 17 | Win+. emoji | emoji picker | Missing |
| 18 | Win+P projector | display mode switch | Missing |
| 19 | Taskbar buttons, click to minimise/restore | same | Done |
| 20 | Taskbar right-click jump list | context menu | Partial (window actions, no recent items) |
| 21 | Pin app to taskbar | pin | Unverified |
| 22 | System tray, hidden icons | tray well | Partial (tray exists; SNI coverage unverified, no overflow) |
| 23 | Clock, calendar flyout | clock | Partial (clock yes; calendar flyout unverified) |
| 24 | Right-click desktop > New / Personalise / Display | desktop menu | Partial (menu done; New templates missing) |
| 25 | Drag title to edge to snap | compositor snap | Partial (needs Glaze compositor; sway lacks it) |
| 26 | Recycle Bin on desktop | Trash | Unverified (desktop icons exist; Trash icon not confirmed) |
| 27 | Install by double-clicking .exe | `kiln-winrun` | Missing |
| 28 | Control Panel / Settings search | Settings has search (`search.rs`) | Done |
| 29 | Win+Space / Alt+Shift language | input sources | Missing |
| 30 | Windows Update / uninstall programs | Update Manager / Software | Partial (`kiln-store` exists; Update Manager separate doc) |

## 16. Roll-up plan (ordered)

1. S: keymap parity and conflict decisions (section 1), `Super+E/D/I/R/L`, PrtSc, Ctrl+Shift+Esc.
2. S: fonts + fontconfig aliases + LibreOffice defaults (6, 8).
3. S: Templates, Files defaults, drive labels (3, 4, 5).
4. M: `kiln-run`, lock, screenshot tool, Show desktop.
5. M: Action Centre, tray SNI/overflow, taskbar pinning.
6. M: `kiln-winrun` + Bottles (x86 first), Welcome where-is sheet.
7. M-L: migration wizard, language/IME pages, printers page.
8. L: Task View overview, gaming + Nvidia (with Driver Manager doc), arm64 Windows-app translation.

Overall risk: scope. Items 1-3 deliver most of the first-hour comfort with almost no new code. Do them first.

## Sources used

- Bottles ARM64 news: https://itsfoss.com/news/bottles-arm64-support/
- FEX-Emu Proton page: https://wiki.fex-emu.com/index.php/Proton
- Debian trixie wine 10.0~repack: https://aptpeek.dev/debian/trixie/main/i386/wine
- Repo files listed in the header.
