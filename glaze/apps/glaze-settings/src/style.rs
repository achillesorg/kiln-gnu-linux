//! Settings-window CSS, generated from the Glaze design tokens.
//!
//! The `@define-color glaze_*` header and the shared Glaze rules come from
//! `ThemeTokens::to_gtk_css_with`; this module adds the settings layout
//! (sidebar, paper cards, rows) and restyles the stock GTK controls in the
//! flat cut-paper language: hairline warm strokes, small radii, no blur, no
//! gradients, terracotta accent fills. Every colour is a `@glaze_*` role and
//! every size, radius, stroke, font and duration comes from the tokens, so
//! Clay/Classic, light/dark and user overrides all reach this window.

use glaze_design::{CssOptions, ThemeKind, ThemeTokens};

/// Inputs that change the CSS besides the tokens.
#[derive(Debug, Clone, PartialEq)]
pub struct StyleInputs {
    pub reduced_motion: bool,
    /// Multiplier on durations (1 / motion.speed, x0.65 for snappy).
    pub duration_scale: f64,
    pub reduced_effects: bool,
    /// `file://` URL of the paper tile for the active scheme, if any.
    pub texture_url: Option<String>,
    /// 0..=1
    pub texture_intensity: f64,
}

fn px(v: f64) -> String {
    let r = (v * 100.0).round() / 100.0;
    if (r - r.round()).abs() < 1e-9 {
        format!("{}px", r.round() as i64)
    } else {
        format!("{r}px")
    }
}

pub fn css(t: &ThemeTokens, s: &StyleInputs) -> String {
    let mut out = t.to_gtk_css_with(CssOptions {
        reduced_motion: s.reduced_motion,
        focus_rings: true,
        duration_scale: s.duration_scale,
        reduced_effects: s.reduced_effects,
        texture_url: None,
    });
    let m = &t.metrics;
    let ty = &t.typography;
    let classic = t.kind == ThemeKind::Classic;
    let scale = if s.reduced_motion {
        0.0
    } else {
        s.duration_scale.clamp(0.0, 8.0)
    };
    let dur = |ms: u32| ((ms as f64) * scale).round() as u32;
    let micro = dur(m.motion_micro_ms);
    let menu = dur(m.motion_menu_ms);
    let ease = "cubic-bezier(0.2, 0, 0, 1)";
    let sw = px(m.stroke_width.max(0.0));
    // Controls keep a visible edge even when the user sets stroke weight 0.
    let swc = px(m.stroke_width.max(1.0));
    let r = m.control_radius;
    let cr = m.menu_radius;
    let h = m.control_height.max(24);
    let fs = m.font_size_pt + 0.75;
    let small = (m.font_size_pt - 0.5).max(7.0);
    let ui = t.ui_font_stack();
    let display = t.display_font_stack();
    let effects = !s.reduced_effects;
    let shadow = if effects && !t.shadows.popover.is_none() {
        format!(
            "{}px {}px 0 0 {}",
            t.shadows.popover.x,
            t.shadows.popover.y,
            t.shadows.color(&t.palette).to_css()
        )
    } else {
        "none".into()
    };
    // Paper grain: the tile under a veil of the surface colour, so intensity
    // fades texels toward the fill exactly like the shell's baked tiles.
    let paper = |surface: &str| -> String {
        match (&s.texture_url, effects) {
            (Some(url), true) if s.texture_intensity > 0.0 => {
                let veil = (1.0 - s.texture_intensity.clamp(0.0, 1.0)).max(0.0);
                format!(
                    "background-image: linear-gradient(alpha(@glaze_{surface}, {veil:.2}), alpha(@glaze_{surface}, {veil:.2})), url(\"{url}\"); background-repeat: repeat;"
                )
            }
            _ => "background-image: none;".into(),
        }
    };
    let raised = if classic {
        "inset 1px 1px 0 @glaze_highlight, inset -1px -1px 0 @glaze_border".to_string()
    } else if effects {
        "0 1px 0 0 alpha(@glaze_border, 0.9)".to_string()
    } else {
        "none".to_string()
    };
    let sunken = if classic {
        "inset 1px 1px 0 @glaze_border, inset -1px -1px 0 @glaze_highlight".to_string()
    } else {
        "inset 0 1px 0 0 alpha(@glaze_ink, 0.10)".to_string()
    };

    out.push_str(&format!(
        r#"
/* ---- glaze-settings ---------------------------------------------------- */
window.fs-window, window.fs-dialog {{
  background-color: @glaze_canvas;
  color: @glaze_text_primary;
  font-family: {ui};
  font-size: {fs:.2}pt;
}}
window.fs-window > .fs-body {{ background-color: @glaze_canvas; }}
headerbar.fs-headerbar {{
  background-color: @glaze_panel;
  background-image: none;
  color: @glaze_text_primary;
  border-bottom: {sw} solid @glaze_border;
  box-shadow: none;
  min-height: 42px;
  padding: 0 6px;
}}
headerbar.fs-headerbar:backdrop {{ background-color: @glaze_panel; }}
.fs-app-title {{ font-family: {display}; font-weight: 700; font-size: {ts:.2}pt; }}
headerbar.fs-headerbar .fs-wincontrols {{ margin-left: 8px; }}
.fs-app-sub {{ color: @glaze_text_secondary; font-size: {small:.2}pt; }}
headerbar.fs-headerbar windowcontrols button, headerbar.fs-headerbar button.fs-wincontrol {{
  min-width: 26px; min-height: 26px; padding: 0; margin: 0 1px;
  border: none; background: none; box-shadow: none; border-radius: {r}px; color: @glaze_ink;
}}
headerbar.fs-headerbar windowcontrols button:hover, headerbar.fs-headerbar button.fs-wincontrol:hover {{ background-color: @glaze_surface_sunken; }}
headerbar.fs-headerbar windowcontrols button.close:hover, headerbar.fs-headerbar button.fs-wincontrol.close:hover {{ background-color: @glaze_accent; color: @glaze_accent_text; }}

/* Sidebar */
.fs-sidebar {{
  background-color: @glaze_panel;
  {paper_panel}
  border-right: {sw} solid @glaze_border;
}}
.fs-sidebar-heading {{
  color: @glaze_text_secondary; font-size: {small:.2}pt; font-weight: 600;
  letter-spacing: 1px; margin: 14px 14px 4px 16px;
}}
list.fs-nav {{ background: none; }}
list.fs-nav > row.fs-nav-heading {{ min-height: 0; background: none; box-shadow: none; }}
list.fs-nav > row.fs-nav-heading:hover {{ background: none; }}
list.fs-nav > row {{
  margin: 1px 8px; padding: 0; border-radius: {r}px; min-height: 30px;
  transition: background-color {micro}ms {ease}, box-shadow {micro}ms {ease};
}}
list.fs-nav > row > box {{ padding: 5px 10px; }}
list.fs-nav > row:hover {{ background-color: alpha(@glaze_ink, 0.06); }}
list.fs-nav > row:selected {{
  background-color: @glaze_accent_soft; color: @glaze_accent_soft_text;
  box-shadow: inset 2px 0 0 0 @glaze_accent;
}}
list.fs-nav > row:selected label {{ color: @glaze_accent_soft_text; }}
list.fs-nav > row.fs-dim {{ opacity: 0.45; }}
.fs-nav-count {{
  background-color: @glaze_accent_soft; color: @glaze_accent_soft_text; border-radius: 8px;
  padding: 0 6px; font-size: {small:.2}pt; font-weight: 600;
}}
.fs-sidebar-foot {{ color: @glaze_text_secondary; font-size: {small:.2}pt; margin: 8px 16px 12px 16px; }}

/* Search */
.fs-search {{
  background-color: @glaze_surface; border: {sw} solid @glaze_border; border-radius: {r}px;
  min-height: 30px; margin: 12px 10px 4px 10px;
  transition: border-color {micro}ms {ease}, box-shadow {micro}ms {ease};
}}
.fs-search:focus-within {{ border-color: @glaze_accent; box-shadow: 0 0 0 1px @glaze_accent; }}
.fs-search > text {{ padding: 0 6px; }}
.fs-search image {{ color: @glaze_text_secondary; }}

/* Pages */
.fs-page {{ margin: 22px 32px 40px 32px; }}
.fs-page-title {{ font-family: {display}; font-weight: 700; font-size: {pt:.2}pt; letter-spacing: -0.01em; }}
.fs-page-blurb {{ color: @glaze_text_secondary; margin-bottom: 6px; }}
.fs-card {{
  background-color: @glaze_surface_raised;
  {paper_card}
  border: {sw} solid @glaze_border;
  border-radius: {cr}px;
  box-shadow: {shadow};
}}
.fs-card-head {{ padding: 12px 16px 8px 16px; }}
.fs-card-title {{ font-family: {display}; font-weight: 700; font-size: {ct:.2}pt; }}
.fs-card-note {{ color: @glaze_text_secondary; font-size: {small:.2}pt; }}
.fs-sep {{ background-color: alpha(@glaze_border, 0.7); min-height: {sw}; margin: 0 16px; }}
.fs-row {{ padding: 9px 16px; min-height: 30px; transition: background-color {menu}ms {ease}; border-radius: {cr}px; }}
.fs-row.fs-flash {{ background-color: @glaze_accent_soft; }}
.fs-row-title {{ color: @glaze_text_primary; }}
.fs-row-sub {{ color: @glaze_text_secondary; font-size: {small:.2}pt; }}
.fs-row:disabled .fs-row-title, .fs-row.fs-off .fs-row-title {{ color: @glaze_text_secondary; }}
.fs-error {{ color: @glaze_danger; font-size: {small:.2}pt; }}
.fs-warn {{ color: @glaze_warning; font-size: {small:.2}pt; }}
.fs-ok {{ color: @glaze_success; font-size: {small:.2}pt; }}
.fs-badge {{
  background-color: @glaze_surface_sunken; color: @glaze_text_secondary; border: {sw} solid @glaze_border;
  border-radius: 8px; padding: 0 6px; font-size: {tiny:.2}pt;
}}
.fs-badge.fs-later {{ background-color: @glaze_accent_soft; color: @glaze_accent_soft_text; border-color: transparent; }}
.fs-value {{ color: @glaze_text_secondary; font-feature-settings: "tnum"; min-width: 44px; }}
.fs-keycap {{
  background-color: @glaze_surface; border: {sw} solid @glaze_border; border-radius: {rs}px;
  box-shadow: 0 1px 0 0 @glaze_border; padding: 1px 6px; font-size: {small:.2}pt; font-weight: 600;
}}
.fs-keycap.fs-disabled-key {{ color: @glaze_text_secondary; font-weight: 400; box-shadow: none; }}
.fs-mono {{ font-family: {mono}; font-size: {small:.2}pt; color: @glaze_text_secondary; }}
.fs-empty {{ padding: 26px 24px; }}
.fs-empty-title {{ font-family: {display}; font-weight: 700; font-size: {ct:.2}pt; }}
.fs-banner {{
  background-color: alpha(@glaze_danger, 0.10); border: {sw} solid alpha(@glaze_danger, 0.55);
  border-radius: {cr}px; padding: 10px 14px; margin: 14px 32px 0 32px;
}}
.fs-banner label {{ color: @glaze_text_primary; }}
.fs-result-crumb {{ color: @glaze_text_secondary; font-size: {small:.2}pt; }}
list.fs-results {{ background: none; }}
list.fs-results > row {{ border-radius: {r}px; padding: 8px 12px; margin: 1px 0; transition: background-color {micro}ms {ease}; }}
list.fs-results > row:hover {{ background-color: @glaze_surface_sunken; }}
list.fs-results > row:selected, list.fs-results > row:focus-visible {{ background-color: @glaze_accent_soft; }}
list.fs-plain {{ background: none; }}
list.fs-plain > row {{ padding: 0; border-radius: {r}px; }}
list.fs-plain > row:hover {{ background-color: alpha(@glaze_ink, 0.04); }}
list.fs-plain > row.fs-drop-target {{ box-shadow: inset 0 2px 0 0 @glaze_accent; }}
list.fs-plain > row.fs-hidden-item label.fs-row-title {{ color: @glaze_text_secondary; }}

/* Toast */
.fs-toast {{
  background-color: @glaze_surface_raised; border: {sw} solid @glaze_border; border-radius: {cr}px;
  box-shadow: {shadow}; padding: 6px 6px 6px 14px; margin: 0 0 18px 0;
}}
.fs-toast.fs-toast-error {{ border-color: @glaze_danger; }}

/* Controls ------------------------------------------------------------- */
button.fs-btn, .fs-page button:not(.fs-tile):not(.fs-swatch):not(.fs-link),
window.fs-dialog button, headerbar.fs-headerbar button.fs-btn {{
  min-height: {h}px; padding: 0 12px; border-radius: {r}px;
  background-color: @glaze_surface; background-image: none; color: @glaze_text_primary;
  border: {sw} solid @glaze_border; box-shadow: {raised}; text-shadow: none;
  transition: background-color {micro}ms {ease}, border-color {micro}ms {ease}, box-shadow {micro}ms {ease};
}}
.fs-page button:hover, window.fs-dialog button:hover, headerbar.fs-headerbar button.fs-btn:hover {{ background-color: @glaze_surface_sunken; }}
.fs-page button:active, window.fs-dialog button:active, headerbar.fs-headerbar button.fs-btn:active,
.fs-page button:checked {{ background-color: @glaze_surface_sunken; box-shadow: {sunken}; }}
.fs-page button:disabled, headerbar.fs-headerbar button.fs-btn:disabled, window.fs-dialog button:disabled {{ opacity: 0.45; box-shadow: none; }}
headerbar.fs-headerbar button.fs-btn {{ background-color: transparent; border-color: transparent; box-shadow: none; padding: 0 9px; }}
headerbar.fs-headerbar button.fs-btn:hover {{ background-color: @glaze_surface_sunken; border-color: @glaze_border; }}
button.fs-primary, .fs-page button.fs-primary, window.fs-dialog button.fs-primary {{
  background-color: @glaze_accent; color: @glaze_accent_text; border-color: shade(@glaze_accent, 0.85);
}}
button.fs-primary label {{ color: @glaze_accent_text; }}
.fs-page button.fs-primary:hover, window.fs-dialog button.fs-primary:hover {{ background-color: shade(@glaze_accent, 1.06); }}
button.fs-danger, .fs-page button.fs-danger, window.fs-dialog button.fs-danger {{ color: @glaze_danger; }}
window.fs-dialog button.fs-danger.fs-primary {{ background-color: @glaze_danger; color: @glaze_canvas; border-color: @glaze_danger; }}
window.fs-dialog button.fs-danger.fs-primary label {{ color: @glaze_canvas; }}
.fs-page button.fs-icon-btn, .fs-page button.fs-icon-btn:hover {{ min-width: {h}px; padding: 0; }}
.fs-page button.fs-ghost {{ background-color: transparent; border-color: transparent; box-shadow: none; }}
.fs-page button.fs-ghost:hover {{ background-color: @glaze_surface_sunken; border-color: @glaze_border; }}
button.fs-link {{ background: none; border: none; box-shadow: none; padding: 0 4px; min-height: 0; color: @glaze_focus; }}
button.fs-link:hover label {{ text-decoration: underline; }}

/* Segmented control */
.fs-segmented {{
  background-color: @glaze_surface_sunken; border: {sw} solid @glaze_border; border-radius: {r}px; padding: 2px;
  box-shadow: {sunken};
}}
.fs-page .fs-segmented > button, .fs-page .fs-segmented > button:hover {{
  min-height: {seg}px; padding: 0 12px; border: {sw} solid transparent; background-color: transparent;
  box-shadow: none; border-radius: {rs}px; color: @glaze_text_secondary;
  transition: background-color {micro}ms {ease}, color {micro}ms {ease}, border-color {micro}ms {ease};
}}
.fs-page .fs-segmented > button:hover {{ color: @glaze_text_primary; background-color: alpha(@glaze_ink, 0.05); }}
.fs-page .fs-segmented > button:checked {{
  background-color: @glaze_surface_raised; color: @glaze_text_primary; border-color: @glaze_border;
  box-shadow: {raised};
}}
.fs-page .fs-segmented > button:checked label {{ font-weight: 600; }}

/* Switch: paper track, terracotta when on, the knob slides. */
switch {{
  background-color: @glaze_surface_sunken; border: {sw} solid @glaze_border; border-radius: 11px;
  min-width: 38px; min-height: 20px; padding: 0; box-shadow: {sunken}; color: transparent;
  transition: background-color {menu}ms {ease}, border-color {menu}ms {ease};
}}
switch:checked {{ background-color: @glaze_accent; border-color: shade(@glaze_accent, 0.85); }}
switch > slider {{
  min-width: 16px; min-height: 16px; margin: 1px; border-radius: 9px;
  background-color: @glaze_surface_raised; border: {sw} solid @glaze_border; box-shadow: none;
  transition: all {menu}ms {ease};
}}
switch:checked > slider {{ border-color: shade(@glaze_accent, 0.85); }}
switch image {{ color: transparent; }}
switch:disabled {{ opacity: 0.5; }}

/* Slider */
scale {{ padding: 8px 6px; min-width: 180px; }}
scale > trough {{
  background-color: @glaze_surface_sunken; border: {sw} solid @glaze_border; border-radius: 3px;
  min-height: 4px; box-shadow: none;
}}
scale > trough > highlight {{ background-color: @glaze_accent; border-radius: 3px; min-height: 4px; min-width: 0; margin: 0; border: none; }}
scale > trough > slider {{
  min-width: 16px; min-height: 16px; margin: -7px; border-radius: 9px;
  background-color: @glaze_surface_raised; border: {swc} solid @glaze_ink; box-shadow: none;
  transition: background-color {micro}ms {ease}, transform {micro}ms {ease};
}}
scale > trough > slider:hover {{ background-color: @glaze_accent_soft; }}
scale > marks.bottom label {{ color: @glaze_text_secondary; font-size: {tiny:.2}pt; }}
scale > marks indicator {{ background-color: @glaze_border; min-height: 4px; min-width: 1px; }}

/* Entries, spins, dropdowns */
entry, spinbutton {{
  min-height: {h}px; border-radius: {r}px; background-color: @glaze_surface; color: @glaze_text_primary;
  border: {sw} solid @glaze_border; box-shadow: {sunken}; background-image: none; padding: 0 8px;
  transition: border-color {micro}ms {ease}, box-shadow {micro}ms {ease};
}}
entry:focus-within, spinbutton:focus-within {{ border-color: @glaze_accent; box-shadow: 0 0 0 1px @glaze_accent; outline: none; }}
entry.error, spinbutton.error {{ border-color: @glaze_danger; box-shadow: 0 0 0 1px @glaze_danger; color: @glaze_text_primary; }}
entry selection, spinbutton selection {{ background-color: @glaze_accent_soft; color: @glaze_accent_soft_text; }}
spinbutton {{ padding: 0; }}
spinbutton > text {{ padding: 0 6px; min-width: 40px; }}
.fs-page spinbutton > button, .fs-page spinbutton > button:hover {{
  border: none; border-left: {sw} solid @glaze_border; border-radius: 0; box-shadow: none;
  background-color: transparent; min-width: 24px; padding: 0; color: @glaze_ink;
}}
.fs-page spinbutton > button:hover {{ background-color: @glaze_surface_sunken; }}
dropdown > button {{ min-width: 160px; }}
dropdown > button > box > stack > row.activatable {{ background: none; }}
popover.menu > contents, dropdown popover > contents, popover.background > contents {{
  background-color: @glaze_surface_raised; border: {sw} solid @glaze_border; border-radius: {cr}px;
  box-shadow: {shadow}; padding: 4px; color: @glaze_text_primary;
}}
popover > arrow {{ background: none; border: none; }}
popover listview > row, popover list > row {{ border-radius: {rs}px; padding: 4px 8px; min-height: 24px; }}
popover listview > row:hover, popover list > row:hover {{ background-color: @glaze_surface_sunken; }}
popover listview > row:selected, popover list > row:selected {{ background-color: @glaze_accent_soft; color: @glaze_accent_soft_text; }}
checkbutton {{ padding: 2px; }}
checkbutton > check {{
  min-width: 15px; min-height: 15px; border-radius: {rs}px; border: {swc} solid @glaze_ink;
  background-color: @glaze_surface; background-image: none; box-shadow: none; color: @glaze_accent_text;
  transition: background-color {micro}ms {ease};
}}
checkbutton > check:checked {{ background-color: @glaze_accent; border-color: shade(@glaze_accent, 0.8); }}

/* Tiles: theme, wallpaper, swatches */
button.fs-tile {{
  padding: 6px; border-radius: {cr}px; background-color: @glaze_surface; background-image: none;
  border: {sw} solid @glaze_border; box-shadow: none;
  transition: border-color {micro}ms {ease}, background-color {micro}ms {ease}, box-shadow {micro}ms {ease};
}}
button.fs-tile:hover {{ border-color: shade(@glaze_border, 0.85); background-color: @glaze_surface_sunken; }}
button.fs-tile.fs-selected {{ border-color: @glaze_accent; box-shadow: 0 0 0 1px @glaze_accent; }}
button.fs-tile picture, button.fs-tile .fs-thumb {{ border-radius: {rs}px; }}
flowbox.fs-tiles > flowboxchild {{ padding: 4px; border-radius: {cr}px; }}
flowbox.fs-tiles > flowboxchild:selected {{ background: none; }}
.fs-tile-title {{ font-weight: 600; }}
.fs-tile-sub {{ color: @glaze_text_secondary; font-size: {small:.2}pt; }}
button.fs-swatch {{
  min-width: 22px; min-height: 22px; padding: 0; border-radius: 12px; border: 1px solid alpha(@glaze_ink, 0.35);
  box-shadow: none; transition: box-shadow {micro}ms {ease};
}}
button.fs-swatch.fs-selected {{ box-shadow: 0 0 0 2px @glaze_surface_raised, 0 0 0 3px @glaze_ink; }}
.fs-thumb-frame {{ border: {sw} solid @glaze_border; border-radius: {rs}px; background-color: @glaze_surface_sunken; }}

/* Scrollbars: thin ink rails */
scrollbar {{ background: none; border: none; }}
scrollbar slider {{ background-color: alpha(@glaze_ink, 0.28); min-width: 5px; min-height: 5px; border-radius: 3px; border: none; }}
scrollbar slider:hover {{ background-color: alpha(@glaze_ink, 0.45); }}

/* Dialogs */
window.fs-dialog .fs-dialog-title {{ font-family: {display}; font-weight: 700; font-size: {ct:.2}pt; }}
window.fs-dialog .fs-dialog-card {{ padding: 18px 20px 14px 20px; }}
.fs-capture {{
  background-color: @glaze_surface_sunken; border: {sw} dashed @glaze_border; border-radius: {cr}px;
  padding: 14px; min-height: 40px;
}}
.fs-capture.fs-armed {{ border-color: @glaze_accent; }}
.fs-preview-box {{ border: {sw} solid @glaze_border; border-radius: {rs}px; background-color: @glaze_surface_sunken; }}
/* GTK4 sets :focus-visible on every ancestor of the focus widget, so ring
   only the focused widget itself; entries show focus with their border. */
*:focus-visible {{ outline: none; }}
*:focus:focus-visible {{ outline: 2px solid @glaze_focus; outline-offset: 1px; }}
text:focus:focus-visible, entry *:focus:focus-visible, spinbutton *:focus:focus-visible {{ outline: none; }}
"#,
        paper_panel = paper("panel"),
        paper_card = paper("surface_raised"),
        ts = ty.clock_size_pt + 2.0,
        pt = ty.clock_size_pt + 9.0,
        ct = ty.clock_size_pt + 2.25,
        tiny = (small - 0.5).max(6.5),
        rs = (r - 1).max(0),
        seg = (h - 6).max(18),
        mono = t.typography.mono_stack(),
    ));
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use glaze_design::ColorScheme;

    fn inputs() -> StyleInputs {
        StyleInputs {
            reduced_motion: false,
            duration_scale: 1.0,
            reduced_effects: false,
            texture_url: Some("file:///tmp/grain-light.png".into()),
            texture_intensity: 0.5,
        }
    }

    #[test]
    fn no_raw_hex_in_settings_rules_and_no_blur() {
        for k in ThemeKind::ALL {
            for sc in ColorScheme::ALL {
                let t = ThemeTokens::for_kind(k, sc);
                let all = css(&t, &inputs());
                let ours = all.split("/* ---- glaze-settings").nth(1).unwrap();
                assert!(
                    !ours.contains('#') || ours.matches('#').count() == 0,
                    "raw hex in settings CSS"
                );
                for line in ours.lines().filter(|l| l.contains("box-shadow")) {
                    // Offsets are "x y blur spread": blur must be 0 where present.
                    assert!(
                        !line.contains("px 3px 3px") && !line.contains("blur"),
                        "{line}"
                    );
                }
            }
        }
    }

    #[test]
    fn reduced_motion_zeroes_settings_transitions_and_effects_drop_texture() {
        let t = ThemeTokens::for_kind(ThemeKind::Clay, ColorScheme::Light);
        let mut i = inputs();
        i.reduced_motion = true;
        let c = css(&t, &i);
        let ours = c.split("/* ---- glaze-settings").nth(1).unwrap();
        assert!(ours.contains("transition: background-color 0ms"));
        assert!(!ours.contains("140ms"));
        let mut i = inputs();
        assert!(css(&t, &i).contains("url(\"file:///tmp/grain-light.png\")"));
        i.reduced_effects = true;
        assert!(!css(&t, &i)
            .split("/* ---- glaze-settings")
            .nth(1)
            .unwrap()
            .contains("url("));
    }
}
