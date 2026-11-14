//! Panel keybindings: maps GTK key events to `glaze-wm` actions.
//!
//! The keymap is data (`glaze_wm::keybindings::Keymap::default_glaze`), so the
//! shell, a future settings editor and the compositor all read the same table.
//!
//! **Scope, stated honestly:** a controller on the panel window only sees keys
//! while the panel (or its launcher popover) holds keyboard focus. System-wide
//! shortcuts must be bound by the compositor, which owns input and knows about
//! foreign windows. When a bound action needs that, this module says so instead
//! of pretending to have moved someone else's window.

use std::rc::Rc;

use glaze_wm::keybindings::{Key, Keymap, Mods, WmAction};
use gtk4::gdk;
use gtk4::prelude::*;

/// Translate a GDK modifier state into a `glaze-wm` [`Mods`].
pub fn mods_of(state: gdk::ModifierType) -> Mods {
    Mods {
        ctrl: state.contains(gdk::ModifierType::CONTROL_MASK),
        alt: state.contains(gdk::ModifierType::ALT_MASK),
        shift: state.contains(gdk::ModifierType::SHIFT_MASK),
        super_: state.contains(gdk::ModifierType::SUPER_MASK),
    }
}

/// Translate a GDK key value into a `glaze-wm` [`Key`], when we might bind it.
pub fn key_of(keyval: gdk::Key) -> Option<Key> {
    if let Some(ch) = keyval.to_unicode() {
        if ch.is_ascii_graphic() {
            return Some(Key::Char(ch.to_ascii_lowercase()));
        }
    }
    Some(match keyval {
        gdk::Key::Tab => Key::Tab,
        gdk::Key::Escape => Key::Escape,
        gdk::Key::Return | gdk::Key::KP_Enter => Key::Enter,
        gdk::Key::space => Key::Space,
        gdk::Key::Left => Key::Left,
        gdk::Key::Right => Key::Right,
        gdk::Key::Up => Key::Up,
        gdk::Key::Down => Key::Down,
        gdk::Key::Home => Key::Home,
        gdk::Key::End => Key::End,
        gdk::Key::Page_Up => Key::PageUp,
        gdk::Key::Page_Down => Key::PageDown,
        gdk::Key::Delete => Key::Delete,
        gdk::Key::BackSpace => Key::Backspace,
        _ => return None,
    })
}

/// Who is responsible for carrying out an action.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Handling {
    /// The shell can do it itself (opening the launcher, so far).
    Shell,
    /// Requires the compositor, which owns input and foreign windows.
    Compositor,
}

/// Whether the shell or the compositor must carry out `action`.
pub fn handling(action: WmAction) -> Handling {
    match action {
        WmAction::Launcher => Handling::Shell,
        _ => Handling::Compositor,
    }
}

/// Install the Glaze keymap on `widget`. Returns the number of bindings loaded.
///
/// `on_launcher` runs when the launcher binding fires.
pub fn install<F>(widget: &impl IsA<gtk4::Widget>, keymap: Rc<Keymap>, on_launcher: F) -> usize
where
    F: Fn() + 'static,
{
    let controller = gtk4::EventControllerKey::new();
    let count = keymap.len();
    let on_launcher = Rc::new(on_launcher);
    controller.connect_key_pressed(move |_controller, keyval, _keycode, state| {
        let mods = mods_of(state);
        let Some(key) = key_of(keyval) else {
            return glib::Propagation::Proceed;
        };
        match keymap.action_for(mods, &key).copied() {
            Some(WmAction::Launcher) => {
                (*on_launcher)();
                glib::Propagation::Stop
            }
            Some(action) => {
                if handling(action) == Handling::Compositor {
                    eprintln!(
                        "[glaze-shell] keybinding {action:?}: the compositor owns this action; \
                         bind it there for a system-wide shortcut"
                    );
                }
                glib::Propagation::Stop
            }
            None => glib::Propagation::Proceed,
        }
    });
    widget.add_controller(controller);
    count
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mods_map_from_gdk_masks() {
        let state = gdk::ModifierType::SUPER_MASK | gdk::ModifierType::SHIFT_MASK;
        let mods = mods_of(state);
        assert!(mods.super_ && mods.shift && !mods.ctrl && !mods.alt);
        assert_eq!(mods_of(gdk::ModifierType::CONTROL_MASK), Mods::CTRL);
        assert_eq!(mods_of(gdk::ModifierType::empty()), Mods::NONE);
    }

    #[test]
    fn keys_map_from_gdk_values() {
        assert_eq!(key_of(gdk::Key::space), Some(Key::Space));
        assert_eq!(key_of(gdk::Key::Escape), Some(Key::Escape));
        assert_eq!(key_of(gdk::Key::KP_Enter), Some(Key::Enter));
        assert_eq!(key_of(gdk::Key::Page_Up), Some(Key::PageUp));
        // Printable characters are lower-cased to match the keymap syntax.
        assert_eq!(key_of(gdk::Key::q), Some(Key::Char('q')));
    }

    #[test]
    fn only_the_launcher_is_a_shell_action() {
        assert_eq!(handling(WmAction::Launcher), Handling::Shell);
        assert_eq!(handling(WmAction::CloseWindow), Handling::Compositor);
        assert_eq!(handling(WmAction::NextWorkspace), Handling::Compositor);
    }
}
