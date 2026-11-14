//! Building the frame: layer surfaces, windows with Glaze paper frames,
//! animated transforms, closing-window ghosts and the client cursor.
//!
//! Elements are produced top-most first (Smithay's convention) and handed to
//! an `OutputDamageTracker`, so only what changed is redrawn. Running
//! animations change their elements every frame; everything else is static
//! and costs nothing once drawn.

use smithay::wayland::seat::WaylandFocus;
use std::time::Duration;

use smithay::backend::renderer::element::memory::MemoryRenderBufferRenderElement;
use smithay::backend::renderer::element::solid::{SolidColorBuffer, SolidColorRenderElement};
use smithay::backend::renderer::element::surface::{
    render_elements_from_surface_tree, WaylandSurfaceRenderElement,
};
use smithay::backend::renderer::element::texture::TextureRenderElement;
use smithay::backend::renderer::element::utils::{
    Relocate, RelocateRenderElement, RescaleRenderElement,
};
use smithay::backend::renderer::element::{AsRenderElements, Id, Kind};
use smithay::backend::renderer::gles::{GlesRenderer, GlesTexture};
use smithay::backend::renderer::utils::RendererSurfaceStateUserData;
use smithay::backend::renderer::Renderer;
use smithay::desktop::utils::send_frames_surface_tree;
use smithay::desktop::{layer_map_for_output, Window};
use smithay::input::pointer::{CursorImageAttributes, CursorImageStatus};
use smithay::output::Output;
use smithay::reexports::wayland_server::Resource;
use smithay::utils::{Logical, Physical, Point, Scale, Size};
use smithay::wayland::compositor::{with_states, with_surface_tree_downward, TraversalAction};
use smithay::wayland::shell::wlr_layer::Layer;

use crate::anim::{RectF, Visual};
use crate::frame::{FrameGeometry, Insets, Rgba, TITLE_H};
use crate::state::{GhostSurface, State};

smithay::backend::renderer::element::render_elements! {
    pub GlazeElement<=GlesRenderer>;
    Surface=WaylandSurfaceRenderElement<GlesRenderer>,
    Window=RelocateRenderElement<RescaleRenderElement<WaylandSurfaceRenderElement<GlesRenderer>>>,
    Solid=SolidColorRenderElement,
    Memory=MemoryRenderBufferRenderElement<GlesRenderer>,
    Texture=TextureRenderElement<GlesTexture>,
}

/// Solid-colour buffers for one frame: left, right, bottom hairlines and the
/// two shadow strips. Kept per window so damage tracking sees stable ids.
pub struct FrameParts {
    pub bufs: [SolidColorBuffer; 5],
}

impl Default for FrameParts {
    fn default() -> Self {
        Self {
            bufs: std::array::from_fn(|_| SolidColorBuffer::default()),
        }
    }
}

fn phys(p: (f64, f64), scale: f64) -> Point<i32, Physical> {
    Point::from(((p.0 * scale).round() as i32, (p.1 * scale).round() as i32))
}

/// Map an outer-frame-relative rect through the visual transform.
fn map_rect(rest: &RectF, vis: &RectF, r: glaze_wm::Rect) -> RectF {
    let (x0, y0) = vis.map_from(rest, f64::from(r.x), f64::from(r.y));
    let (x1, y1) = vis.map_from(rest, f64::from(r.x + r.w), f64::from(r.y + r.h));
    RectF {
        x: x0,
        y: y0,
        w: (x1 - x0).max(0.0),
        h: (y1 - y0).max(0.0),
    }
}

fn solid(
    buf: &mut SolidColorBuffer,
    r: RectF,
    color: Rgba,
    alpha: f32,
    scale: f64,
) -> Option<GlazeElement> {
    let x0 = r.x.round() as i32;
    let y0 = r.y.round() as i32;
    let w = ((r.x + r.w).round() as i32 - x0).max(0);
    let h = ((r.y + r.h).round() as i32 - y0).max(0);
    if w == 0 || h == 0 {
        return None;
    }
    buf.update((w, h), color.premultiplied());
    Some(GlazeElement::Solid(SolidColorRenderElement::from_buffer(
        buf,
        phys((f64::from(x0), f64::from(y0)), scale),
        scale,
        alpha,
        Kind::Unspecified,
    )))
}

/// Shift a global logical rect into output-local coordinates.
fn local(r: RectF, off: (f64, f64)) -> RectF {
    RectF {
        x: r.x - off.0,
        y: r.y - off.1,
        ..r
    }
}

/// Frame (title bar, hairlines, shadow) elements for one window, top first.
#[allow(clippy::too_many_arguments)]
fn frame_elements(
    renderer: &mut GlesRenderer,
    out: &mut Vec<GlazeElement>,
    parts: &mut FrameParts,
    title: Option<&smithay::backend::renderer::element::memory::MemoryRenderBuffer>,
    prev_title: Option<(&smithay::backend::renderer::element::memory::MemoryRenderBuffer, f32)>,
    rest: glaze_wm::Rect,
    visual: Visual,
    focused: bool,
    palette: &crate::frame::Palette,
    scale: f64,
    off: (f64, f64),
) {
    let rest_f = RectF::from(rest);
    let vis = visual.rect;
    let g = FrameGeometry::for_outer(rest);
    // Focus change: the previous title bar fades out on top of the new one.
    if let Some((buffer, a)) = prev_title {
        let tr = local(map_rect(&rest_f, &vis, g.title), off);
        let size: Size<i32, Logical> =
            (tr.w.round().max(1.0) as i32, tr.h.round().max(1.0) as i32).into();
        if let Ok(e) = MemoryRenderBufferRenderElement::from_buffer(
            renderer,
            Point::<f64, Physical>::from((tr.x * scale, tr.y * scale)),
            buffer,
            Some(visual.alpha * a),
            None,
            Some(size),
            Kind::Unspecified,
        ) {
            out.push(GlazeElement::Memory(e));
        }
    }
    if let Some(buffer) = title {
        let tr = local(map_rect(&rest_f, &vis, g.title), off);
        let size: Size<i32, Logical> =
            (tr.w.round().max(1.0) as i32, tr.h.round().max(1.0) as i32).into();
        if let Ok(e) = MemoryRenderBufferRenderElement::from_buffer(
            renderer,
            Point::<f64, Physical>::from((tr.x * scale, tr.y * scale)),
            buffer,
            Some(visual.alpha),
            None,
            Some(size),
            Kind::Unspecified,
        ) {
            out.push(GlazeElement::Memory(e));
        }
    }
    let shadow = if focused {
        palette.shadow
    } else {
        palette.shadow_inactive
    };
    let [left, right, bottom, sr, sb] = &mut parts.bufs;
    for (buf, r, c) in [
        (left, g.left, palette.stroke),
        (right, g.right, palette.stroke),
        (bottom, g.bottom, palette.stroke),
        (sr, g.shadow_right, shadow),
        (sb, g.shadow_bottom, shadow),
    ] {
        if let Some(e) = solid(buf, local(map_rect(&rest_f, &vis, r), off), c, visual.alpha, scale) {
            out.push(e);
        }
    }
}

/// The snap preview: a translucent, accent-tinted paper sheet with a
/// terracotta hairline edge and a faint hard offset sheet (no blur).
fn preview_elements(
    out: &mut Vec<GlazeElement>,
    parts: &mut FrameParts,
    r: RectF,
    alpha: f32,
    palette: &crate::frame::Palette,
    scale: f64,
) {
    let [top, bottom, left, right, fill] = &mut parts.bufs;
    let edge = palette.accent;
    let edges = [
        (top, RectF { h: 1.0, ..r }),
        (bottom, RectF { y: r.y + r.h - 1.0, h: 1.0, ..r }),
        (left, RectF { w: 1.0, ..r }),
        (right, RectF { x: r.x + r.w - 1.0, w: 1.0, ..r }),
    ];
    for (buf, rr) in edges {
        if let Some(e) = solid(buf, rr, edge, 0.9 * alpha, scale) {
            out.push(e);
        }
    }
    let sheet = palette.title_bg.mix(palette.accent, 0.16);
    let inner = RectF {
        x: r.x + 1.0,
        y: r.y + 1.0,
        w: (r.w - 2.0).max(0.0),
        h: (r.h - 2.0).max(0.0),
    };
    if let Some(e) = solid(fill, inner, sheet, 0.42 * alpha, scale) {
        out.push(e);
    }
}

/// Build every element for `output`, top-most first.
pub fn output_elements(
    renderer: &mut GlesRenderer,
    state: &mut State,
    output: &Output,
) -> Vec<GlazeElement> {
    let scale = output.current_scale().fractional_scale();
    let now = state.now();
    let mut out: Vec<GlazeElement> = Vec::new();
    let output_geo = state.space.output_geometry(output).unwrap_or_default();
    let off = (f64::from(output_geo.loc.x), f64::from(output_geo.loc.y));
    let pointer_here = output_geo.to_f64().contains(state.pointer_location);

    // Log-out fade to ink, above everything (including the pointer).
    if let Some(t0) = state.exit_fade {
        let d = state.motion.duration(crate::anim::durations::LOGOUT);
        let t = if d.is_zero() { 1.0 } else { (now.saturating_sub(t0).as_secs_f64() / d.as_secs_f64()).min(1.0) };
        let full = RectF {
            x: 0.0,
            y: 0.0,
            w: f64::from(output_geo.size.w),
            h: f64::from(output_geo.size.h),
        };
        if let Some(e) = solid(&mut state.fade_buf, full, Rgba::hex(0x2B2926), t as f32, scale) {
            out.push(e);
        }
    }

    // Pointer. On the DRM backend Glaze draws it (theme image for named
    // cursors and the frame/grab cursor); nested, the parent draws its own
    // pointer and only client cursor surfaces are composited.
    let named = state.frame_cursor.or(match &state.cursor_status {
        CursorImageStatus::Named(icon) => Some(*icon),
        _ => None,
    });
    let hidden = matches!(state.cursor_status, CursorImageStatus::Hidden) && state.frame_cursor.is_none();
    if state.software_cursor && pointer_here && !hidden {
        if let Some(icon) = named {
            let img = state
                .cursors
                .get_or_insert_with(crate::cursor::CursorCache::from_env)
                .get(icon);
            let p = state.pointer_location - img.hotspot.to_f64();
            if let Ok(e) = MemoryRenderBufferRenderElement::from_buffer(
                renderer,
                Point::<f64, Physical>::from(((p.x - off.0) * scale, (p.y - off.1) * scale)),
                &img.buffer,
                None,
                None,
                None,
                Kind::Cursor,
            ) {
                out.push(GlazeElement::Memory(e));
            }
        }
    }
    if let (CursorImageStatus::Surface(surface), None) = (&state.cursor_status, state.frame_cursor) {
        if surface.is_alive() && pointer_here {
            let hotspot = with_states(surface, |s| {
                s.data_map
                    .get::<std::sync::Mutex<CursorImageAttributes>>()
                    .map(|a| a.lock().unwrap().hotspot)
                    .unwrap_or_default()
            });
            let p = state.pointer_location - hotspot.to_f64();
            out.extend(
                render_elements_from_surface_tree::<_, WaylandSurfaceRenderElement<GlesRenderer>>(
                    renderer,
                    surface,
                    phys((p.x - off.0, p.y - off.1), scale),
                    scale,
                    1.0,
                    Kind::Cursor,
                )
                .into_iter()
                .map(GlazeElement::Surface),
            );
        }
    }

    // Alt+Tab switcher: centred on the primary output, above everything.
    if let (Some((_, _, buf, (sw, sh))), Some(primary)) = (&state.switcher, state.output.as_ref()) {
        if primary == output {
            let d = state.motion.duration(crate::anim::durations::SWITCHER);
            let t = if d.is_zero() {
                1.0
            } else {
                (now.saturating_sub(state.switcher_since).as_secs_f64() / d.as_secs_f64()).min(1.0)
            };
            let t = crate::anim::Easing::OutCubic.apply(t);
            let k = 0.96 + 0.04 * t;
            let (w, h) = (f64::from(*sw) * k, f64::from(*sh) * k);
            let x = (f64::from(output_geo.size.w) - w) / 2.0;
            let y = (f64::from(output_geo.size.h) - h) / 2.0;
            if let Ok(e) = MemoryRenderBufferRenderElement::from_buffer(
                renderer,
                Point::<f64, Physical>::from((x * scale, y * scale)),
                buf,
                Some(t as f32),
                None,
                Some((w.round() as i32, h.round() as i32).into()),
                Kind::Unspecified,
            ) {
                out.push(GlazeElement::Memory(e));
            }
        }
    }

    // Locked: the lock surface of this output and nothing else (the clear
    // colour is the paper background, also shown if the locker died).
    if state.locked {
        if let Some((_, lock)) = state.lock_surfaces.iter().find(|(o, s)| o == output && s.alive()) {
            let d = state.motion.duration(crate::anim::durations::LOCK);
            let a = if d.is_zero() {
                1.0
            } else {
                (now.saturating_sub(state.locked_since).as_secs_f64() / d.as_secs_f64()).min(1.0)
            };
            out.extend(
                render_elements_from_surface_tree::<_, WaylandSurfaceRenderElement<GlesRenderer>>(
                    renderer,
                    lock.wl_surface(),
                    phys((0.0, 0.0), scale),
                    scale,
                    a as f32,
                    Kind::Unspecified,
                )
                .into_iter()
                .map(GlazeElement::Surface),
            );
        }
        return out;
    }

    let popup_d = state.motion.duration(crate::anim::durations::POPUP);
    let popup_slide = state.motion.intensity.slide() * 0.6;
    type Seen = std::collections::HashMap<smithay::reexports::wayland_server::backend::ObjectId, Duration>;
    let layer_elements =
        |renderer: &mut GlesRenderer, layers: &[Layer], out: &mut Vec<GlazeElement>, fade: bool, layer_seen: &mut Seen| {
            let map = layer_map_for_output(output);
            for layer in layers {
                for l in map.layers_on(*layer).rev() {
                    let Some(geo) = map.layer_geometry(l) else {
                        continue;
                    };
                    // Menus, toasts, the start menu (not the bar): fade in
                    // and settle a few px when they first appear.
                    let (alpha, dy) = if fade && !l.namespace().ends_with("-panel") {
                        let id = smithay::reexports::wayland_server::Resource::id(l.wl_surface());
                        let t0 = *layer_seen.entry(id).or_insert(now);
                        let t = if popup_d.is_zero() {
                            1.0
                        } else {
                            (now.saturating_sub(t0).as_secs_f64() / popup_d.as_secs_f64()).min(1.0)
                        };
                        let e = crate::anim::Easing::OutCubic.apply(t);
                        (e as f32, (1.0 - e) * popup_slide)
                    } else {
                        (1.0, 0.0)
                    };
                    // Layer geometry is already output-local.
                    let loc = Point::<f64, Logical>::from((f64::from(geo.loc.x), f64::from(geo.loc.y) + dy))
                        .to_physical_precise_round(scale);
                    let elems: Vec<WaylandSurfaceRenderElement<GlesRenderer>> =
                        l.render_elements(renderer, loc, Scale::from(scale), alpha);
                    out.extend(elems.into_iter().map(GlazeElement::Surface));
                }
            }
        };
    layer_elements(renderer, &[Layer::Overlay, Layer::Top], &mut out, true, &mut state.layer_seen);

    // X11 override-redirect windows (menus, tooltips) above all windows.
    for x in state.x11_override_windows() {
        let Some(surface) = x.wl_surface() else {
            continue;
        };
        let g = x.geometry();
        out.extend(
            render_elements_from_surface_tree::<_, WaylandSurfaceRenderElement<GlesRenderer>>(
                renderer,
                &surface,
                phys((f64::from(g.loc.x) - off.0, f64::from(g.loc.y) - off.1), scale),
                scale,
                1.0,
                Kind::Unspecified,
            )
            .into_iter()
            .map(GlazeElement::Surface),
        );
    }

    // Closing windows (ghosts) above the live ones.
    let palette = state.palette;
    for ghost in state.ghosts.iter_mut() {
        let visual = ghost.anim.sample(now);
        let rest_f = RectF::from(ghost.rest);
        let client = ghost.insets.client(ghost.rest);
        for s in &ghost.surfaces {
            let r = local(
                map_rect(
                    &rest_f,
                    &visual.rect,
                    glaze_wm::Rect::new(
                        client.x + s.offset.x,
                        client.y + s.offset.y,
                        s.size.w,
                        s.size.h,
                    ),
                ),
                off,
            );
            out.push(GlazeElement::Texture(
                TextureRenderElement::from_static_texture(
                    s.id.clone(),
                    s.context.clone(),
                    Point::<f64, Physical>::from((r.x * scale, r.y * scale)),
                    s.texture.clone(),
                    s.scale,
                    s.transform,
                    Some(visual.alpha),
                    None,
                    Some((r.w.round().max(1.0) as i32, r.h.round().max(1.0) as i32).into()),
                    None,
                    Kind::Unspecified,
                ),
            ));
        }
        if ghost.insets != Insets::NONE {
            frame_elements(
                renderer,
                &mut out,
                &mut ghost.parts,
                ghost.title_buffer.as_ref(),
                None,
                ghost.rest,
                visual,
                false,
                &palette,
                scale,
                off,
            );
        }
    }

    // Windows, top-most first; the snap preview sits just under the window
    // being dragged.
    let focused = state.wm.focused().map(str::to_owned);
    let wm = &state.wm;
    let preview = state.snap_preview.as_ref().map(|p| (p.window.clone(), p.sample(now)));
    let preview_parts = &mut state.preview_parts;
    let mut preview_done = preview.is_none();
    let resizing = state.resizing.clone();
    let motion = state.motion;
    for entry in state.windows.iter_mut().rev() {
        let (Some(target), true) = (entry.rest, entry.shown) else {
            continue;
        };
        let animating = entry.anim.is_some_and(|a| !a.is_done(now));
        // Draw the frame around the buffer the client actually committed,
        // not the size we asked for: while a client catches up with a
        // resize (llvmpipe clients take 50-100 ms), frame and content stay
        // glued instead of showing gaps or clipped content. During an
        // interactive resize the edge opposite the dragged one stays put.
        let rest = if animating {
            target
        } else {
            let ins = entry.insets(wm);
            let actual = entry.window.geometry().size;
            let want = ins.client(target);
            if actual.w > 0 && actual.h > 0 && (actual.w, actual.h) != (want.w, want.h) {
                let w = actual.w + ins.left + ins.right;
                let h = actual.h + ins.top + ins.bottom;
                let edges = resizing
                    .as_ref()
                    .filter(|(id, _)| entry.id.as_deref() == Some(id.as_str()))
                    .map(|(_, e)| *e)
                    .unwrap_or_default();
                let x = if edges.left { target.x + target.w - w } else { target.x };
                let y = if edges.top { target.y + target.h - h } else { target.y };
                glaze_wm::Rect::new(x, y, w, h)
            } else {
                target
            }
        };
        let running = entry.anim.filter(|a| !a.is_done(now));
        let mut visual = running.map(|a| a.sample(now)).unwrap_or(Visual {
            rect: RectF::from(rest),
            alpha: 1.0,
        });
        let morphing = running.is_some_and(|a| a.kind == crate::anim::AnimKind::Move) && entry.morph.is_some();
        if running.is_some_and(|a| a.kind == crate::anim::AnimKind::Move) && !morphing {
            // No snapshot: glide the live window at its final size (1:1).
            visual.rect = RectF {
                x: visual.rect.x.round(),
                y: visual.rect.y.round(),
                w: f64::from(rest.w),
                h: f64::from(rest.h),
            };
        }
        let prev_title = entry.prev_title.as_ref().and_then(|(b, t0)| {
            let d = motion.duration(crate::anim::durations::FOCUS);
            let t = if d.is_zero() { 1.0 } else { now.saturating_sub(*t0).as_secs_f64() / d.as_secs_f64() };
            (t < 1.0).then(|| (b.clone(), (1.0 - t) as f32))
        });
        if visual.alpha <= 0.0 {
            continue;
        }
        let insets = entry.insets(wm);
        let client = insets.client(rest);
        let geo_loc = entry.window.geometry().loc;
        let surface_loc = phys(
            (
                f64::from(client.x - geo_loc.x) - off.0,
                f64::from(client.y - geo_loc.y) - off.1,
            ),
            scale,
        );
        let rest_f = RectF::from(rest);
        let sx = if rest_f.w > 0.0 {
            visual.rect.w / rest_f.w
        } else {
            1.0
        };
        let sy = if rest_f.h > 0.0 {
            visual.rect.h / rest_f.h
        } else {
            1.0
        };
        let origin = phys((rest_f.x - off.0, rest_f.y - off.1), scale);
        let offset = phys((visual.rect.x - rest_f.x, visual.rect.y - rest_f.y), scale);
        if let (true, Some((old, surfaces))) = (morphing, entry.morph.as_ref()) {
            // Size morph: the frozen snapshot scales from the old rect to the
            // current visual rect (contents never jump).
            let old_f = RectF::from(*old);
            let old_client = insets.client(*old);
            for sfc in surfaces {
                let r = local(
                    map_rect(
                        &old_f,
                        &visual.rect,
                        glaze_wm::Rect::new(
                            old_client.x + sfc.offset.x,
                            old_client.y + sfc.offset.y,
                            sfc.size.w,
                            sfc.size.h,
                        ),
                    ),
                    off,
                );
                out.push(GlazeElement::Texture(TextureRenderElement::from_static_texture(
                    sfc.id.clone(),
                    sfc.context.clone(),
                    Point::<f64, Physical>::from((r.x * scale, r.y * scale)),
                    sfc.texture.clone(),
                    sfc.scale,
                    sfc.transform,
                    Some(visual.alpha),
                    None,
                    Some((r.w.round().max(1.0) as i32, r.h.round().max(1.0) as i32).into()),
                    None,
                    Kind::Unspecified,
                )));
            }
        } else {
            let elems: Vec<WaylandSurfaceRenderElement<GlesRenderer>> =
                entry
                    .window
                    .render_elements(renderer, surface_loc, Scale::from(scale), visual.alpha);
            out.extend(elems.into_iter().map(|e| {
                GlazeElement::Window(RelocateRenderElement::from_element(
                    RescaleRenderElement::from_element(e, origin, Scale { x: sx, y: sy }),
                    offset,
                    Relocate::Relative,
                ))
            }));
        }
        if entry.ssd && insets != Insets::NONE {
            let is_focused = entry.id.as_deref() == focused.as_deref();
            frame_elements(
                renderer,
                &mut out,
                &mut entry.parts,
                entry.title_buffer.as_ref(),
                prev_title.as_ref().map(|(b, a)| (b, *a)),
                rest,
                visual,
                is_focused,
                &palette,
                scale,
                off,
            );
        }
        if let Some((pid, (r, alpha))) = &preview {
            if !preview_done && entry.id.as_deref() == Some(pid.as_str()) {
                preview_elements(&mut out, preview_parts, local(*r, off), *alpha, &palette, scale);
                preview_done = true;
            }
        }
    }
    if let (false, Some((_, (r, alpha)))) = (preview_done, &preview) {
        preview_elements(&mut out, preview_parts, local(*r, off), *alpha, &palette, scale);
    }

    layer_elements(renderer, &[Layer::Bottom, Layer::Background], &mut out, false, &mut state.layer_seen);
    let _ = TITLE_H;
    out
}

/// Send frame callbacks to everything on screen.
pub fn send_frames(state: &State, output: &Output, time: Duration) {
    for entry in &state.windows {
        if entry.shown {
            entry
                .window
                .send_frame(output, time, Some(Duration::ZERO), |_, _| {
                    Some(output.clone())
                });
        }
    }
    for x in state.x11_override_windows() {
        if let Some(s) = x.wl_surface() {
            send_frames_surface_tree(&s, output, time, Some(Duration::ZERO), |_, _| Some(output.clone()));
        }
    }
    for (o, lock) in &state.lock_surfaces {
        if o == output && lock.alive() {
            send_frames_surface_tree(lock.wl_surface(), output, time, Some(Duration::ZERO), |_, _| {
                Some(output.clone())
            });
        }
    }
    let map = layer_map_for_output(output);
    for l in map.layers() {
        l.send_frame(output, time, Some(Duration::ZERO), |_, _| {
            Some(output.clone())
        });
    }
    if let CursorImageStatus::Surface(surface) = &state.cursor_status {
        if surface.is_alive() {
            send_frames_surface_tree(surface, output, time, Some(Duration::ZERO), |_, _| {
                Some(output.clone())
            });
        }
    }
}

/// Snapshot the textures of a window's surface tree (for its close
/// animation). Must run before the surfaces are destroyed.
pub fn capture_window(renderer: &GlesRenderer, window: &Window) -> Vec<GhostSurface> {
    let Some(root) = window.wl_surface() else {
        return Vec::new();
    };
    let context = renderer.context_id();
    let geo_loc = window.geometry().loc;
    let mut out = Vec::new();
    with_surface_tree_downward(
        &root,
        Point::<i32, Logical>::from((0, 0)),
        // The tree walk already holds each surface's state lock: read it
        // through `states`, never `with_renderer_surface_state` (that locks
        // again and deadlocks the compositor).
        |_surface, states, loc| {
            let mut loc = *loc;
            let view = states
                .data_map
                .get::<RendererSurfaceStateUserData>()
                .and_then(|d| d.lock().unwrap().view());
            match view.map(Some) {
                Some(Some(view)) => {
                    loc += view.offset;
                    TraversalAction::DoChildren(loc)
                }
                _ => TraversalAction::SkipChildren,
            }
        },
        |_surface, states, loc| {
            let captured = states
                .data_map
                .get::<RendererSurfaceStateUserData>()
                .and_then(|d| {
                    let s = d.lock().unwrap();
                    let view = s.view()?;
                    let texture = s.texture::<GlesTexture>(context.clone())?.clone();
                    Some((texture, view, s.buffer_scale(), s.buffer_transform()))
                });
            if let Some((texture, view, buffer_scale, transform)) = captured {
                out.push(GhostSurface {
                    id: Id::new(),
                    texture,
                    context: context.clone(),
                    offset: *loc + view.offset - geo_loc,
                    size: view.dst,
                    scale: buffer_scale,
                    transform,
                });
            }
        },
        |_, _, _| true,
    );
    out
}
