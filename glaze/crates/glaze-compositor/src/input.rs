//! Input routing: keyboard shortcuts, pointer hit-testing over Glaze frames,
//! focus-on-click, and the move/resize grabs.
//!
//! Generic over Smithay's [`InputBackend`], so the winit backend (nested) and
//! the libinput backend (tty, next phase) share it.

use std::time::{Duration, Instant};

use glaze_wm::{detect_zone, Key, Mods, Rect};
use smithay::backend::input::{
    AbsolutePositionEvent, Axis, AxisSource, ButtonState, Event, InputBackend, InputEvent,
    KeyState, KeyboardKeyEvent, PointerAxisEvent, PointerButtonEvent, PointerMotionEvent,
};
use smithay::desktop::{layer_map_for_output, WindowSurfaceType};
use smithay::input::keyboard::{keysyms, FilterResult, Keysym};
use smithay::input::pointer::{
    AxisFrame, ButtonEvent, CursorIcon, CursorImageStatus, Focus, GestureHoldBeginEvent,
    GestureHoldEndEvent, GesturePinchBeginEvent, GesturePinchEndEvent, GesturePinchUpdateEvent,
    GestureSwipeBeginEvent, GestureSwipeEndEvent, GestureSwipeUpdateEvent, GrabStartData,
    MotionEvent, PointerGrab, PointerInnerHandle, RelativeMotionEvent,
};
use smithay::input::Seat;
use smithay::reexports::wayland_server::protocol::wl_surface::WlSurface;
use smithay::utils::{Logical, Point, Serial, SERIAL_COUNTER};
use smithay::wayland::shell::wlr_layer::{KeyboardInteractivity, Layer};
use tracing::info;

use crate::frame::{self, Button, Edges, FramePart};
use crate::state::State;

const BTN_LEFT: u32 = 0x110;
const BTN_RIGHT: u32 = 0x111;
const DOUBLE_CLICK: Duration = Duration::from_millis(400);

/// What is under the pointer.
pub enum Under {
    /// A client surface and its origin in global coordinates.
    Surface(WlSurface, Point<f64, Logical>),
    /// A Glaze frame part of window `id`.
    Frame(String, FramePart),
}

fn mods_of(m: &smithay::input::keyboard::ModifiersState) -> Mods {
    Mods {
        ctrl: m.ctrl,
        alt: m.alt,
        shift: m.shift,
        super_: m.logo,
    }
}

/// Map an xkb keysym to a `glaze-wm` key.
pub fn key_of(sym: Keysym) -> Option<Key> {
    let raw = sym.raw();
    Some(match raw {
        keysyms::KEY_Tab | keysyms::KEY_ISO_Left_Tab => Key::Tab,
        keysyms::KEY_Escape => Key::Escape,
        keysyms::KEY_Return | keysyms::KEY_KP_Enter => Key::Enter,
        keysyms::KEY_space => Key::Space,
        keysyms::KEY_Left => Key::Left,
        keysyms::KEY_Right => Key::Right,
        keysyms::KEY_Up => Key::Up,
        keysyms::KEY_Down => Key::Down,
        keysyms::KEY_Home => Key::Home,
        keysyms::KEY_End => Key::End,
        keysyms::KEY_Page_Up => Key::PageUp,
        keysyms::KEY_Page_Down => Key::PageDown,
        keysyms::KEY_Delete => Key::Delete,
        keysyms::KEY_BackSpace => Key::Backspace,
        r if (keysyms::KEY_F1..=keysyms::KEY_F12).contains(&r) => {
            Key::Function((r - keysyms::KEY_F1 + 1) as u8)
        }
        _ => {
            let c = sym.key_char()?;
            if c.is_ascii_graphic() {
                Key::Char(c.to_ascii_lowercase())
            } else {
                return None;
            }
        }
    })
}

/// Compositor bindings that mirror the Sway session's keys (sway.conf and
/// `glaze-snap`), checked before the `glaze-wm` keymap.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompAction {
    Snap(glaze_wm::SnapZone),
    RestoreOrMinimize,
    Minimize,
    RestoreLastMinimized,
    ToggleFullscreen,
    ToggleFloating,
    Terminal,
    Exit,
    Lock,
}

/// The Glaze/Sway key table (Super = Mod4).
pub fn comp_binding(m: Mods, key: &Key) -> Option<CompAction> {
    use glaze_wm::SnapZone::*;
    let only_super = m.super_ && !m.ctrl && !m.alt && !m.shift;
    let super_shift = m.super_ && m.shift && !m.ctrl && !m.alt;
    let super_ctrl = m.super_ && m.ctrl && !m.shift && !m.alt;
    Some(match key {
        Key::Down if only_super => CompAction::RestoreOrMinimize,
        Key::Left if super_shift => CompAction::Snap(TopLeft),
        Key::Right if super_shift => CompAction::Snap(TopRight),
        Key::Left if super_ctrl => CompAction::Snap(BottomLeft),
        Key::Right if super_ctrl => CompAction::Snap(BottomRight),
        Key::Up if super_shift => CompAction::Snap(TopHalf),
        Key::Down if super_shift => CompAction::Snap(BottomHalf),
        Key::Char('c') if only_super => CompAction::Snap(Center),
        Key::Char('m') if only_super => CompAction::Minimize,
        Key::Char('m') if super_shift => CompAction::RestoreLastMinimized,
        Key::Char('f') if only_super => CompAction::ToggleFullscreen,
        Key::Space if super_shift => CompAction::ToggleFloating,
        Key::Enter if only_super => CompAction::Terminal,
        Key::Char('t') if m.ctrl && m.alt && !m.super_ && !m.shift => CompAction::Terminal,
        Key::Char('e') if super_shift => CompAction::Exit,
        Key::Char('l') if only_super => CompAction::Lock,
        _ => return None,
    })
}

enum KeyAction {
    Wm(glaze_wm::WmAction),
    Comp(CompAction),
    Quit,
    /// Ctrl+Alt+F<n>: switch to VT n (session backend).
    Vt(i32),
    /// Release of a key whose press was a shortcut: swallow it.
    Swallow,
}

/// Feed one backend input event into the compositor.
pub fn process_input_event<B: InputBackend>(state: &mut State, event: InputEvent<B>) {
    match event {
        InputEvent::Keyboard { event } => on_key::<B>(state, event),
        InputEvent::PointerMotionAbsolute { event } => {
            let Some(output) = state.output.clone() else {
                return;
            };
            let Some(geo) = state.space.output_geometry(&output) else {
                return;
            };
            let pos = event.position_transformed(geo.size) + geo.loc.to_f64();
            on_motion(state, pos, event.time_msec());
        }
        InputEvent::PointerMotion { event } => {
            let mut pos = state.pointer_location + event.delta();
            if let Some(geo) = state
                .output
                .as_ref()
                .and_then(|o| state.space.output_geometry(o))
            {
                pos.x = pos
                    .x
                    .clamp(geo.loc.x as f64, (geo.loc.x + geo.size.w - 1) as f64);
                pos.y = pos
                    .y
                    .clamp(geo.loc.y as f64, (geo.loc.y + geo.size.h - 1) as f64);
            }
            on_motion(state, pos, event.time_msec());
        }
        InputEvent::PointerButton { event } => on_button::<B>(state, event),
        InputEvent::PointerAxis { event } => on_axis::<B>(state, event),
        _ => {}
    }
}

fn on_key<B: InputBackend>(state: &mut State, event: B::KeyboardKeyEvent) {
    let serial = SERIAL_COUNTER.next_serial();
    let time = Event::time_msec(&event);
    let key_state = event.state();
    let keycode = event.key_code();
    let keyboard = state.seat.get_keyboard().expect("keyboard");
    let action = keyboard.input::<KeyAction, _>(
        state,
        keycode,
        key_state,
        serial,
        time,
        |state, mods, handle| {
            if !mods.alt && state.wm.is_cycling() {
                state.wm.end_cycle();
            }
            if key_state == KeyState::Released {
                if state.lone_super.take() == Some(keycode) && !state.locked {
                    state.lone_super_fire = true;
                }
                if let Some(pos) = state.suppressed_keys.iter().position(|k| *k == keycode) {
                    state.suppressed_keys.remove(pos);
                    return FilterResult::Intercept(KeyAction::Swallow);
                }
                return FilterResult::Forward;
            }
            let m = mods_of(mods);
            // Lone Super (press + release, nothing in between) toggles Start.
            let is_super = handle
                .raw_syms()
                .iter()
                .any(|s| matches!(s.raw(), keysyms::KEY_Super_L | keysyms::KEY_Super_R));
            state.lone_super = if is_super && !m.ctrl && !m.alt && !m.shift && !state.locked {
                Some(keycode)
            } else {
                None
            };
            // Locked: only VT switching is handled here; every other key goes
            // to the lock surface (no shortcut can bypass the locker).
            if state.locked {
                // A dead locker can be replaced with Super+L.
                let locker_alive = state.lock_surfaces.iter().any(|(_, s)| s.alive());
                if !locker_alive
                    && m.super_
                    && handle
                        .raw_syms()
                        .iter()
                        .any(|s| key_of(*s) == Some(Key::Char('l')))
                {
                    state.suppressed_keys.push(keycode);
                    return FilterResult::Intercept(KeyAction::Comp(CompAction::Lock));
                }
                let sym = handle.modified_sym().raw();
                if (keysyms::KEY_XF86Switch_VT_1..=keysyms::KEY_XF86Switch_VT_12).contains(&sym) {
                    state.suppressed_keys.push(keycode);
                    return FilterResult::Intercept(KeyAction::Vt(
                        (sym - keysyms::KEY_XF86Switch_VT_1 + 1) as i32,
                    ));
                }
                return FilterResult::Forward;
            }
            // Emergency exit: Ctrl+Alt+Backspace.
            if m.ctrl && m.alt && handle.modified_sym().raw() == keysyms::KEY_BackSpace {
                state.suppressed_keys.push(keycode);
                return FilterResult::Intercept(KeyAction::Quit);
            }
            // VT switching: xkb turns Ctrl+Alt+F<n> into XF86Switch_VT_<n>.
            let sym = handle.modified_sym().raw();
            if (keysyms::KEY_XF86Switch_VT_1..=keysyms::KEY_XF86Switch_VT_12).contains(&sym) {
                state.suppressed_keys.push(keycode);
                return FilterResult::Intercept(KeyAction::Vt(
                    (sym - keysyms::KEY_XF86Switch_VT_1 + 1) as i32,
                ));
            }
            for sym in handle.raw_syms() {
                if let Some(key) = key_of(sym) {
                    if let Some(action) = comp_binding(m, &key) {
                        state.suppressed_keys.push(keycode);
                        return FilterResult::Intercept(KeyAction::Comp(action));
                    }
                    if let Some(action) = state.wm.action_for(m, &key) {
                        state.suppressed_keys.push(keycode);
                        return FilterResult::Intercept(KeyAction::Wm(action));
                    }
                }
            }
            FilterResult::Forward
        },
    );
    if state.switcher.is_some() && !state.wm.is_cycling() {
        state.refresh_switcher();
    }
    if std::mem::take(&mut state.lone_super_fire) {
        info!("lone Super: toggle Start");
        state.spawn("glaze-shell --toggle-start");
    }
    match action {
        Some(KeyAction::Wm(action)) => state.apply_action(action),
        Some(KeyAction::Swallow) => {}
        Some(KeyAction::Vt(vt)) => crate::backend::change_vt(state, vt),
        Some(KeyAction::Comp(a)) => apply_comp_action(state, a),
        Some(KeyAction::Quit) => {
            info!("Ctrl+Alt+Backspace: exiting");
            state.request_exit();
        }
        None => {}
    }
}

fn apply_comp_action(state: &mut State, action: CompAction) {
    info!(?action, "key binding");
    let focused = state.wm.focused().map(str::to_owned);
    match action {
        CompAction::Terminal => {
            let term = std::env::var("TERMINAL").unwrap_or_else(|_| "foot".into());
            state.spawn(&term);
        }
        CompAction::Exit => state.request_exit_animated(),
        CompAction::Lock => state.start_locker(),
        CompAction::RestoreLastMinimized => {
            if let Some(id) = state.wm.last_minimized().map(str::to_owned) {
                state.activate_window(&id);
            }
        }
        _ => {
            let Some(id) = focused else {
                return;
            };
            let changed = match action {
                CompAction::Snap(zone) => state.wm.snap(&id, zone),
                CompAction::RestoreOrMinimize => state.wm.restore_or_minimize(&id),
                CompAction::Minimize => state.wm.minimize(&id),
                CompAction::ToggleFullscreen => {
                    let fs = state.wm.get(&id).is_some_and(|w| w.fullscreen);
                    state.wm.set_fullscreen(&id, !fs)
                }
                CompAction::ToggleFloating => state.wm.toggle_floating(&id),
                _ => false,
            };
            if changed {
                state.apply_layout(true);
            }
        }
    }
}

/// The cursor for a resize edge or corner.
pub fn resize_cursor(e: Edges) -> CursorIcon {
    match (e.top, e.bottom, e.left, e.right) {
        (true, _, true, _) | (_, true, _, true) => CursorIcon::NwseResize,
        (true, _, _, true) | (_, true, true, _) => CursorIcon::NeswResize,
        (true, _, _, _) | (_, true, _, _) => CursorIcon::NsResize,
        _ => CursorIcon::EwResize,
    }
}

/// Topmost thing under `pos`.
pub fn under(state: &State, pos: Point<f64, Logical>) -> Option<Under> {
    if state.locked {
        // Only the lock surface of the output under the pointer.
        for (output, lock) in &state.lock_surfaces {
            let Some(geo) = state.space.output_geometry(output) else {
                continue;
            };
            if geo.to_f64().contains(pos) && lock.alive() {
                return Some(Under::Surface(lock.wl_surface().clone(), geo.loc.to_f64()));
            }
        }
        return None;
    }
    let output = state.output.as_ref()?;
    let output_geo = state.space.output_geometry(output)?;
    let map = layer_map_for_output(output);
    let layer_hit = |layers: &[Layer]| -> Option<Under> {
        for layer in layers {
            if let Some(l) = map.layer_under(*layer, pos - output_geo.loc.to_f64()) {
                let geo = map.layer_geometry(l)?;
                let rel = pos - output_geo.loc.to_f64() - geo.loc.to_f64();
                if let Some((surface, loc)) = l.surface_under(rel, WindowSurfaceType::ALL) {
                    return Some(Under::Surface(
                        surface,
                        (loc + geo.loc + output_geo.loc).to_f64(),
                    ));
                }
            }
        }
        None
    };
    if let Some(hit) = layer_hit(&[Layer::Overlay, Layer::Top]) {
        return Some(hit);
    }
    // X11 menus and tooltips (override-redirect) sit above every window.
    for x in state
        .x11_override_windows()
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
    {
        let g = x.geometry();
        if g.to_f64().contains(pos) {
            if let Some(s) = x.wl_surface() {
                return Some(Under::Surface(s, g.loc.to_f64()));
            }
        }
    }
    for entry in state.windows.iter().rev() {
        let (Some(id), Some(rest)) = (entry.id.as_deref(), entry.rest) else {
            continue;
        };
        if !entry.shown || state.space.element_location(&entry.window).is_none() {
            continue;
        }
        let loc = state.space.element_location(&entry.window)?;
        // Client surfaces and popups first (popups may extend past the frame).
        if let Some((surface, sloc)) = entry
            .window
            .surface_under(pos - loc.to_f64(), WindowSurfaceType::ALL)
        {
            return Some(Under::Surface(surface, (sloc + loc).to_f64()));
        }
        if entry.ssd {
            let resizable = !state
                .wm
                .get(id)
                .is_some_and(|w| w.maximized || w.fullscreen);
            if let Some(part) = frame::hit_test(
                rest.w,
                rest.h,
                pos.x.floor() as i32 - rest.x,
                pos.y.floor() as i32 - rest.y,
                resizable,
            ) {
                if part != FramePart::Client {
                    return Some(Under::Frame(id.to_owned(), part));
                }
            }
        }
    }
    layer_hit(&[Layer::Bottom, Layer::Background])
}

fn on_motion(state: &mut State, pos: Point<f64, Logical>, time: u32) {
    state.pointer_location = pos;
    let serial = SERIAL_COUNTER.next_serial();
    let hit = under(state, pos);
    // Title-bar button hover.
    let hovered = match &hit {
        Some(Under::Frame(id, FramePart::Button(b))) => Some((id.clone(), *b)),
        _ => None,
    };
    let mut hover_changed = false;
    for entry in &mut state.windows {
        let h = match (&hovered, entry.id.as_deref()) {
            (Some((hid, b)), Some(id)) if hid == id => Some(*b),
            _ => None,
        };
        if entry.hovered != h {
            entry.hovered = h;
            if h.is_none() {
                entry.pressed = None;
            }
            hover_changed = true;
        }
    }
    if hover_changed {
        state.refresh_titles();
    }
    let cursor = match &hit {
        Some(Under::Frame(_, FramePart::Resize(e))) => Some(resize_cursor(*e)),
        Some(Under::Frame(..)) | None => Some(CursorIcon::Default),
        Some(Under::Surface(..)) => None,
    };
    if state.frame_cursor != cursor {
        state.frame_cursor = cursor;
    }
    if state.software_cursor {
        state.needs_redraw = true;
    }
    let focus = match hit {
        Some(Under::Surface(s, loc)) => Some((s, loc)),
        _ => None,
    };
    let pointer = state.pointer.clone();
    pointer.motion(
        state,
        focus,
        &MotionEvent {
            location: pos,
            serial,
            time,
        },
    );
    pointer.frame(state);
    if matches!(state.cursor_status, CursorImageStatus::Surface(_)) {
        state.needs_redraw = true;
    }
}

fn on_button<B: InputBackend>(state: &mut State, event: B::PointerButtonEvent) {
    let serial = SERIAL_COUNTER.next_serial();
    let button = event.button_code();
    let button_state = event.state();
    let time = Event::time_msec(&event);
    state.lone_super = None;
    let pointer = state.pointer.clone();

    // A surface may have appeared under a pointer that has not moved since
    // (a fresh session's taskbar): refresh pointer focus first, or the first
    // click goes nowhere.
    if button_state == ButtonState::Pressed && !pointer.is_grabbed() {
        let pos = state.pointer_location;
        let focus_now = match under(state, pos) {
            Some(Under::Surface(s, _)) => Some(s),
            _ => None,
        };
        if pointer.current_focus() != focus_now {
            on_motion(state, pos, time);
        }
    }
    let super_held = state
        .seat
        .get_keyboard()
        .is_some_and(|k| k.modifier_state().logo);
    if state.locked {
        pointer.button(
            state,
            &ButtonEvent {
                button,
                state: button_state,
                serial,
                time,
            },
        );
        pointer.frame(state);
        return;
    }
    if button_state == ButtonState::Pressed && !pointer.is_grabbed() && super_held {
        if let Some(id) = window_at(state, state.pointer_location) {
            if button == BTN_LEFT || button == BTN_RIGHT {
                state.activate_window(&id);
                pointer.button(
                    state,
                    &ButtonEvent {
                        button,
                        state: button_state,
                        serial,
                        time,
                    },
                );
                let seat = state.seat.clone();
                if button == BTN_LEFT {
                    start_move(state, &seat, &id, serial);
                } else if let Some(rest) =
                    state.entry_for_id(&id).and_then(|i| state.windows[i].rest)
                {
                    // Resize from the nearest corner (Sway's floating_modifier).
                    let p = state.pointer_location;
                    let left = p.x < f64::from(rest.x) + f64::from(rest.w) / 2.0;
                    let top = p.y < f64::from(rest.y) + f64::from(rest.h) / 2.0;
                    let edges = Edges {
                        top,
                        bottom: !top,
                        left,
                        right: !left,
                    };
                    start_resize(state, &seat, &id, edges, serial);
                }
                pointer.frame(state);
                return;
            }
        }
    }

    if button_state == ButtonState::Pressed && !pointer.is_grabbed() {
        match under(state, state.pointer_location) {
            Some(Under::Frame(id, part)) => {
                state.activate_window(&id);
                if button == BTN_LEFT {
                    match part {
                        FramePart::Title => {
                            let double =
                                state.last_title_click.as_ref().is_some_and(|(last, at)| {
                                    *last == id && at.elapsed() < DOUBLE_CLICK
                                });
                            if double {
                                state.last_title_click = None;
                                let maximized = state.wm.get(&id).is_some_and(|w| w.maximized);
                                state.set_maximized(&id, !maximized);
                            } else {
                                state.last_title_click = Some((id.clone(), Instant::now()));
                                pointer.button(
                                    state,
                                    &ButtonEvent {
                                        button,
                                        state: button_state,
                                        serial,
                                        time,
                                    },
                                );
                                let seat = state.seat.clone();
                                start_move(state, &seat, &id, serial);
                                pointer.frame(state);
                                return;
                            }
                        }
                        FramePart::Button(b) => {
                            if let Some(i) = state.entry_for_id(&id) {
                                state.windows[i].pressed = Some(b);
                                state.refresh_titles();
                            }
                        }
                        FramePart::Resize(edges) => {
                            pointer.button(
                                state,
                                &ButtonEvent {
                                    button,
                                    state: button_state,
                                    serial,
                                    time,
                                },
                            );
                            let seat = state.seat.clone();
                            start_resize(state, &seat, &id, edges, serial);
                            pointer.frame(state);
                            return;
                        }
                        FramePart::Client => {}
                    }
                }
            }
            Some(Under::Surface(surface, _)) => {
                let root = root_surface(&surface);
                if let Some(id) = state
                    .entry_for_surface(&root)
                    .and_then(|i| state.windows[i].id.clone())
                {
                    if state.wm.focused() != Some(id.as_str()) {
                        state.activate_window(&id);
                    }
                } else {
                    focus_layer_if_interactive(state, &root, serial);
                }
            }
            None => {}
        }
    }

    if button_state == ButtonState::Released && button == BTN_LEFT {
        // Title-bar button release: act if still over the pressed button.
        let mut act = None;
        for entry in &mut state.windows {
            if let (Some(p), Some(id)) = (entry.pressed.take(), entry.id.clone()) {
                if entry.hovered == Some(p) {
                    act = Some((id, p));
                }
            }
        }
        if let Some((id, b)) = act {
            state.refresh_titles();
            match b {
                Button::Minimize => {
                    state.minimize_window(&id);
                }
                Button::Maximize => {
                    let maximized = state.wm.get(&id).is_some_and(|w| w.maximized);
                    state.set_maximized(&id, !maximized);
                }
                Button::Close => {
                    state.close_window(&id);
                }
            }
        } else {
            state.refresh_titles();
        }
    }

    pointer.button(
        state,
        &ButtonEvent {
            button,
            state: button_state,
            serial,
            time,
        },
    );
    pointer.frame(state);
}

fn on_axis<B: InputBackend>(state: &mut State, event: B::PointerAxisEvent) {
    let source = event.source();
    let mut frame = AxisFrame::new(Event::time_msec(&event)).source(source);
    for axis in [Axis::Horizontal, Axis::Vertical] {
        let amount = event
            .amount(axis)
            .or_else(|| event.amount_v120(axis).map(|v| v * 15.0 / 120.0))
            .unwrap_or(0.0);
        if amount != 0.0 {
            frame = frame.value(axis, amount);
            if let Some(v120) = event.amount_v120(axis) {
                frame = frame.v120(axis, v120 as i32);
            }
        }
        if source == AxisSource::Finger && event.amount(axis) == Some(0.0) {
            frame = frame.stop(axis);
        }
    }
    let pointer = state.pointer.clone();
    pointer.axis(state, frame);
    pointer.frame(state);
}

/// The managed window under `pos` (surface or frame), if any.
fn window_at(state: &State, pos: Point<f64, Logical>) -> Option<String> {
    match under(state, pos)? {
        Under::Frame(id, _) => Some(id),
        Under::Surface(surface, _) => {
            let root = root_surface(&surface);
            state
                .entry_for_surface(&root)
                .and_then(|i| state.windows[i].id.clone())
        }
    }
}

fn root_surface(surface: &WlSurface) -> WlSurface {
    let mut root = surface.clone();
    while let Some(parent) = smithay::wayland::compositor::get_parent(&root) {
        root = parent;
    }
    root
}

fn focus_layer_if_interactive(state: &mut State, surface: &WlSurface, serial: Serial) {
    let Some(output) = state.output.clone() else {
        return;
    };
    let interactive = {
        let map = layer_map_for_output(&output);
        map.layer_for_surface(surface, WindowSurfaceType::TOPLEVEL)
            .is_some_and(|l| l.cached_state().keyboard_interactivity != KeyboardInteractivity::None)
    };
    if interactive {
        let keyboard = state.seat.get_keyboard().expect("keyboard");
        keyboard.set_focus(state, Some(surface.clone()), serial);
    }
}

// --- grabs -------------------------------------------------------------------

/// Start an interactive move of window `id` (title bar drag or client
/// `xdg_toplevel.move`).
pub fn start_move(state: &mut State, seat: &Seat<State>, id: &str, serial: Serial) {
    let Some(pointer) = seat.get_pointer() else {
        return;
    };
    let start_data = pointer.grab_start_data().unwrap_or(GrabStartData {
        focus: None,
        button: BTN_LEFT,
        location: pointer.current_location(),
    });
    let Some(i) = state.entry_for_id(id) else {
        return;
    };
    let Some(rest) = state.windows[i].rest else {
        return;
    };
    let Some(managed) = state.wm.get(id).cloned() else {
        return;
    };
    // Dragging a maximised or tiled window restores its stacking size under
    // the pointer, keeping the grab point proportionally.
    let p = start_data.location;
    let restore_size = managed.restore.filter(|_| !managed.maximized);
    let start = if managed.maximized || state.wm.is_tiled(id) || restore_size.is_some() {
        let rel = ((p.x - rest.x as f64) / rest.w.max(1) as f64).clamp(0.0, 1.0);
        let s = restore_size.unwrap_or(managed.stacking);
        Rect::new(
            (p.x - s.w as f64 * rel) as i32,
            (p.y - (p.y - rest.y as f64).min(frame::TITLE_H as f64 / 2.0)) as i32,
            s.w,
            s.h,
        )
    } else {
        rest
    };
    let grab = MoveGrab {
        start_data,
        id: id.to_owned(),
        start,
    };
    pointer.set_grab(state, grab, serial, Focus::Clear);
}

/// Snap zone for a drag drop: halves left/right, quarters in the corners,
/// top edge = maximise. The bottom edge (the taskbar) does not snap.
pub fn drag_zone(p: (i32, i32), work_area: Rect) -> Option<glaze_wm::SnapZone> {
    use glaze_wm::SnapZone;
    match detect_zone(p, work_area)? {
        SnapZone::TopHalf => Some(SnapZone::Maximize),
        SnapZone::BottomHalf => None,
        z => Some(z),
    }
}

fn update_snap_preview(state: &mut State, id: &str, window_rect: Rect) {
    let p = state.pointer_location;
    let zone = drag_zone((p.x as i32, p.y as i32), state.wm.work_area());
    let current = state.snap_preview.as_ref().map(|s| s.zone);
    if zone == current {
        return;
    }
    state.needs_redraw = true;
    let Some(zone) = zone else {
        state.snap_preview = None;
        return;
    };
    let area = state.wm.work_area();
    let target = if zone == glaze_wm::SnapZone::Maximize {
        area
    } else {
        glaze_wm::geometry::snap_zone(zone, area, crate::wm::SNAP_GAP)
    };
    let now = state.now();
    // Grow from the previous preview, or out of the dragged window.
    let from = match &state.snap_preview {
        Some(prev) => prev.sample(now).0,
        None => crate::anim::RectF::from(window_rect),
    };
    state.snap_preview = Some(crate::state::SnapPreview {
        window: id.to_owned(),
        zone,
        target,
        from,
        start: now,
        duration: state.motion.duration(160),
    });
}

/// Start an interactive resize.
pub fn start_resize(state: &mut State, seat: &Seat<State>, id: &str, edges: Edges, serial: Serial) {
    let Some(pointer) = seat.get_pointer() else {
        return;
    };
    let start_data = pointer.grab_start_data().unwrap_or(GrabStartData {
        focus: None,
        button: BTN_LEFT,
        location: pointer.current_location(),
    });
    let Some(rest) = state.entry_for_id(id).and_then(|i| state.windows[i].rest) else {
        return;
    };
    let grab = ResizeGrab {
        start_data,
        id: id.to_owned(),
        edges,
        start: rest,
    };
    pointer.set_grab(state, grab, serial, Focus::Clear);
}

pub struct MoveGrab {
    start_data: GrabStartData<State>,
    id: String,
    start: Rect,
}

pub struct ResizeGrab {
    start_data: GrabStartData<State>,
    id: String,
    edges: Edges,
    start: Rect,
}

const MIN_OUTER_W: i32 = 160;
const MIN_OUTER_H: i32 = 80;

macro_rules! passthrough_gestures {
    () => {
        fn relative_motion(
            &mut self,
            data: &mut State,
            handle: &mut PointerInnerHandle<'_, State>,
            _focus: Option<(WlSurface, Point<f64, Logical>)>,
            event: &RelativeMotionEvent,
        ) {
            handle.relative_motion(data, None, event);
        }
        fn axis(
            &mut self,
            data: &mut State,
            handle: &mut PointerInnerHandle<'_, State>,
            details: AxisFrame,
        ) {
            handle.axis(data, details)
        }
        fn frame(&mut self, data: &mut State, handle: &mut PointerInnerHandle<'_, State>) {
            handle.frame(data);
        }
        fn gesture_swipe_begin(
            &mut self,
            data: &mut State,
            handle: &mut PointerInnerHandle<'_, State>,
            event: &GestureSwipeBeginEvent,
        ) {
            handle.gesture_swipe_begin(data, event)
        }
        fn gesture_swipe_update(
            &mut self,
            data: &mut State,
            handle: &mut PointerInnerHandle<'_, State>,
            event: &GestureSwipeUpdateEvent,
        ) {
            handle.gesture_swipe_update(data, event)
        }
        fn gesture_swipe_end(
            &mut self,
            data: &mut State,
            handle: &mut PointerInnerHandle<'_, State>,
            event: &GestureSwipeEndEvent,
        ) {
            handle.gesture_swipe_end(data, event)
        }
        fn gesture_pinch_begin(
            &mut self,
            data: &mut State,
            handle: &mut PointerInnerHandle<'_, State>,
            event: &GesturePinchBeginEvent,
        ) {
            handle.gesture_pinch_begin(data, event)
        }
        fn gesture_pinch_update(
            &mut self,
            data: &mut State,
            handle: &mut PointerInnerHandle<'_, State>,
            event: &GesturePinchUpdateEvent,
        ) {
            handle.gesture_pinch_update(data, event)
        }
        fn gesture_pinch_end(
            &mut self,
            data: &mut State,
            handle: &mut PointerInnerHandle<'_, State>,
            event: &GesturePinchEndEvent,
        ) {
            handle.gesture_pinch_end(data, event)
        }
        fn gesture_hold_begin(
            &mut self,
            data: &mut State,
            handle: &mut PointerInnerHandle<'_, State>,
            event: &GestureHoldBeginEvent,
        ) {
            handle.gesture_hold_begin(data, event)
        }
        fn gesture_hold_end(
            &mut self,
            data: &mut State,
            handle: &mut PointerInnerHandle<'_, State>,
            event: &GestureHoldEndEvent,
        ) {
            handle.gesture_hold_end(data, event)
        }
        fn start_data(&self) -> &GrabStartData<State> {
            &self.start_data
        }
    };
}

impl PointerGrab<State> for MoveGrab {
    fn motion(
        &mut self,
        data: &mut State,
        handle: &mut PointerInnerHandle<'_, State>,
        _focus: Option<(WlSurface, Point<f64, Logical>)>,
        event: &MotionEvent,
    ) {
        data.pointer_location = event.location;
        data.frame_cursor = Some(CursorIcon::Grabbing);
        handle.motion(data, None, event);
        let delta = event.location - self.start_data.location;
        let rect = Rect::new(
            self.start.x + delta.x.round() as i32,
            self.start.y + delta.y.round() as i32,
            self.start.w,
            self.start.h,
        );
        data.wm.set_stacking_geometry(&self.id, rect);
        data.apply_layout(false);
        update_snap_preview(data, &self.id, rect);
    }

    fn button(
        &mut self,
        data: &mut State,
        handle: &mut PointerInnerHandle<'_, State>,
        event: &ButtonEvent,
    ) {
        handle.button(data, event);
        if handle.current_pressed().is_empty() {
            // Drop onto an edge or corner: snap there (Win7-style Aero snap).
            let p = data.pointer_location;
            data.snap_preview = None;
            if let Some(zone) = drag_zone((p.x as i32, p.y as i32), data.wm.work_area()) {
                info!(id = %self.id, ?zone, "drag snap");
                data.wm.snap(&self.id, zone);
                data.apply_layout(true);
            }
            data.frame_cursor = None;
            data.needs_redraw = true;
            handle.unset_grab(self, data, event.serial, event.time, true);
        }
    }

    fn unset(&mut self, _data: &mut State) {}

    passthrough_gestures!();
}

impl PointerGrab<State> for ResizeGrab {
    fn motion(
        &mut self,
        data: &mut State,
        handle: &mut PointerInnerHandle<'_, State>,
        _focus: Option<(WlSurface, Point<f64, Logical>)>,
        event: &MotionEvent,
    ) {
        data.pointer_location = event.location;
        data.frame_cursor = Some(resize_cursor(self.edges));
        data.resizing = Some((self.id.clone(), self.edges));
        handle.motion(data, None, event);
        let d = event.location - self.start_data.location;
        let (dx, dy) = (d.x.round() as i32, d.y.round() as i32);
        let s = self.start;
        let (mut x, mut y, mut w, mut h) = (s.x, s.y, s.w, s.h);
        if self.edges.left {
            let nw = (s.w - dx).max(MIN_OUTER_W);
            x = s.x + s.w - nw;
            w = nw;
        }
        if self.edges.right {
            w = (s.w + dx).max(MIN_OUTER_W);
        }
        if self.edges.top {
            let nh = (s.h - dy).max(MIN_OUTER_H);
            y = s.y + s.h - nh;
            h = nh;
        }
        if self.edges.bottom {
            h = (s.h + dy).max(MIN_OUTER_H);
        }
        data.wm
            .set_stacking_geometry(&self.id, Rect::new(x, y, w, h));
        data.apply_layout(false);
    }

    fn button(
        &mut self,
        data: &mut State,
        handle: &mut PointerInnerHandle<'_, State>,
        event: &ButtonEvent,
    ) {
        handle.button(data, event);
        if handle.current_pressed().is_empty() {
            data.frame_cursor = None;
            data.resizing = None;
            data.needs_redraw = true;
            handle.unset_grab(self, data, event.serial, event.time, true);
        }
    }

    fn unset(&mut self, _data: &mut State) {}

    passthrough_gestures!();
}
