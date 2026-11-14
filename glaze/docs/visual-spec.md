# Glaze visual specification

> Status: **authoritative visual contract** for Glaze. Values here are *derived*
> from the implementation and must match it.
>
> Single source of truth: `crates/glaze-common/src/theme.rs`
> (`ThemeTokens`, `ThemeMetrics`, `Palette`) and `crates/glaze-common/src/config.rs`.
> This document, `themes/*.toml` and `themes/README.md` are **reference exports
> for designers, packagers and reviewers** — never a second runtime source of
> truth. If code and document disagree, the code wins and this document is wrong.
>
> Related: `GLAZE.md` §3 (design), §3.2 (themes), §3.3 (claymorphism), §3.4
> (tokens and quality gates), §6.4 (motion), §6.5 (accessibility);
> `docs/architecture.md`; `crates/glaze-common/src/theme.rs` tests.
>
> **Mirror date:** the values below were re-read from `theme.rs` and re-verified
> on 2026-11-11. `theme.rs` is under active development by other Glaze work; the
> contrasts in §4.3 and the exports in `themes/` are cheap to re-check (see
> `themes/README.md` §5 "Keeping exports in sync").

## 1. Intent

Glaze is a real native Fedora Wayland desktop environment whose structure evolves
from the Windows 2000 interaction model (clear taskbar, start button, app
hierarchy, window buttons, dense but legible affordances, keyboard efficiency)
into a modern, restrained, claymorphic surface. Windows 2000 is a *structural
reference*, never an asset source: no Microsoft artwork, icons, fonts or
trademarks appear anywhere in Glaze.

One shell, two deliberate themes (`GLAZE.md` §3.2):

| Theme | Role | Character |
|---|---|---|
| **Glaze Clay** (default) | the modern face | soft volume, generous spacing, rounded controls, calm indigo-blue accent |
| **Glaze Classic** (heritage) | the evolved 2000-era reference | squarer, tighter metrics, flat grey hierarchy, crisp bevel cues |

Neither theme is a colour-inverted hack of the other, and neither is a GTK theme
wrapper: both are full token sets consumed by the shell, the settings app and
(eventually) compositor decorations.

## 2. Claymorphism, rigorously defined

Claymorphism in Glaze means **gentle dimensional volume** (`GLAZE.md` §3.3):

- slightly inflated controls,
- diffuse outside shadows,
- faint inset highlights,
- extremely light material separation,
- compositional hierarchy (who is on top, what is active).

Positive rules:

1. Mild depth on **controls, menus, cards and active elements only**; large
   content regions stay mostly flat.
2. Contrast and boundaries must remain **readable with shadows disabled**.
3. Prefer **subtle layered borders and highlights** over giant shadow stacks.
4. **No shadow on every nested surface**; avoid double/triple nesting of raised
   cards.
5. Design light and dark as two real palettes; neither is an inverse-colour hack.
6. Animation must feel physically coherent, never elastic/jiggly or slow.
7. Support reduced motion, keyboard focus rings, high DPI and mixed scaling.

### 2.1 The explicit "do NOT" list

Claymorphism here **does not** mean, and Glaze must never ship:

- blur everywhere (glassmorphism) as the primary material;
- extruded toy/3D "plasticine" shapes;
- loud glossy skeuomorphism (gradients, bevels, specular glare);
- illegible low-contrast neumorphism (white-on-white / grey-on-grey softness);
- a chunky, child-oriented, oversized-control interface;
- shadow stacks on every nested widget;
- colour-inverted dark mode;
- depth that hides state (active vs pinned vs idle must stay obvious);
- decoration that breaks input correctness or pushes layout around.

## 3. Tokens

All numeric values below are literal copies of `ThemeMetrics::clay()` and
`ThemeMetrics::classic()` in `crates/glaze-common/src/theme.rs`. They are
machine-logical pixels (GTK logical px at scale 1.0) and are *starting values*
that are adjusted by testing, not by taste (`GLAZE.md` §3.4).

### 3.1 Metrics — Glaze Clay vs Glaze Classic

| Token | Glaze Clay | Glaze Classic | §3.4 target band (Clay / Classic) | In band |
|---|---|---|---|---|
| `panel_height` | **52** | **36** | 48–54 / 32–38 | yes |
| `control_radius` | **10** | **3** | 9–12 / 2–4 | yes |
| `menu_radius` | **16** | **4** | 14–18 / 3–5 | yes |
| `window_radius` | **14** | **5** | 12–16 / 4–6 | yes |
| `overlay_radius` | **20** | **5** | 18–22 / 4–6 | yes |
| `control_height` | **36** | **28** | 34–40 / 26–32 | yes |
| `panel_padding` | **10** | **4** | (supporting role) | n/a |
| `spacing` | **8** | **4** | (supporting role) | n/a |
| `font_size_pt` | **10.5** | **10.0** | (supporting role) | n/a |
| `motion_micro_ms` | **130** | **60** | 100–160 / 0–100 | yes |
| `motion_menu_ms` | **190** | **90** | 160–230 / 0–120 | yes |
| `motion_workspace_ms` | **260** | **140** | 220–320 / 0–160 | yes |

Metrics depend only on the **theme family**, not on light/dark. Dark mode never
changes a radius, a height or a duration.

`ThemeMetrics::scaled(factor)` multiplies the absolute px metrics by `factor`
(clamped to `>= 0.5`) and multiplies `font_size_pt`. It is the mechanism behind
`appearance.scale` / `appearance.font_scale` when the toolkit's own scaling is
not used; `glaze-settings` applies it to its own window today, and the shell is
expected to do the same for its surfaces.

### 3.2 Where each token is used

| Token | Applied to |
|---|---|
| `panel_height` | taskbar thickness (overridable by `panel.height` in config) |
| `control_radius` | start button, task buttons, clock, launcher search field, launcher entries, menu items |
| `menu_radius` | launcher surface, context menus |
| `window_radius` | Glaze-drawn window decorations / card surfaces |
| `overlay_radius` | floating overlays (layer-shell surfaces), larger popovers. **Not yet emitted** by `to_gtk_css`; reserved for the overlay surfaces in `GLAZE.md` §3.4 |
| `control_height` | minimum hit height of every interactive control |
| `panel_padding` | outer padding of the panel; also the padding `glaze-settings` uses for its pages and cards |
| `spacing` | gap between taskbar groups; also the gap `glaze-settings` uses between rows |
| `font_size_pt` | base font size emitted for the panel. Consumers may multiply it by `appearance.font_scale` / `appearance.scale` (glaze-settings does) |
| `motion_*_ms` | transition durations in the emitted GTK CSS (`0ms` under reduced motion) |

### 3.3 Shadow and highlight behaviour

Shadows are emitted from role colours only — never a raw literal
(`crates/glaze-common/src/theme.rs`, test `css_uses_tokens_not_raw_hex`).

| Surface | Glaze Clay | Glaze Classic |
|---|---|---|
| Panel | `0 1px 3px @glaze_shadow` (soft, low) | same rule, but `shadow` is a crisp near-black at 24% alpha |
| Launcher / overlay | `0 8px 28px @glaze_shadow, inset 0 1px 0 @glaze_highlight` | same rule, tight alpha, reads as a hard bevel edge |
| Generic menu | `0 6px 20px @glaze_shadow` | same rule |
| Start button | `inset 0 1px 0 @glaze_highlight` | same rule, high-contrast highlight |
| Running task | `inset 0 1px 0 @glaze_highlight, 0 1px 2px @glaze_shadow` | same rule |
| Large content regions | **none** | **none** |

Rules that follow from the claymorphism definition:

- Exactly **one** elevation step is allowed between a surface and its parent.
  `.glaze_launcher` (raised) inside `.glaze_panel` (surface) is the deepest
  supported nesting.
- The inset highlight is a 1px top edge, never a full gradient.
- Shadows use the `shadow` role (translucent, 8-bit alpha), so they blend with
  whatever is behind them; they must never be replaced with a hard grey line.
- No `box-shadow` on `.glaze-desktop` or `.glaze-desktop-icon` (hover/selection
  use `alpha(@glaze_accent, 0.15 / 0.28)` fills instead).

### 3.4 Motion

| Interaction | Clay | Classic | Curve |
|---|---|---|---|
| Micro (hover, press, task state) | 130 ms | 60 ms | `ease` |
| Menu / overlay show-hide | 190 ms | 90 ms | `ease` |
| Workspace switch (future compositor work) | 260 ms | 140 ms | `ease` |

Constraints (`GLAZE.md` §6.4):

- Animations must be **reversible without glitches during rapid repeated input**
  (menu spam-toggling is a listed test).
- No animation may block the input loop; use GPU-friendly transform/opacity.
- Menus animate **from the invoking affordance** (origin continuity), not from
  the screen centre.
- `reduced_motion = true` emits `0ms` transitions everywhere
  (`CssOptions { reduced_motion: true, .. }`), and the UI must remain fully
  usable and legible with all motion off.

## 4. Colour

### 4.1 Named roles

Colours exist **only** as named roles. Adding a role to `ThemeRole::ALL` in
`glaze-common` is the only supported way to add a colour to the design system;
components never contain a raw hex literal (`GLAZE.md` §3.4).

| Role @define-color | Meaning | Must be used for |
|---|---|---|
| `glaze_canvas` | backdrop | desktop background, settings window background |
| `glaze_surface` | default container | panel/taskbar, cards |
| `glaze_surface_raised` | one step up | launcher, menus, running task button |
| `glaze_surface_sunken` | one step down | search field, hover fill, inset wells |
| `glaze_border` | separation | 1px outlines, separators |
| `glaze_text_primary` | main text | labels, clock, active task label |
| `glaze_text_secondary` | supporting text | pinned/idle task labels, hints, group titles |
| `glaze_accent` | brand/selection | focused task, selected entry, active start button |
| `glaze_accent_text` | text on accent | any text/icon drawn on `accent` |
| `glaze_focus` | keyboard focus | 2px focus outline, offset 1px |
| `glaze_danger` | destructive/error | shutdown, error text |
| `glaze_success` | positive | connected, charging |
| `glaze_warning` | caution | low battery, degraded state |
| `glaze_shadow` | depth | all `box-shadow` outside edges |
| `glaze_highlight` | inset light | 1px inset top edge of raised controls |

Status roles (`danger`/`success`/`warning`) are **iconographic**: they are used
for icons, glyphs, dots and short status text, never as the sole carrier of a
meaning, and never as long-form body text.

### 4.2 Palettes

Values are the exact `rgb()/rgba()` literals in `crates/glaze-common/src/theme.rs`,
shown as `#rrggbb` (or `#rrggbbaa` when alpha < 255). Machine-readable copies
live in `themes/glaze-clay.toml` and `themes/glaze-classic.toml`.

**Glaze Clay — light**

| Role | Value | | Role | Value |
|---|---|---|---|---|
| canvas | `#eef1f6` | | accent | `#3d5ae0` |
| surface | `#f7f9fc` | | accent_text | `#ffffff` |
| surface_raised | `#ffffff` | | focus | `#2c46c8` |
| surface_sunken | `#e4e8ef` | | danger | `#bf3b3b` |
| border | `#d3d9e3` | | success | `#1e7a50` |
| text_primary | `#1e2430` | | warning | `#c98a1e` |
| text_secondary | `#5a6472` | | shadow | `#0a162833` |
| | | | highlight | `#ffffffc8` |

**Glaze Clay — dark**

| Role | Value | | Role | Value |
|---|---|---|---|---|
| canvas | `#14171c` | | accent | `#7c93ff` |
| surface | `#1c2027` | | accent_text | `#0e1116` |
| surface_raised | `#232833` | | focus | `#9db0ff` |
| surface_sunken | `#101216` | | danger | `#e26a6a` |
| border | `#313846` | | success | `#4fc08d` |
| text_primary | `#edf0f5` | | warning | `#e0b35a` |
| text_secondary | `#a2abba` | | shadow | `#00000078` |
| | | | highlight | `#ffffff18` |

**Glaze Classic — light**

| Role | Value | | Role | Value |
|---|---|---|---|---|
| canvas | `#d4d0c8` | | accent | `#0a246a` |
| surface | `#d4d0c8` | | accent_text | `#ffffff` |
| surface_raised | `#eglaze8` | | focus | `#316ac5` |
| surface_sunken | `#bfbbb2` | | danger | `#a80000` |
| border | `#808080` | | success | `#006400` |
| text_primary | `#000000` | | warning | `#806000` |
| text_secondary | `#404040` | | shadow | `#0000003c` |
| | | | highlight | `#ffffffe6` |

**Glaze Classic — dark**

| Role | Value | | Role | Value |
|---|---|---|---|---|
| canvas | `#2b2b2b` | | accent | `#3b5ba5` |
| surface | `#333333` | | accent_text | `#ffffff` |
| surface_raised | `#3d3d3d` | | focus | `#6c8fd6` |
| surface_sunken | `#202020` | | danger | `#f09a9a` |
| border | `#6e6e6e` | | success | `#6fcf6f` |
| text_primary | `#e6e6e6` | | warning | `#e0c070` |
| text_secondary | `#a8a8a8` | | shadow | `#00000096` |
| | | | highlight | `#ffffff28` |

### 4.3 Measured contrast (WCAG 2.2)

Computed with the same relative-luminance maths as
`glaze-common::color::Color::contrast_ratio`. These numbers were produced by a
standalone audit program that re-reads the literal palette values above from
`theme.rs` — they are measurements, not estimates.

| Pair | Clay light | Clay dark | Classic light | Classic dark | Target |
|---|---|---|---|---|---|
| text_primary / surface | 14.75 | 14.30 | 13.66 | 10.12 | ≥ 4.5 (AA) |
| text_primary / canvas | 13.74 | 15.72 | 13.66 | 11.34 | ≥ 4.5 (AA) |
| text_primary / surface_raised | 15.55 | 12.92 | 17.95 | 8.70 | ≥ 4.5 (AA) |
| text_secondary / surface | 5.69 | 7.06 | 6.74 | 5.31 | ≥ 4.5 (AA, aim) |
| text_secondary / surface_raised | 6.00 | 6.38 | 8.86 | 4.57 | ≥ 4.5 (AA, aim) |
| accent_text / accent | 5.62 | 6.73 | 14.25 | 6.51 | ≥ 4.5 (AA) |
| focus / surface | 7.07 | 7.85 | 3.42 | 3.93 | ≥ 3.0 (UI) |
| danger / surface | 5.10 | 5.06 | 5.12 | 5.89 | ≥ 3.0 (UI) |
| success / surface | 5.03 | 7.20 | 4.84 | 6.52 | ≥ 3.0 (UI) |
| warning / surface | 2.79 | 8.38 | 3.80 | 7.19 | ≥ 3.0 (UI) |
| border / surface | 1.34 | 1.39 | 2.57 | 2.48 | decorative |

Two honest readings of this table:

1. **Enforced by test.** `glaze-common`'s test `shell_text_meets_wcag_aa` gates
   `text_primary/surface ≥ 4.5`, `text_secondary/surface ≥ 3.0` and
   `accent_text/accent ≥ 4.5` for all four combinations. Every one of those
   passes with margin (worst case: `accent_text/accent` on Clay light, 5.62).
   The status roles clear the 3.0 non-text threshold everywhere except Clay light
   `warning`.
2. **Not enforced, and must be respected by hand.** `border` is intentionally a
   low-contrast separation token (1.3–2.6:1); it may therefore *never* be the
   only indicator of a state or boundary — pair it with a fill, a shape or an
   icon. Clay light `warning` (2.79) sits just under the 3.0 non-text threshold
   on `surface`: use it for icons/glyphs on a raised or accent surface, or fix it
   with a coordinated `theme.rs` change plus an extended contrast test — never
   with an ad-hoc colour inside a component.

### 4.4 Light and dark are two palettes, never an inversion

- Light is the default (`ColorScheme::Light`), and Clay light is the default of
  the whole product (`ThemeTokens::default_clay()`).
- Dark mode has its own values: e.g. Clay's accent moves from `#3d5ae0` (light)
  to the lighter, lower-saturation `#7c93ff`, and `accent_text` flips to
  `#0e1116` rather than staying white. This is because a hill-climbing
  luminance/contrast optimisation against `#1c2027`, not a `255 - channel`
  transform. Never generate a scheme by inverting another.
- Canvas and surface are **not** the same colour in Clay (canvas `#eef1f6`,
  surface `#f7f9fc`, ratio 1.07:1) — separation is by hue/lightness, not by
  border alone. In Classic they *are* the same (`#d4d0c8`), which is deliberate
  heritage behaviour: separation there comes from the raised/sunken pair and the
  100%-opaque highlight bevel.
- Shadows must be re-authored per scheme: Clay light uses a blue-black shadow
  at 20% alpha, Clay dark uses pure black at 47%, Classic light 24%, Classic
  dark 59%. A single shadow value for both schemes is a defect.
- When the user picks a scheme, every surface, role and shadow changes together.
  Mixed schemes (light widget on dark panel) are a defect.

## 5. Visual quality gates

These are the `GLAZE.md` §3.4 gates, restated as a pass/fail checklist. A visual
change is not "done" until every applicable line is checked. Reproduce the
automated subset with `cargo test -p glaze-common` (theme + contrast tests).

### 5.1 Active vs pinned is unmistakable

| # | Check | How to verify |
|---|---|---|
| 1 | A **pinned but not running** task shows only its icon and `text_secondary` colour, no fill | inspect a task button with `.glaze-pinned` only |
| 2 | A **running** task additionally gets `surface_raised` background + a 1px inset highlight + a 2px drop shadow | `.glaze-running` rule |
| 3 | The **focused** task inverts to `accent` background with `accent_text` | `.glaze-focused` rule |
| 4 | The three states are distinguishable **with shadows disabled** and in **greyscale** | render with a flat stylesheet; the fill colour delta must still carry the state |

### 5.2 Legible at every scale

| # | Check | How to verify |
|---|---|---|
| 5 | No clipping or overlap at **100%, 125%, 150%, 200%** | run the shell nested at each `GDK_SCALE` / monitor scale; screenshot each |
| 6 | Long labels ellipsize rather than pushing the panel | set a 60-character window title, confirm panel does not reflow |
| 7 | Text remains AA-contrast on both light and dark backgrounds | §4.3 table + `shell_text_meets_wcag_aa` |
| 8 | Control hit height ≥ `control_height` at every scale | measure a task button under each scale |

### 5.3 Motion

| # | Check | How to verify |
|---|---|---|
| 9 | Menus/launcher **animate from the invoking affordance** (start button, clock), not from screen centre | open from each trigger; watch the origin |
| 10 | Launcher/menu does not "pop in instantly" | duration is `motion_menu_ms` (190 / 90 ms) |
| 11 | Rapid repeated open/close never leaves a stuck or half-drawn surface | spam the start button for ~10 s |
| 12 | Reduced motion turns every transition to `0ms` and stays usable | config `reduced_motion = true`; verify in the settings app live |
| 13 | No animation blocks input; no jiggly/elastic overshoot beyond the duration | interact during the animation |

### 5.4 Stability of layout

| # | Check | How to verify |
|---|---|---|
| 14 | **No layout jumps** when apps start or close | launch/close several apps; the task area grows/shrinks without shifting the clock off-anchor |
| 15 | **No layout jump when the clock changes** | use `font-feature-settings: "tnum"` (already in the token CSS) and observe the minute rollover |
| 16 | **No horizontal taskbar overflow** without intentional scrolling/grouping | open more windows than fit; the task area must group, scroll or ellipsize deliberately |
| 17 | Panel height is stable while content changes | compare panel height before/after a burst of launches |

### 5.5 Iconography

| # | Check | How to verify |
|---|---|---|
| 18 | Icons are **original or licensed** Glaze artwork — never Microsoft/Windows assets or third-party trademarks | `assets/LICENSE.md` provenance note |
| 19 | Icons render at **matching size** in one row (no 16px next to 24px) | fixed icon size supplied by the consumer, not by the SVG |
| 20 | Icons share **one optical weight**: 24×24 grid, 2px stroke, round caps/joins | `assets/icons/*.svg` all carry `stroke-width="2"` |
| 21 | Icons are **monochrome-capable** so the active/pinned/focused colours apply | symbolic assets, recoloured by the widget's foreground colour |

## 6. Accessibility (`GLAZE.md` §6.5)

Target: **WCAG 2.2 AA** for shell text and controls where applicable.

| Requirement | Glaze behaviour | Evidence / owner |
|---|---|---|
| Contrast of body/shell text | ≥ 4.5:1 measured, all four theme×scheme pairs | §4.3; `cargo test -p glaze-common shell_text_meets_wcag_aa` |
| Contrast of focus indicator and large text | ≥ 3.0:1 | §4.3 (`focus/surface` 3.42–7.85) |
| Every interactive control has a visible focus ring | 2px `outline: solid @glaze_focus; outline-offset: 1px`, emitted when `CssOptions { focus_rings: true }` | `to_gtk_css`, `:focus-visible` rules |
| Keyboard-first operation | Tab order follows visual order; Escape dismisses; arrow keys navigate launcher/menus; Enter activates | shell + settings; settings uses real GTK focus chains |
| Accessible names | Every icon-only control must set an accessible label (`update_property(&[gtk4::accessible::Property::Label(..)])` / `set_tooltip_text`) | shell/settings code review |
| Reduced motion | `appearance.reduced_motion` → `0ms` transitions; no essential information may be conveyed by motion alone | `theme.rs` test `reduced_motion_emits_zero_durations`; settings exposes the switch |
| Scalable UI without clipping | `appearance.scale` + `appearance.font_scale`; `ThemeMetrics::scaled()` (clamped ≥ 0.5); reflow instead of clip | settings Appearance page (live) |
| High DPI and mixed scaling | All metrics are logical px; the toolkit multiplies by the output scale. Never bake physical-pixel assumptions into the SVG or the CSS. Mixed-DPI multi-monitor is a P2 item (`GLAZE.md` §5) and must not be claimed until tested | token design; status file |
| High contrast | Dark and light are real palettes; status roles degrade to iconographic use (§4.3) | palettes + this check |
| No colour-only signalling | Active vs pinned differs in fill *and* colour; errors carry text/icon, not just red | §5.1, §4.1 |

Accessibility non-goals **for now**, to be recorded honestly in
`PROJECT_STATUS.md` rather than implied: screen-reader announcement testing, a
full high-contrast theme family, and mixed-DPI runtime validation are not yet
verified on Fedora.

## 7. Enforcement

| Mechanism | What it guarantees |
|---|---|
| `ThemeRole::ALL` + CSS emitter | every role is defined; components cannot reference an undefined colour |
| test `css_defines_every_role_color` | no surface silently loses a role |
| test `css_uses_tokens_not_raw_hex` | component CSS contains **no** raw hex — colour lives in roles only |
| test `shell_text_meets_wcag_aa` | the AA text gates above are locked in |
| test `clay_is_rounder_than_classic` | the two themes stay genuinely distinct (radius, height, motion) |
| test `reduced_motion_emits_zero_durations` | reduced motion really reaches the CSS |
| `themes/*.toml` | designers/packagers get the same numbers without reading Rust |
| this document | the "do NOT" list and the gates are reviewable in one place |

Because `glaze-common` is the only place colour and metric values exist
(`docs/architecture.md` §"Theme single-source"), a change to any number above is
a one-file change in `crates/glaze-common/src/theme.rs`, followed by regenerating
`themes/*.toml` and updating this document.



