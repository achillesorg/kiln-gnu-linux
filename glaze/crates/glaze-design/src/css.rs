//! GTK4 CSS emitter for [`ThemeTokens`].
//!
//! Component rules reference `@glaze_*` colour roles only; raw hex never
//! appears outside the `@define-color` header (tested). Every size, radius,
//! stroke, font, shadow, texture and transition timing comes from the tokens
//! or [`CssOptions`], so a theme file or the user config can change any of
//! them and the shell reloads the CSS live.

use crate::kinds::{ColorScheme, ThemeKind};
use crate::theme::{Shadow, ThemeTokens, MIN_TOUCH_TARGET_PX};

/// Options that alter CSS emission without changing the tokens themselves.
#[derive(Debug, Clone, PartialEq)]
pub struct CssOptions {
    /// Emit `0ms` transitions everywhere (Glaze or system reduced motion).
    pub reduced_motion: bool,
    /// Include focus-ring rules.
    pub focus_rings: bool,
    /// Multiplier on every transition duration (`1.0` = theme timing; the
    /// config's `motion.speed` 2.0 maps to 0.5 here).
    pub duration_scale: f64,
    /// Reduced effects: no paper texture and no shadows (flat fills only).
    pub reduced_effects: bool,
    /// URL of the pre-rendered, intensity-baked paper texture tile for
    /// surfaces (`file:///...png`), or `None` for plain fills.
    pub texture_url: Option<String>,
}

impl Default for CssOptions {
    fn default() -> Self {
        CssOptions {
            reduced_motion: false,
            focus_rings: true,
            duration_scale: 1.0,
            reduced_effects: false,
            texture_url: None,
        }
    }
}

/// CSS value for one shadow token with the theme's shadow colour.
fn shadow_css(s: &Shadow, color: &str) -> String {
    if s.is_none() {
        "none".into()
    } else {
        format!("{}px {}px {}px {}px {color}", s.x, s.y, s.blur, s.spread)
    }
}

/// Format a px float compactly (`1px`, `1.25px`).
fn px(v: f64) -> String {
    let r = (v * 100.0).round() / 100.0;
    if (r - r.round()).abs() < 1e-9 {
        format!("{}px", r.round() as i64)
    } else {
        format!("{r}px")
    }
}

impl ThemeTokens {
    /// Emit GTK4 CSS for a `GtkCssProvider` with default options.
    pub fn to_gtk_css(&self) -> String {
        self.to_gtk_css_with(CssOptions::default())
    }

    /// Emit GTK4 CSS with explicit options.
    pub fn to_gtk_css_with(&self, opts: CssOptions) -> String {
        self.to_gtk_css_full(opts, None)
    }

    /// Emit GTK4 CSS; `panel_texture` is a finer, fainter tile for the
    /// taskbar (`(url, tile_px)`, drawn at half size); `None` uses the
    /// surface texture there too.
    pub fn to_gtk_css_full(&self, opts: CssOptions, panel_texture: Option<(String, u32)>) -> String {
        let m = &self.metrics;
        let p = &self.palette;
        let ty = &self.typography;
        let scale = if opts.reduced_motion {
            0.0
        } else {
            opts.duration_scale.clamp(0.0, 8.0)
        };
        let dur = |ms: u32| ((ms as f64) * scale).round() as u32;
        let micro = dur(m.motion_micro_ms);
        let menu = dur(m.motion_menu_ms);
        let ease = "cubic-bezier(0.2, 0, 0, 1)";
        let dir = if self.scheme == ColorScheme::Dark {
            "dark"
        } else {
            "light"
        };
        let flat = self.kind == ThemeKind::Clay;
        let effects = !opts.reduced_effects;

        let shadow_color = if effects {
            self.shadows.color(p).to_css()
        } else {
            "transparent".into()
        };
        let sh = |s: &Shadow| {
            if effects {
                shadow_css(s, &shadow_color)
            } else {
                "none".into()
            }
        };
        let popover_sh = sh(&self.shadows.popover);
        let window_sh = sh(&self.shadows.window);
        let button_sh = sh(&self.shadows.button);
        // Classic keeps a bevel (inset highlight) on raised controls.
        let bevel = if flat {
            String::new()
        } else {
            "inset 1px 1px 0 @glaze_highlight, inset -1px -1px 0 @glaze_border".into()
        };
        // Clay "raised" is a hard 1 px paper edge under the button (Windows
        // 2000 structure, flat paper execution); "sunken" a faint inner top
        // edge. No blur anywhere.
        let raised_sh = match (bevel.is_empty(), button_sh.as_str()) {
            (true, "none") if effects => "0 1px 0 0 @glaze_border".to_string(),
            (true, s) => s.to_string(),
            (false, "none") => bevel.clone(),
            (false, s) => format!("{bevel}, {s}"),
        };
        let sunken_sh = if flat {
            "inset 0 1px 0 0 alpha(@glaze_ink, 0.10)".to_string()
        } else {
            "inset 1px 1px 0 @glaze_border, inset -1px -1px 0 @glaze_highlight".into()
        };
        let panel_texture = match (&panel_texture, &opts.texture_url, effects) {
            (Some((url, tile)), _, true) => format!(
                "url(\"{url}\");\n  background-size: {}px {}px",
                (tile / 2).max(1),
                (tile / 2).max(1)
            ),
            (None, Some(url), true) => format!("url(\"{url}\")"),
            _ => "none".into(),
        };
        let dark = self.scheme == ColorScheme::Dark;
        // Wells (tray + clock, float/tile toggle) sit clearly below the bar
        // value: a darker paper with a firmer hairline, no shadow.
        let well_bg = if dark {
            "mix(@glaze_panel, black, 0.30)"
        } else {
            "mix(@glaze_panel, @glaze_ink, 0.07)"
        };
        let well_border = if dark {
            "mix(@glaze_border, @glaze_ink, 0.12)"
        } else {
            "mix(@glaze_border, @glaze_ink, 0.10)"
        };
        // Bar top edge: warm, low-opacity ink line (not a cool/bright rule).
        let top_rule = if dark {
            "alpha(@glaze_ink, 0.16)"
        } else {
            "alpha(@glaze_ink, 0.14)"
        };
        let texture = match (&opts.texture_url, effects) {
            (Some(url), true) => format!("url(\"{url}\")"),
            _ => "none".into(),
        };

        let sw = px(m.stroke_width);
        let ui = self.ui_font_stack();
        let display = self.display_font_stack();
        let mono = ty.mono_stack();
        let touch = m.touch_target.max(MIN_TOUCH_TARGET_PX);
        let h = m.control_height;
        let r = m.control_radius;
        let mr = m.menu_radius;
        let fs = m.font_size_pt;
        let weight = ty.emphasis_weight;
        let tw = m.task_button_width;
        let iw = m.task_icon_width;
        let ul = m.underline_height;

        let mut css = String::with_capacity(16 * 1024);
        css.push_str("/* Glaze theme — generated by glaze-design. Do not edit by hand. */\n");
        css.push_str(&format!(
            "/* theme={} scheme={dir} texture={} effects={} */\n",
            self.kind.slug(),
            if texture == "none" { "none" } else { "on" },
            if effects { "full" } else { "reduced" },
        ));
        for (role, color) in p.iter() {
            css.push_str(&format!(
                "@define-color glaze_{} {};\n",
                role.key(),
                color.to_css()
            ));
        }

        // ---- Panel / taskbar -------------------------------------------------
        css.push_str(&format!(
            "\n.glaze-panel {{\n  background-color: @glaze_panel;\n  background-image: {panel_texture};\n  \
             background-repeat: repeat;\n  color: @glaze_text_primary;\n  border: 0 solid @glaze_border;\n  \
             font-family: {ui};\n  font-size: {fs:.2}pt;\n  box-shadow: none;\n}}\n\
             .glaze-panel.glaze-edge-bottom {{ border-top-width: {sw}; }}\n\
             .glaze-panel {{ border-color: {top_rule}; }}\n\
             .glaze-panel.glaze-edge-top {{ border-bottom-width: {sw}; }}\n\
             .glaze-panel.glaze-edge-left {{ border-right-width: {sw}; }}\n\
             .glaze-panel.glaze-edge-right {{ border-left-width: {sw}; }}\n\
             .glaze-panel label {{ color: inherit; }}\n"
        ));
        // Reset Adwaita button chrome inside Glaze surfaces.
        css.push_str(
            ".glaze-panel button, .glaze-popover button {\n  background: none;\n  border: none;\n  \
             box-shadow: none;\n  text-shadow: none;\n  -gtk-icon-shadow: none;\n  outline: none;\n  \
             min-height: 0;\n  min-width: 0;\n  padding: 0;\n  color: inherit;\n}\n",
        );
        css.push_str(&format!(
            ".glaze-section-sep {{ background-color: @glaze_border; min-width: {sw}; min-height: {sw}; margin: 10px 4px; }}\n"
        ));

        // Start button: terracotta paper tab, serif word.
        css.push_str(&format!(
            ".glaze-panel button.glaze-start-button {{\n  min-height: {h}px;\n  border-radius: {r}px;\n  \
             padding: 0 10px 0 9px;\n  background-color: @glaze_accent;\n  color: @glaze_accent_text;\n  \
             box-shadow: {raised_sh};\n  transition: background-color {micro}ms {ease}, box-shadow {micro}ms {ease};\n}}\n\
             .glaze-panel button.glaze-start-button:hover {{ background-color: shade(@glaze_accent, 1.06); }}\n\
             .glaze-panel button.glaze-start-button:active,\n\
             .glaze-panel button.glaze-start-button.glaze-open {{ background-color: shade(@glaze_accent, 0.92); box-shadow: {sunken_sh}; }}\n\
             .glaze-start-label {{ font-family: {sf}; font-weight: {sw8}; font-size: {spx}; letter-spacing: -0.01em; \
             color: alpha(@glaze_accent_text, 0.92); }}\n{start_dark}",
            start_dark = if self.scheme == ColorScheme::Dark && flat {
                ".glaze-panel button.glaze-start-button { background-color: @glaze_accent_soft; \
                 box-shadow: inset 0 0 0 1px alpha(@glaze_accent, 0.55); }\n\
                 .glaze-panel button.glaze-start-button:hover { background-color: mix(@glaze_accent_soft, @glaze_accent, 0.25); }\n\
                 .glaze-panel button.glaze-start-button:active, .glaze-panel button.glaze-start-button.glaze-open \
                 { background-color: mix(@glaze_accent_soft, @glaze_accent, 0.35); }\n\
                 .glaze-panel .glaze-start-label { color: @glaze_accent_soft_text; }\n"
            } else {
                ""
            },
            sf = if ty.start_label_serif { display.clone() } else { ui.clone() },
            sw8 = ty.start_label_weight,
            spx = px(ty.start_label_px),
        ));

        // Pinned launchers: icon-only, frameless.
        css.push_str(&format!(
            ".glaze-panel button.glaze-pin-button {{\n  min-width: {h}px;\n  min-height: {h}px;\n  \
             border-radius: {r}px;\n  color: @glaze_ink;\n  transition: background-color {micro}ms {ease};\n}}\n\
             .glaze-panel button.glaze-pin-button:hover {{ background-color: @glaze_surface_sunken; }}\n\
             .glaze-panel button.glaze-pin-button:active {{ background-color: @glaze_border; }}\n\
             .glaze-pins {{ background-color: alpha(@glaze_ink, 0.035); border: {sw} solid alpha(@glaze_ink, 0.08); \
             border-radius: {r}px; padding: 1px; }}\n\
             .glaze-panel button.glaze-pin-button.glaze-running {{ box-shadow: inset 0 -2px 0 0 alpha(@glaze_accent, 0.85); }}\n"
        ));

        // Task buttons (running windows). Idle = raised paper tab; hover =
        // one step lighter; active = faint terracotta tint + terracotta
        // underline (the main cue) + semibold label; minimised = no sheet,
        // dimmed content. Same rules in light and dark.
        css.push_str(&format!(
            ".glaze-panel button.glaze-task-button {{\n  min-height: {h}px;\n  border-radius: {r}px;\n  \
             padding: 0 8px 0 9px;\n  background-color: @glaze_surface_raised;\n  color: @glaze_text_primary;\n  \
             border: {sw} solid @glaze_border;\n  box-shadow: {raised_sh};\n  \
             transition: background-color {micro}ms {ease}, border-color {micro}ms {ease}, opacity {micro}ms {ease};\n}}\n\
             .glaze-panel button.glaze-task-button:hover {{ background-color: {hover_bg}; }}\n\
             .glaze-panel button.glaze-task-button:active {{ background-color: @glaze_surface_sunken; box-shadow: {sunken_sh}; }}\n\
             .glaze-panel button.glaze-task-button.glaze-active {{ background-color: mix(@glaze_surface_raised, @glaze_accent, 0.10); \
             border-color: mix(@glaze_border, @glaze_accent, 0.45); box-shadow: none; }}\n\
             .glaze-panel button.glaze-task-button.glaze-active:hover {{ background-color: mix(@glaze_surface_raised, @glaze_accent, 0.16); }}\n\
             .glaze-panel button.glaze-task-button.glaze-active .glaze-task-label {{ font-weight: {weight}; }}\n\
             .glaze-panel button.glaze-task-button.glaze-minimized {{ background-color: transparent; border-color: alpha(@glaze_border, 0.7); \
             border-style: dashed; box-shadow: none; }}\n\
             .glaze-panel button.glaze-task-button.glaze-minimized > overlay > box {{ opacity: 0.6; }}\n\
             .glaze-panel button.glaze-task-button.glaze-minimized:hover {{ background-color: alpha(@glaze_ink, 0.05); }}\n\
             .glaze-panel button.glaze-task-button.glaze-urgent {{ border-color: @glaze_accent; }}\n\
             .glaze-task-label {{ color: @glaze_text_primary; }}\n\
             .glaze-underline {{ background-color: @glaze_accent; min-height: {ul}px; border-radius: 1px; margin-bottom: -1px; }}\n\
             .glaze-count-pill {{\n  background-color: @glaze_accent_soft;\n  color: @glaze_accent_soft_text;\n  \
             border-radius: 8px;\n  min-width: 20px;\n  min-height: 16px;\n  font-size: {pill:.2}pt;\n  font-weight: 600;\n}}\n\
             .glaze-task-glyph {{ color: @glaze_ink; }}\n",
            pill = ty.small_size_pt + 0.5,
            hover_bg = if self.scheme == ColorScheme::Dark { "shade(@glaze_surface_raised, 1.12)" } else { "@glaze_surface" },
        ));
        css.push_str(&format!(
            "/* layout hints for the shell: task={tw}px icon={iw}px */\n"
        ));

        // Float/tile segmented toggle.
        css.push_str(&format!(
            ".glaze-segmented {{\n  background-color: {well_bg};\n  border: {sw} solid {well_border};\n  \
             border-radius: {r}px;\n  padding: 2px;\n}}\n\
             .glaze-panel button.glaze-segment {{\n  min-width: 28px;\n  min-height: 20px;\n  border-radius: {rs}px;\n  \
             color: @glaze_ink;\n  opacity: 0.6;\n  transition: background-color {micro}ms {ease}, opacity {micro}ms {ease};\n}}\n\
             .glaze-panel button.glaze-segment:hover {{ opacity: 1; }}\n\
             .glaze-panel button.glaze-segment.glaze-selected {{ background-color: @glaze_surface_raised; opacity: 1; \
             box-shadow: 0 0 0 {sw} @glaze_border; }}\n",
            rs = (r - 1).max(0),
        ));

        // Tray, bell dot, clock, show-desktop.
        css.push_str(&format!(
            ".glaze-tray {{ color: @glaze_ink; }}\n\
             .glaze-panel button.glaze-tray-button {{ min-width: 26px; min-height: {h}px; border-radius: {r}px; color: @glaze_ink; \
             transition: background-color {micro}ms {ease}; }}\n\
             .glaze-panel button.glaze-tray-button:hover {{ background-color: @glaze_surface_sunken; }}\n\
             .glaze-bell-dot {{ background-color: @glaze_accent; border-radius: 3px; min-width: 6px; min-height: 6px; }}\n\
             .glaze-panel button.glaze-clock {{ min-height: {ch}px; padding: 0 8px 0 10px; margin: 2px 0; border-radius: {rs}px; \
             transition: background-color {micro}ms {ease}; }}\n\
             .glaze-panel button.glaze-clock:hover {{ background-color: @glaze_surface_sunken; }}\n\
             .glaze-clock-time {{ font-family: {display}; font-weight: 700; font-size: {cs:.2}pt; color: @glaze_text_primary; \
             font-feature-settings: \"tnum\"; }}\n\
             .glaze-clock-date {{ font-size: {ss:.2}pt; color: @glaze_text_secondary; margin-top: -1px; }}\n\
             .glaze-panel button.glaze-show-desktop {{ min-width: 7px; border-left: {sw} solid @glaze_border; border-radius: 0; \
             transition: background-color {micro}ms {ease}; }}\n\
             .glaze-panel button.glaze-show-desktop:hover {{ background-color: @glaze_surface_sunken; }}\n\
             .glaze-notification-badge {{ color: @glaze_accent_soft_text; background-color: @glaze_accent_soft; border-radius: 8px; padding: 0 5px; }}\n\
             .glaze-tray-overflow {{ color: @glaze_ink; }}\n\
             .glaze-tray-well {{ background-color: {well_bg}; border: {sw} solid {well_border}; border-radius: {r}px; \
             box-shadow: none; padding: 0 2px 0 6px; min-height: {h}px; }}\n\
             .glaze-tray-well button.glaze-tray-button:hover, .glaze-tray-well button.glaze-clock:hover {{ background-color: alpha(@glaze_ink, 0.06); }}\n\
             .glaze-grip {{ min-width: 0; margin: 10px 5px; border-left: {sw} solid @glaze_border; }}\n",
            cs = ty.clock_size_pt,
            ch = (h - 4).max(16),
            rs = (r - 1).max(0),
            ss = ty.small_size_pt,
        ));

        // Glaze popovers: context menu, hover card, notifications, launcher.
        css.push_str(&format!(
            "popover.glaze-popover {{ background: none; box-shadow: none; padding: 0; margin: 0; }}\n\
             popover.glaze-popover > arrow {{ background: none; border: none; }}\n\
             popover.glaze-popover > contents {{\n  background-color: @glaze_surface_raised;\n  background-image: {texture};\n  \
             background-repeat: repeat;\n  color: @glaze_text_primary;\n  border: {sw} solid @glaze_border;\n  \
             border-radius: {mr}px;\n  padding: 6px;\n  box-shadow: {popover_sh};\n  font-family: {ui};\n  font-size: {fs:.2}pt;\n}}\n\
             popover.glaze-popover label {{ color: inherit; }}\n\
             .glaze-popover button.glaze-menu-item {{\n  min-height: 26px;\n  border-radius: {rs}px;\n  padding: 0 10px;\n  \
             transition: background-color {micro}ms {ease};\n}}\n\
             .glaze-popover button.glaze-menu-item:hover {{ background-color: @glaze_accent_soft; color: @glaze_accent_soft_text; }}\n\
             .glaze-popover button.glaze-menu-item.glaze-danger:hover {{ color: @glaze_danger; }}\n\
             .glaze-menu-sep {{ background-color: @glaze_border; min-height: {sw}; margin: 4px 2px; }}\n\
             .glaze-card-title {{ font-family: {display}; font-weight: 700; font-size: {cs:.2}pt; }}\n\
             .glaze-card-sub {{ color: @glaze_text_secondary; font-size: {ss:.2}pt; }}\n\
             .glaze-popover button.glaze-card-row {{\n  background-color: transparent;\n  border: none;\n  \
             border-radius: {rs}px;\n  padding: 6px 10px;\n  min-width: 200px;\n  min-height: 20px;\n  \
             transition: background-color {micro}ms {ease}, border-color {micro}ms {ease};\n}}\n\
             .glaze-popover button.glaze-card-row:hover {{ background-color: @glaze_accent_soft; color: @glaze_accent_soft_text; }}\n\
             .glaze-popover button.glaze-card-row.glaze-active {{ box-shadow: inset 2px 0 0 0 @glaze_accent; font-weight: {weight}; }}\n\
             .glaze-popover button.glaze-card-close {{ min-width: 18px; min-height: 18px; border-radius: 3px; color: @glaze_ink; opacity: 0.7; }}\n\
             .glaze-popover button.glaze-card-close:hover {{ background-color: @glaze_accent_soft; opacity: 1; }}\n",
            rs = (r - 1).max(0),
            cs = ty.clock_size_pt,
            ss = ty.small_size_pt,
        ));

        // Button styles (panel.button_style).
        css.push_str(
            ".glaze-style-flat button.glaze-task-button { border-color: transparent; background-color: transparent; box-shadow: none; }\n\
             .glaze-style-flat button.glaze-task-button:hover { border-color: @glaze_border; }\n\
             .glaze-style-flat button.glaze-task-button.glaze-active { background-color: @glaze_surface_raised; border-color: @glaze_border; }\n\
             .glaze-style-outline button.glaze-task-button { background-color: transparent; }\n\
             .glaze-style-outline button.glaze-task-button.glaze-active { background-color: transparent; border-color: @glaze_accent; }\n",
        );

        // Start menu (paper card).
        css.push_str(&format!(
            "popover.glaze-start-menu > contents {{ padding: 12px 12px 8px 12px; }}\n\
             .glaze-menu-title {{ font-family: {display}; font-weight: 700; font-size: {ts:.2}pt; color: @glaze_text_primary; }}\n\
             .glaze-search {{\n  background-color: @glaze_surface;\n  border: {sw} solid @glaze_border;\n  border-radius: {r}px;\n  \
             min-height: 32px;\n  transition: border-color {micro}ms {ease}, box-shadow {micro}ms {ease};\n}}\n\
             .glaze-search:focus-within {{ border-color: mix(@glaze_border, @glaze_accent, 0.7); box-shadow: none; }}\n\
             entry.glaze-search-entry, entry.glaze-search-entry:focus, entry.glaze-search-entry:focus-within {{\n  background: none;\n  border: none;\n  \
             box-shadow: none;\n  outline: none;\n  min-height: 30px;\n  padding: 0 8px 0 4px;\n  color: @glaze_text_primary;\n  caret-color: @glaze_accent;\n}}\n\
             entry.glaze-search-entry > text > placeholder {{ color: @glaze_text_secondary; }}\n\
             entry.glaze-search-entry selection {{ background-color: @glaze_accent_soft; color: @glaze_accent_soft_text; }}\n\
             .glaze-section-title {{ color: @glaze_text_secondary; font-size: {ss:.2}pt; font-weight: 600; letter-spacing: 0.06em; \
             margin: 8px 4px 2px 4px; }}\n\
             .glaze-category {{ font-family: {display}; font-weight: 700; font-size: {cs:.2}pt; color: @glaze_text_primary; margin: 10px 8px 2px 8px; }}\n\
             .glaze-popover button.glaze-app-tile {{\n  padding: 8px 2px 6px 2px;\n  border-radius: {r}px;\n  border: {sw} solid transparent;\n  \
             transition: background-color {micro}ms {ease}, border-color {micro}ms {ease};\n}}\n\
             .glaze-popover button.glaze-app-row {{\n  padding: 5px 8px;\n  min-height: 30px;\n  border-radius: {r}px;\n  \
             border: {sw} solid transparent;\n  transition: background-color {micro}ms {ease}, border-color {micro}ms {ease};\n}}\n\
             .glaze-popover button.glaze-app-tile:hover, .glaze-popover button.glaze-app-row:hover {{ background-color: @glaze_surface_sunken; }}\n\
             .glaze-popover button.glaze-app-tile.glaze-selected, .glaze-popover button.glaze-app-row.glaze-selected {{\n  \
             background-color: @glaze_accent_soft;\n  color: @glaze_accent_soft_text;\n  border-color: alpha(@glaze_accent, 0.55);\n}}\n\
             .glaze-popover button.glaze-power-button {{ min-width: 30px; min-height: 30px; border-radius: {r}px; \
             transition: background-color {micro}ms {ease}; }}\n\
             .glaze-popover button.glaze-power-button:hover {{ background-color: @glaze_surface_sunken; }}\n\
             .glaze-popover button.glaze-menu-item.glaze-confirm {{ background-color: @glaze_accent; color: @glaze_accent_text; }}\n\
             .glaze-user {{ color: @glaze_text_secondary; }}\n\
             .glaze-popover scrollbar, .glaze-popover scrollbar.vertical {{ background: none; border: none; }}\n\
             .glaze-popover scrollbar slider {{ background-color: @glaze_border; min-width: 4px; border-radius: 2px; border: none; }}\n\
             .glaze-popover calendar {{ background: none; border: none; color: @glaze_text_primary; }}\n\
             .glaze-popover calendar > grid > label.today {{ background-color: @glaze_accent; color: @glaze_accent_text; border-radius: {r}px; }}\n\
             .glaze-popover calendar > grid > label:selected {{ background-color: @glaze_accent_soft; color: @glaze_accent_soft_text; border-radius: {r}px; }}\n\
             tooltip, tooltip.background {{ background-color: @glaze_surface_raised; color: @glaze_text_primary; border: {sw} solid @glaze_border; \
             border-radius: {rs}px; box-shadow: none; }}\n\
             tooltip label {{ color: @glaze_text_primary; }}\n",
            ts = ty.clock_size_pt + 2.0,
            cs = ty.clock_size_pt,
            ss = ty.small_size_pt,
            rs = (r - 1).max(0),
        ));

        // Classic start menu (Windows 2000 structure in paper): brand strip,
        // single column of rows, cascading submenus.
        css.push_str(&format!(
            "popover.glaze-classic-menu, popover.glaze-classic-menu > contents {{ background: none; border: none; box-shadow: none; padding: 0; }}\n\
             .glaze-menu-card {{\n  background-color: @glaze_surface_raised;\n  background-image: {texture};\n  background-repeat: repeat;\n  \
             border: {sw} solid @glaze_border;\n  border-radius: {rl}px;\n  box-shadow: {popover_sh};\n  margin: 0 3px 3px 0;\n}}\n\
             .glaze-submenu-card {{ padding: 4px; }}\n\
             .glaze-menu-strip {{ background-color: @glaze_accent; border-radius: {rl}px 0 0 {rl}px; }}\n\
             .glaze-menu-column {{ padding: 8px 6px 6px 6px; }}\n\
             .glaze-popover button.glaze-menu-row {{\n  min-height: 26px;\n  padding: 2px 10px 2px 8px;\n  border-radius: {rs}px;\n  \
             border: {sw} solid transparent;\n  transition: background-color {micro}ms {ease}, border-color {micro}ms {ease};\n}}\n\
             .glaze-menu-column button.glaze-menu-row {{ min-height: 28px; }}\n\
             .glaze-popover button.glaze-menu-row:hover, .glaze-popover button.glaze-menu-row.glaze-selected {{\n  \
             background-color: @glaze_accent_soft;\n  color: @glaze_accent_soft_text;\n}}\n\
             {strip_dark}\
             .glaze-popover button.glaze-menu-row:disabled {{ color: @glaze_text_secondary; }}\n\
             .glaze-menu-panel {{ padding: 6px 4px 2px 4px; }}\n\
             entry.glaze-run-entry {{ background: @glaze_surface; border: {sw} solid @glaze_border; border-radius: {r}px; box-shadow: none; \
             outline: none; min-height: 30px; color: @glaze_text_primary; caret-color: @glaze_accent; }}\n\
             entry.glaze-run-entry:focus-within {{ border-color: @glaze_accent; }}\n\
             .glaze-popover checkbutton check, .glaze-popover checkbutton radio {{ background: @glaze_surface; border: {sw} solid @glaze_border; \
             box-shadow: none; color: @glaze_accent_text; -gtk-icon-source: none; }}\n\
             .glaze-popover checkbutton radio:checked {{ background: @glaze_accent; border-color: @glaze_accent; }}\n",
            rl = mr,
            rs = (r - 1).max(0),
            strip_dark = if self.scheme == ColorScheme::Dark && flat {
                ".glaze-menu-strip { background-color: @glaze_accent_soft; box-shadow: inset -1px 0 0 0 alpha(@glaze_accent, 0.45); }\n"
            } else {
                ""
            },
        ));

        // Title-bar window controls (shell overlay on Sway title bars).
        css.push_str(&format!(
            "window.glaze-wincontrols-window, window.glaze-wincontrols-window > * {{ background: none; box-shadow: none; }}\n\
             .glaze-wincontrols button {{ background: none; border: none; box-shadow: none; outline: none; min-width: 0; min-height: 0; \
             padding: 0; border-radius: {rs}px; color: @glaze_ink; transition: background-color {micro}ms {ease}; }}\n\
             .glaze-wincontrols button:hover {{ background-color: alpha(@glaze_ink, 0.08); }}\n\
             .glaze-wincontrols button:active {{ background-color: alpha(@glaze_ink, 0.15); }}\n\
             .glaze-wincontrols button.glaze-wincontrol-close:hover {{ background-color: @glaze_accent; }}\n",
            rs = (r - 2).max(0),
        ));

        // Shared flat context menu (menu.rs).
        css.push_str(&format!(
            "popover.glaze-flat-menu, popover.glaze-flat-menu > contents {{ background: none; border: none; box-shadow: none; padding: 0; }}\n\
             .glaze-flat-menu-card {{ padding: 4px; min-width: 210px; }}\n\
             .glaze-popover button.glaze-mrow {{ min-height: 32px; padding: 0 12px 0 10px; border-radius: {rs}px; border: none; \
             outline: none; color: @glaze_text_primary; transition: background-color {micro}ms {ease}; }}\n\
             .glaze-popover button.glaze-mrow:hover, .glaze-popover button.glaze-mrow.glaze-selected {{ \
             background-color: @glaze_accent_soft; color: @glaze_accent_soft_text; }}\n\
             .glaze-popover button.glaze-mrow.glaze-disabled {{ opacity: 0.45; }}\n\
             .glaze-popover button.glaze-mrow.glaze-disabled:hover, .glaze-popover button.glaze-mrow.glaze-disabled.glaze-selected \
             {{ background: none; color: @glaze_text_primary; }}\n\
             .glaze-popover button.glaze-mrow.glaze-danger:hover, .glaze-popover button.glaze-mrow.glaze-danger.glaze-selected {{ color: @glaze_danger; }}\n\
             .glaze-mrow-accel {{ color: @glaze_text_secondary; font-size: {ss:.2}pt; margin-left: 18px; }}\n\
             .glaze-menu-header {{ color: @glaze_text_secondary; font-size: {ss:.2}pt; font-weight: 600; padding: 6px 12px 4px 10px; }}\n",
            rs = (r - 1).max(0),
            ss = ty.small_size_pt + 0.5,
        ));

        // Toasts (shell notices above the taskbar end).
        css.push_str(&format!(
            "window.glaze-toast-window, window.glaze-ghost-window, window.glaze-ghost-window > * \
             {{ background: none; background-color: transparent; box-shadow: none; }}\n\
             .glaze-toast {{ background-color: @glaze_surface_raised; background-image: {texture}; background-repeat: repeat; \
             color: @glaze_text_primary; border: {sw} solid @glaze_border; border-radius: {mr}px; padding: 10px 14px 10px 10px; \
             box-shadow: {popover_sh}; margin: 0 3px 3px 0; font-family: {ui}; font-size: {fs:.2}pt; min-width: 260px; }}\n\
             .glaze-toast-mark {{ background-color: @glaze_accent; min-width: 3px; border-radius: 2px; }}\n\
             .glaze-toast-title {{ font-weight: {weight}; color: @glaze_text_primary; }}\n\
             .glaze-toast-body {{ color: @glaze_text_secondary; font-size: {ss:.2}pt; }}\n",
            ss = ty.small_size_pt + 0.5,
        ));

        // Legacy surfaces (launcher, generic menus, desktop, mobile) keep their
        // class names; they now use the v2 tokens.
        css.push_str(&format!(
            ".glaze-launcher {{\n  background-color: @glaze_surface_raised;\n  color: @glaze_text_primary;\n  \
             border: {sw} solid @glaze_border;\n  border-radius: {mr}px;\n  padding: {s}px;\n  box-shadow: {popover_sh};\n}}\n\
             .glaze-launcher-search {{\n  min-height: {h}px;\n  border-radius: {r}px;\n  background-color: @glaze_surface_sunken;\n  \
             color: @glaze_text_primary;\n  padding: 0 10px;\n}}\n\
             .glaze-launcher-group-title {{\n  color: @glaze_text_secondary;\n  font-size: 0.85em;\n  padding: 6px 8px 2px 8px;\n}}\n\
             .glaze-launcher-entry {{\n  min-height: {h}px;\n  border-radius: {r}px;\n  padding: 4px 10px;\n  \
             transition: background-color {menu}ms {ease};\n}}\n\
             .glaze-launcher-entry:hover {{ background-color: @glaze_surface_sunken; }}\n\
             .glaze-launcher-entry.glaze-selected {{ background-color: @glaze_accent; color: @glaze_accent_text; }}\n\
             .glaze-launcher-empty {{ color: @glaze_text_secondary; padding: 12px; }}\n\
             .glaze-menu {{\n  background-color: @glaze_surface_raised;\n  border: {sw} solid @glaze_border;\n  \
             border-radius: {mr}px;\n  padding: 4px;\n  box-shadow: {popover_sh};\n}}\n\
             .glaze-menu-item {{ min-height: {h}px; border-radius: {r}px; padding: 0 8px; }}\n\
             .glaze-menu-item:hover {{ background-color: @glaze_accent_soft; color: @glaze_accent_soft_text; }}\n\
             .glaze-separator {{ background-color: @glaze_border; min-height: {sw}; margin: 4px 6px; }}\n\
             .glaze-sheet {{ background-color: @glaze_surface; border: {sw} solid @glaze_border; border-radius: {wr}px; box-shadow: {window_sh}; }}\n",
            s = m.spacing,
            wr = m.window_radius,
        ));
        css.push_str(&format!(
            ".glaze-desktop {{\n  background-color: @glaze_canvas;\n  color: @glaze_text_primary;\n}}\n\
             .glaze-desktop-icon {{\n  border-radius: {r}px;\n  padding: 6px;\n  border: {sw} solid transparent;\n  \
             transition: background-color {micro}ms {ease}, border-color {micro}ms {ease};\n}}\n\
             .glaze-desktop-icon-label {{ background-color: alpha(@glaze_surface_raised, 0.78); color: @glaze_text_primary; \
             border-radius: 3px; padding: 0 4px; font-size: {fs:.2}pt; }}\n\
             .glaze-desktop-icon.glaze-selected {{ border-color: alpha(@glaze_accent, 0.7); }}\n\
             .glaze-desktop-icon:hover {{ background-color: alpha(@glaze_accent, 0.15); }}\n\
             .glaze-desktop-icon.glaze-selected {{ background-color: alpha(@glaze_accent, 0.28); }}\n\
             .glaze-text-secondary {{ color: @glaze_text_secondary; }}\n\
             .glaze-danger {{ color: @glaze_danger; }}\n\
             .glaze-success {{ color: @glaze_success; }}\n\
             .glaze-display {{ font-family: {display}; letter-spacing: -0.01em; }}\n\
             .glaze-mono {{ font-family: {mono}; }}\n"
        ));

        // Mobile / touch layout. Every control is at least `touch` px.
        css.push_str(&format!(
            ".glaze-nav-bar {{\n  background-color: @glaze_panel;\n  color: @glaze_text_primary;\n  \
             border-top: {sw} solid @glaze_border;\n  font-family: {ui};\n  font-size: {fs:.2}pt;\n}}\n\
             .glaze-nav-button {{\n  min-width: {touch}px;\n  min-height: {touch}px;\n  border-radius: {mr}px;\n  \
             background-color: transparent;\n  color: @glaze_text_secondary;\n  transition: background-color {micro}ms {ease};\n}}\n\
             .glaze-nav-button:hover {{ background-color: @glaze_surface_sunken; }}\n\
             .glaze-nav-button:active,\n.glaze-nav-button.glaze-open {{ background-color: @glaze_accent; color: @glaze_accent_text; }}\n\
             .glaze-app-grid {{\n  background-color: @glaze_surface;\n  color: @glaze_text_primary;\n  font-family: {ui};\n  padding: {pad}px;\n}}\n\
             .glaze-app-grid-search {{\n  min-height: {touch}px;\n  border-radius: {mr}px;\n  background-color: @glaze_surface_sunken;\n  \
             color: @glaze_text_primary;\n  padding: 0 16px;\n}}\n\
             .glaze-app-tile {{\n  min-width: {touch}px;\n  min-height: {touch}px;\n  border-radius: {mr}px;\n  padding: 10px 6px;\n  \
             background-color: transparent;\n  color: @glaze_text_primary;\n}}\n\
             .glaze-app-tile:hover,\n.glaze-app-tile:active {{ background-color: @glaze_surface_sunken; }}\n\
             .glaze-page-dot {{ min-width: 8px; min-height: 8px; border-radius: 4px; background-color: @glaze_border; }}\n\
             .glaze-page-dot.glaze-selected {{ background-color: @glaze_accent; }}\n",
            pad = m.panel_padding + m.spacing,
        ));

        // Classic bar (panel.style = "classic-bar"): full-width flat bar,
        // full-height icon buttons, running bars, flush tray (Windows 10
        // shape, Glaze paper colours). Hover plates fade, presses squish.
        let bh = (m.panel_height - 1).max(20);
        css.push_str(&format!(
            ".glaze-style-bar button.glaze-start-button, .glaze-style-bar button.glaze-bar-button {{\n  \
             min-width: 48px; min-height: {bh}px; border-radius: 0; padding: 0; margin: 0;\n  \
             background-color: transparent; box-shadow: none; border: none;\n  \
             transition: background-color {micro}ms {ease}, transform {micro}ms {ease};\n}}\n\
             .glaze-style-bar button.glaze-bar-button {{ min-width: 46px; color: @glaze_ink; }}\n\
             .glaze-style-bar button.glaze-start-button:hover, .glaze-style-bar button.glaze-bar-button:hover \
             {{ background-color: alpha(@glaze_ink, 0.08); }}\n\
             .glaze-style-bar button.glaze-start-button.glaze-open {{ background-color: alpha(@glaze_accent, 0.26); }}\n\
             .glaze-style-bar button.glaze-start-button:active, .glaze-style-bar button.glaze-bar-button:active \
             {{ transform: scale(0.92); }}\n\
             .glaze-style-bar .glaze-start-mark {{ transition: transform {micro}ms {ease}; }}\n\
             .glaze-style-bar button.glaze-start-button:hover .glaze-start-mark {{ transform: rotate(-12deg); }}\n\
             .glaze-style-bar button.glaze-task-button {{\n  min-height: {bh}px; border: none; border-radius: 0; \
             padding: 0; background-color: transparent; box-shadow: none;\n  \
             transition: background-color {micro}ms {ease}, transform {micro}ms {ease};\n}}\n\
             .glaze-style-bar button.glaze-task-button:hover {{ background-color: alpha(@glaze_ink, 0.08); }}\n\
             .glaze-style-bar button.glaze-task-button.glaze-active {{ background-color: alpha(@glaze_ink, 0.12); box-shadow: none; }}\n\
             .glaze-style-bar button.glaze-task-button.glaze-active:hover {{ background-color: alpha(@glaze_ink, 0.16); }}\n\
             .glaze-style-bar button.glaze-task-button:active {{ transform: scale(0.94); }}\n\
             .glaze-style-bar button.glaze-task-button.glaze-minimized {{ background-color: transparent; border: none; }}\n\
             .glaze-style-bar button.glaze-task-button.glaze-minimized > overlay > box {{ opacity: 0.7; }}\n\
             .glaze-style-bar button.glaze-task-button.glaze-group {{ box-shadow: inset -3px 0 0 -1px alpha(@glaze_ink, 0.18); }}\n\
             .glaze-style-bar .glaze-underline {{ min-height: 2px; border-radius: 0; margin-bottom: 0; }}\n\
             .glaze-style-bar .glaze-tray-well, .glaze-style-bar .glaze-segmented {{ background: none; border: none; \
             box-shadow: none; padding: 0; min-height: {bh}px; }}\n\
             .glaze-style-bar button.glaze-tray-button {{ min-height: {bh}px; min-width: 30px; border-radius: 0; }}\n\
             .glaze-style-bar button.glaze-segment {{ min-height: {bh}px; min-width: 34px; border-radius: 0; }}\n\
             .glaze-style-bar button.glaze-segment.glaze-selected {{ box-shadow: inset 0 -2px 0 0 @glaze_accent; background: none; }}\n\
             .glaze-style-bar button.glaze-clock {{ min-height: {bh}px; margin: 0; border-radius: 0; padding: 0 10px; }}\n\
             .glaze-style-bar .glaze-clock-time {{ font-family: {ui}; font-weight: 400; font-size: {fs:.2}pt; }}\n\
             .glaze-style-bar .glaze-clock-date {{ font-size: {fs:.2}pt; color: @glaze_text_primary; margin-top: 0; }}\n\
             .glaze-style-bar button.glaze-show-desktop {{ min-width: 8px; min-height: {bh}px; \
             border-left: {sw} solid alpha(@glaze_ink, 0.18); }}\n\
             .glaze-style-bar .glaze-section-sep, .glaze-style-bar .glaze-grip {{ background: none; border: none; \
             min-width: 0; margin: 0; }}\n"
        ));

        // Two-pane Start menu and tiles.
        css.push_str(&format!(
            "popover.glaze-start-pane, popover.glaze-start-pane > contents {{ background: none; border: none; \
             box-shadow: none; padding: 0; margin: 0; }}\n\
             .glaze-start-pane-card {{ background-color: @glaze_surface_raised; background-image: {texture}; \
             background-repeat: repeat; border: {sw} solid @glaze_border; border-bottom: none; border-radius: 0; \
             box-shadow: {popover_sh}; color: @glaze_text_primary; font-family: {ui}; font-size: {fs:.2}pt; }}\n\
             .glaze-start-rail {{ background-color: @glaze_surface_raised; padding: 4px 0; \
             transition: box-shadow {micro}ms {ease}; }}\n\
             .glaze-start-rail.glaze-rail-open {{ box-shadow: 3px 0 0 0 alpha(@glaze_ink, 0.10); \
             border-right: {sw} solid @glaze_border; }}\n\
             .glaze-start-pane button.glaze-rail-item {{ min-height: 44px; border-radius: 0; padding: 0; \
             transition: background-color {micro}ms {ease}; }}\n\
             .glaze-start-pane button.glaze-rail-item:hover {{ background-color: alpha(@glaze_ink, 0.08); }}\n\
             .glaze-start-list {{ padding: 6px 4px; }}\n\
             .glaze-start-pane button.glaze-start-letter {{ min-height: 30px; min-width: 30px; padding: 0 10px; \
             border-radius: 0; font-weight: {weight}; color: @glaze_text_primary; }}\n\
             .glaze-start-pane button.glaze-start-letter:hover {{ background-color: alpha(@glaze_ink, 0.08); }}\n\
             .glaze-start-letter-label {{ color: @glaze_text_secondary; margin: 8px 10px 4px 10px; }}\n\
             .glaze-start-pane button.glaze-start-row {{ min-height: 36px; padding: 0 10px; border-radius: 0; \
             transition: background-color {micro}ms {ease}; }}\n\
             .glaze-start-pane button.glaze-start-row:hover {{ background-color: alpha(@glaze_ink, 0.08); }}\n\
             .glaze-start-pane button.glaze-start-row:active {{ background-color: alpha(@glaze_ink, 0.14); }}\n\
             .glaze-start-jump {{ background-color: @glaze_surface_raised; border: {sw} solid @glaze_border; padding: 8px; \
             box-shadow: {popover_sh}; }}\n\
             .glaze-start-pane button.glaze-start-jump-key {{ min-width: 44px; min-height: 40px; border-radius: 0; \
             font-weight: {weight}; }}\n\
             .glaze-start-pane button.glaze-start-jump-key:hover {{ background-color: @glaze_accent_soft; }}\n\
             .glaze-start-tiles {{ padding: 8px 16px 16px 12px; }}\n\
             .glaze-tile-group {{ font-size: {fs:.2}pt; color: @glaze_text_primary; margin: 10px 2px 8px 2px; }}\n\
             .glaze-start-pane button.glaze-tile {{ border-radius: 0; padding: 6px; border: {sw} solid transparent; \
             transition: transform {micro}ms {ease}, border-color {micro}ms {ease}, box-shadow {micro}ms {ease}; }}\n\
             .glaze-start-pane button.glaze-tile:hover {{ transform: translateY(-2px); \
             border-color: alpha(@glaze_ink, 0.25); box-shadow: 0 2px 0 0 alpha(@glaze_ink, 0.12); }}\n\
             .glaze-start-pane button.glaze-tile:active {{ transform: scale(0.97); }}\n\
             .glaze-start-pane button.glaze-tile.glaze-tile-accent {{ background-color: @glaze_accent; color: @glaze_accent_text; }}\n\
             .glaze-start-pane button.glaze-tile.glaze-tile-ink {{ background-color: mix(@glaze_ink, @glaze_surface_raised, 0.15); color: @glaze_surface_raised; }}\n\
             .glaze-start-pane button.glaze-tile.glaze-tile-paper {{ background-color: @glaze_surface_sunken; color: @glaze_text_primary; }}\n\
             .glaze-tile-name {{ font-size: {ss:.2}pt; margin: 0 2px 0 2px; color: inherit; }}\n",
            ss = ty.small_size_pt + 0.5,
        ));

        if opts.focus_rings {
            css.push_str(
                "button:focus-visible,\n.glaze-launcher-entry:focus-visible,\n\
                 .glaze-panel button.glaze-task-button:focus-visible,\n.glaze-panel button.glaze-start-button:focus-visible,\n\
                 .glaze-panel button.glaze-pin-button:focus-visible,\n.glaze-nav-button:focus-visible,\n.glaze-app-tile:focus-visible {\n  \
                 outline: 2px solid @glaze_focus;\n  outline-offset: 1px;\n}\n",
            );
        }
        css
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::ThemeRole;

    fn all() -> Vec<ThemeTokens> {
        let mut v = Vec::new();
        for k in ThemeKind::ALL {
            for s in ColorScheme::ALL {
                v.push(ThemeTokens::for_kind(k, s));
            }
        }
        v
    }

    #[test]
    fn css_defines_every_role_color() {
        for t in all() {
            let css = t.to_gtk_css();
            for role in ThemeRole::ALL {
                assert!(
                    css.contains(&format!("@define-color glaze_{} ", role.key())),
                    "missing @define-color for {role:?}"
                );
            }
        }
    }

    #[test]
    fn css_uses_tokens_not_raw_hex() {
        for t in all() {
            let css = t.to_gtk_css_with(CssOptions {
                texture_url: Some("file:///usr/share/glaze/textures/grain.png".into()),
                ..CssOptions::default()
            });
            let body = css
                .lines()
                .filter(|l| !l.trim_start().starts_with("@define-color"))
                .collect::<Vec<_>>()
                .join("\n");
            assert!(
                !body.contains('#'),
                "component CSS must not contain raw hex"
            );
        }
    }

    /// Parse every `box-shadow:` value and return the blur radii found.
    fn blur_radii(css: &str) -> Vec<f64> {
        let mut out = Vec::new();
        for line in css.lines() {
            let Some(idx) = line.find("box-shadow:") else {
                continue;
            };
            let value = line[idx + "box-shadow:".len()..]
                .split([';', '}'])
                .next()
                .unwrap_or("");
            for layer in value.split(", ").filter(|l| !l.contains("rgba(") || true) {
                let lens: Vec<f64> = layer
                    .split_whitespace()
                    .filter_map(|tok| tok.strip_suffix("px"))
                    .filter_map(|n| n.parse::<f64>().ok())
                    .collect();
                if lens.len() >= 3 && !layer.trim_start().starts_with("inset") {
                    out.push(lens[2]);
                }
            }
        }
        out
    }

    /// Brief requirement: Clay CSS has no shadow with blur > 0, at any
    /// strength and in both schemes.
    #[test]
    fn clay_css_has_no_blurred_shadows() {
        for scheme in ColorScheme::ALL {
            let mut t = ThemeTokens::for_kind(ThemeKind::Clay, scheme);
            for strength in [0.0, 1.0, 2.0] {
                t.shadows.strength = strength;
                let css = t.to_gtk_css();
                let radii = blur_radii(&css);
                assert!(!radii.is_empty(), "expected some offset shadows");
                assert!(
                    radii.iter().all(|b| *b == 0.0),
                    "Clay emitted a blurred shadow: {radii:?}"
                );
                assert!(!css.contains("inset 1px 1px 0"), "Clay must not bevel");
            }
        }
        // Classic is allowed (and expected) to blur and bevel.
        let classic = ThemeTokens::for_kind(ThemeKind::Classic, ColorScheme::Light).to_gtk_css();
        assert!(blur_radii(&classic).iter().any(|b| *b > 0.0));
        assert!(classic.contains("inset 1px 1px 0"));
    }

    #[test]
    fn reduced_motion_emits_zero_durations_and_speed_scales() {
        let t = ThemeTokens::default_clay();
        let css = t.to_gtk_css_with(CssOptions {
            reduced_motion: true,
            ..CssOptions::default()
        });
        assert!(css.contains("background-color 0ms"));
        assert!(!css.contains("background-color 140ms"));
        let fast = t.to_gtk_css_with(CssOptions {
            duration_scale: 0.5,
            ..CssOptions::default()
        });
        assert!(fast.contains("background-color 70ms"));
    }

    #[test]
    fn texture_is_applied_and_reduced_effects_removes_it_and_shadows() {
        let t = ThemeTokens::default_clay();
        let url = "file:///tmp/glaze/grain-40.png".to_string();
        let css = t.to_gtk_css_with(CssOptions {
            texture_url: Some(url.clone()),
            ..CssOptions::default()
        });
        assert!(css.contains(&format!("background-image: url(\"{url}\")")));
        let reduced = t.to_gtk_css_with(CssOptions {
            texture_url: Some(url.clone()),
            reduced_effects: true,
            ..CssOptions::default()
        });
        assert!(!reduced.contains(&url));
        assert!(
            blur_radii(&reduced).is_empty(),
            "no shadows in reduced effects"
        );
    }

    #[test]
    fn metrics_and_fonts_reach_the_css() {
        let t = ThemeTokens::default_clay();
        let css = t.to_gtk_css();
        assert!(css.contains("min-height: 28px"));
        assert!(css.contains("border-radius: 5px"));
        assert!(css.contains("1px solid @glaze_border"));
        assert!(css.contains("Noto Serif"));
        assert!(css.contains("\"Inter\""));
        let mut thick = t.clone();
        thick.metrics.stroke_width = 1.25;
        assert!(thick.to_gtk_css().contains("1.25px solid @glaze_border"));
    }

    #[test]
    fn touch_targets_reach_the_css() {
        for t in all() {
            let css = t.to_gtk_css();
            assert!(css.contains(".glaze-nav-button"));
            assert!(css.contains("min-height: 48px"));
        }
    }

    #[test]
    fn px_formatting_is_compact() {
        assert_eq!(px(1.0), "1px");
        assert_eq!(px(1.25), "1.25px");
    }
}
