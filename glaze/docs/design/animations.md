# Motion plan: Glaze desktop and TilesDE

Status: plan only, no code changes. Scope: Glaze (`apps/glaze-shell/src/anim.rs`,
`crates/glaze-compositor/src/anim.rs`, `crates/glaze-design/src/motion.rs`) and TilesDE
(`/Users/xavi/fedeos/tilesde/shell/qml`).

## 1. Principles

- Animate `opacity` and `translate` only. No scale transforms on window surfaces, no blur
  animation, no animated shadows, no layout-driven width/height animation on large surfaces.
- The VM compositor (llvmpipe) leaves stale pixels on scale transforms and samples scaled
  textures several times slower than 1:1. Scale is allowed only on small, GPU-cheap, non-window
  elements (a press dip on a tile or button) and never on a client surface.
- No overshoot in Glaze (flat paper). TilesDE may use one springy settle (press, app open);
  nothing bounces on exit.
- Exits are shorter than entrances (about 0.8x). Exits use an ease that is visible from frame 1.
- Idle cost is zero: tick callbacks and timers run only while something moves.
- Everything honours reduced motion (section 5). Reduced motion means instant, not slower.
- Interruptible: any running animation retargets and keeps velocity (Glaze `Spring`, QML
  `Behavior`). Never queue.

## 2. Audit of existing animation

### Glaze compositor (`glaze-compositor/src/anim.rs`, 788 lines)
- Easings: `Linear, OutCubic, InCubic, InQuad, InOutCubic, Spring` (critically damped, K=7).
- Base durations (ms): OPEN 180, CLOSE 230, MINIMIZE 220, RESTORE 220, MOVE 220, WORKSPACE 260,
  FOCUS 160, POPUP 140, POPUP_CLOSE 120, SWITCHER 120, LOCK 220, LOGOUT 350, HOVER 90.
- Intensity `off/low/normal/high` multiplies time 0/0.6/1/1.25 and slide 0/5/10/16 px.
  `popup_slide = slide * 0.6`.
- Open: rise `OPEN_SLIDE` (10 px) plus fade, no scale (documented llvmpipe fix). High intensity
  adds `OPEN_SCALE` 0.94, skipped on software renderer.
- Good: Tween retarget keeps current value, zero duration under Off. Gap: durations are
  hard-coded constants, not shared with the shell or settings.

### Glaze shell (`apps/glaze-shell/src/anim.rs`, 273 lines)
- GTK frame-clock `Animator`, one per host widget. Critically damped `Spring`
  (`glaze_design::motion`), `settle_ms` from `MotionSettings` (speed, snappy, off, reduced).
- Instant path when `settings.instant()`; `done` hook still fires. Frame stats logged
  (mean/p95/max, slow >20 ms), watchdog fallback timer.
- Users: panel/task strip, launcher, menus, classic menu, start pane, toast (opacity "o"),
  slide.rs. Call sites pass ad hoc `base_ms` literals.
- Gap: two models (Tween+Easing in compositor, Spring in shell) with separate numbers; a
  menu and a window popup of the same kind can feel different.

### TilesDE (`shell/qml`)
- Tokens in `Theme.qml`: `motionScale` (off 0, low 0.6, normal 1, high 1.4), `pressMs` 140,
  `turnstileMs` 320, `flipMs` 650/1000, `flipPeriodMs` 5000/8000, `flipStaggerMs` 140,
  `animMs` 280. `Theme.reduceMotion` = system flag or `Personalise.motion === "off"`.
- Tile press: tilt <= tiltMax plus scale 0.96, SpringAnimation spring 5 / damping 0.4.
- Tile flip: rotation about X, staggered by index, per-tile loop timer (many concurrent
  animations on a full start screen).
- Start screen: turnstile enter/exit per cell (`enterP`, delay row*70+col*35 capped 600),
  edit-mode scale 0.88/1.04, Behavior on x/y/width/height 200-240 ms, scroll 320 ms InOutCubic.
- Main.qml: app open `appP` spring 4.2/0.38, lock `lockP` 3.2/0.32, shade `shadeP` 3.4,
  switcher `switchP`; home scales content `1 - 0.06*appP` (full-surface scale, expensive).
- Others: keyboard `p` 240 in / 180 out; Shade height 200 + opacity; Pivot snap turnstileMs;
  AllApps row turnstile (28 ms stagger, cap 12) and scroll 380; group header chevron 160;
  AlphaJump 160/90; lock screen idle bounce every 6 s.
- Gaps: literals (120/150/160/180/200/240/280/320/380) instead of tokens; several Behaviors
  not gated on reduceMotion (AllApps scale, AlphaJump use `duration: 0` form, fine but
  inconsistent); no search and list-reveal tokens; per-tile flip timers not budgeted.

## 3. Shared motion tokens

Base values before the user speed/intensity multiplier. Both shells read the same table;
Glaze in `glaze-design::motion` (Rust consts), TilesDE in `Theme.qml`.

| Token | ms | Curve | Use |
|---|---|---|---|
| `instant` | 0 | none | reduced motion, hover colour snap |
| `fast` | 90 | OutCubic | hover, press release, focus ring |
| `quick` | 140 | OutCubic | popup/menu open, press, toggle |
| `base` | 200 | OutCubic | open, reveal, sheet, group collapse |
| `slow` | 280 | InOutCubic | workspace/page moves, panel slide |
| `deliberate` | 350 | InOutCubic | logout, lock, app launch from tile |
| `exit` | 0.8 x enter | InQuad (Glaze), InCubic (Tiles) | all dismissals |

Distances: `slide-sm` 6 px (menus, toasts), `slide-md` 10 px (windows), `slide-lg` 16 px
(High intensity). Stagger: 28 ms/item (lists, cap 12 items = 336 ms), 35 ms/col + 70 ms/row
(tile grid, cap 600 ms). Spring (Tiles only): spring 4-5, damping 0.38-0.4 for press and
app open; Glaze keeps critically damped.

Intensity scaling (both): off 0, low 0.6, normal 1, high 1.25 (Tiles currently 1.4: align to 1.25).

## 4. Per-surface specs

Format: duration, easing, properties, constraint.

### Glaze

| Surface | Spec |
|---|---|
| Window open | 180 ms OutCubic; opacity 0 -> 1, translateY +10 -> 0. No scale (High scale only on GPU renderer). |
| Window close | 230 -> 180 ms InQuad; opacity 1 -> 0, translateY 0 -> +10. Keep surface texture; remove after last frame. |
| Minimize | 220 ms InOutCubic; translate toward taskbar button plus opacity to 0. No scale; if a shrink is wanted, GPU renderer only. |
| Restore | 220 ms OutCubic; reverse of minimize. |
| Maximize/unmaximize | 220 ms InOutCubic; animate frame rect position only; client resizes once at end (configure at t=0 and t=end, not per frame). Crossfade old/new buffer 120 ms. |
| Move/snap | 220 ms InOutCubic; translate; drag itself is 1:1 with no tween. |
| Workspace switch | 260 ms InOutCubic; horizontal translate of whole workspace layer by output width, outgoing fades to 0.6 opacity. Windows of other workspaces not rendered. |
| Focus change | 160 ms OutCubic; border/titlebar colour only. |
| Alt-tab switcher | 120 ms OutCubic open; opacity only; selection highlight 90 ms. |
| Launcher/start pane | open 140 ms OutCubic opacity + translateY 6 -> 0; close 120 ms InQuad. Result list reveal: 28 ms stagger, opacity only, cap 12 rows. Search filtering: no animation on rows, only 90 ms fade on the empty state. |
| Menus/context/classic menu | 140 ms open, 120 ms close; opacity + translateY `popup_slide` (6 px). Submenu 140 ms, 150 ms hover-intent delay, no animation delay stacking. |
| Toasts | enter 200 ms OutCubic opacity + translate 6 px from edge; dwell 4-5 s; exit 160 ms InQuad. Stack reflow 200 ms translate. |
| Panel | auto-hide slide 280 ms InOutCubic translate; task button add/remove 200 ms width via Spring (existing `Animator`), icon opacity. Hover 90 ms colour. Popovers (tray, calendar) 140 ms as menus. |
| Lock/unlock | 220 ms; opacity crossfade of lock layer, no blur animation (blur static pre-rendered). |
| Logout | 350 ms; global opacity to 0 on a black layer. |

Perf: one tick source (compositor frame callback); damage limited to the animated rect; cap
simultaneous window animations at 4 (extra ones jump); skip all window animation while a drag
runs; software-renderer flag (`llvmpipe`) forces translate+opacity only and drops High scale.

### TilesDE

| Surface | Spec |
|---|---|
| Tile press | tilt <= tiltMax deg plus scale 0.96, spring 5 / damping 0.4 (keep). Release springs back. Tiles are small; scale is acceptable. Disable tilt on low-power or `quality 2`. |
| Tile flip | `flipMs` 650/1000 InOutCubic rotation X; max 6 tiles flipping at once (global scheduler, not per-tile timers); pause while scrolling, editing, app open, screen off. |
| Page transition (Pivot, home pager) | 280-320 ms OutCubic; contentX/translate and opacity of neighbours; no scale. |
| App open from tile | 350 ms; tile-region window rect translate plus opacity (turn-style: home content translate -Y 24 px and fade, not `scale 0.94`); spring 4.2/0.38 kept for drag-driven `appP`. Replace home `scale 1-0.06*appP` with opacity 1 -> 0.4 plus translate. |
| App close to tile | 280 ms InCubic; reverse, no spring overshoot. |
| Switcher | 220 ms OutCubic; cards translate and opacity; press scale 0.96 (140 ms). |
| Shade (pull-down) | drag-driven `shadeP`, release spring 3.4/0.34; tile toggles 140 ms; height anim 200 ms replaced by translateY of fixed-height panel (avoid relayout). |
| Lock screen | drag-driven `lockP`; release spring 3.2/0.32; idle bounce every 6 s kept but disabled when reduced; wallpaper static. |
| Keyboard | 240 ms OutCubic up, 180 ms InCubic down; translateY only, content not resized per frame (resize app once at end). |
| Search | field expand 200 ms OutCubic opacity+translate; result rows reveal 28 ms stagger cap 12, opacity+translateY 12 px, 200 ms; filtering no animation. |
| List reveal (All apps) | row turnstile `turnstileMs` 320, delay min(index,12)*28; rotation X ok per row but limit to visible rows only. |
| Group collapse | 200 ms OutCubic; chevron 160 ms rotation; tiles beneath translate (no height tween per frame: set final height, translate children). |
| Edit mode | wiggle none; scale 0.88/1.04 over 180 ms OutCubic; reflow 200-240 ms translate. |
| Toggle/button | 140 ms press, 90 ms release. |

Perf: no `layer.enabled` + animated scale on full-screen items; `ShaderEffectSource.scheduleUpdate`
(Main.qml bsrc, 1500 ms) stays paused during any animation; use `Behavior` with `enabled:
!Theme.reduceMotion` consistently; prefer `Translate`/`opacity` properties over `x/y` with
layout-bound anchors; cap concurrent animations (flips <= 6, stagger caps above).

## 5. Reduced motion

- One predicate per shell: Glaze `MotionSettings::instant()` (off OR reduced); TilesDE
  `Theme.reduceMotion`. Sources: system flag (GTK `gtk-enable-animations`, Tiles
  `Services.reduceMotion`), config `appearance.reduced_motion`, `motion.style = off`.
- Reduced = every duration 0, no loops (flip, lock bounce, music bars), no parallax, no
  stagger; state change still occurs and `done` hooks still fire.
- Optional "reduced" middle ground (not instant): opacity-only crossfade 90-120 ms, no
  translate. Tiles `low` (0.6) already approximates this; Glaze `low` keeps 5 px slide.
- Add the missing gates in Tiles (AllApps and AlphaJump Behaviors written as `duration:
  reduceMotion ? 0 : n` should become `enabled:` for consistency).
- Settings UI: one switch in Glaze Settings and Tiles Settings, live apply, no restart.

## 6. Implementation order

Each task is independent and ships alone. Verify on the VM: log frame stats (Glaze already
prints p95/max) and compare reduced motion on/off.

### Glaze
1. G1 Shared tokens: add `glaze-design::motion::tokens` (table above), make compositor
   `durations` and shell `base_ms` literals read them. No behaviour change. (small)
2. G2 Reduced-motion audit: confirm every `Animator::animate` and compositor tween path
   returns instantly under `instant()`; add tests (`Tween` zero duration, intensity Off). (small)
3. G3 Exit shortening and close/open polish: close 230 -> 180, exit curve InQuad, cap 4
   concurrent window anims. (small)
4. G4 Maximize/restore crossfade with single configure. (medium)
5. G5 Menus, launcher, toasts to token specs (translate+opacity, stagger cap). (medium)
6. G6 Workspace switch: horizontal layer translate with outgoing fade. (medium)
7. G7 Panel auto-hide and popovers to tokens; frame-stats regression check. (small)

### TilesDE
1. T1 Tokens in `Theme.qml`: replace literals 120-380 with token names; align high
   scale 1.4 -> 1.25. No behaviour change. (small)
2. T2 Reduced-motion gating sweep (AllApps, AlphaJump, Shade drag, lock bounce). (small)
3. T3 Flip scheduler: global budget (<= 6 concurrent), pause on scroll/edit/app open. (medium)
4. T4 App open/close: replace home full-surface `scale` with translate+opacity;
   close uses InCubic, no overshoot. (medium)
5. T5 Shade and keyboard: translateY instead of height/resize per frame. (medium)
6. T6 Search and list reveals: stagger tokens, visible rows only. (small)
7. T7 Group collapse via translate with final-height commit. (small)

Order across repos: G1/T1 first, G2/T2 next, then user-visible polish (G3, T3, T4, G5), then
the heavier ones (G4, G6, T5).

## 7. Acceptance

- p95 frame interval <= 20 ms on VM for window open/close and menus (Glaze log line).
- No stale pixels after open, close, minimize, workspace switch on llvmpipe.
- With reduced motion on: no animation, no timers running (idle CPU unchanged).
- Same token yields same duration in both shells (table diff in CI is optional).
