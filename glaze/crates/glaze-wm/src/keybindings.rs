//! Keybinding syntax, matching and the default Glaze keymap.
//!
//! Bindings are written as `mods+key`, e.g. `"super+shift+q"`, `"super+1"`,
//! `"alt+tab"`. Parsing is case-insensitive and deterministic; matching is
//! **exact**: a binding fires only when the pressed modifier set equals the
//! binding's modifier set, so `super+q` never fires on `super+shift+q`. That
//! matters for a compositor, where a too-eager match steals input from
//! applications.
//!
//! The default keymap encodes the Glaze P1/P2 shortcuts (`GLAZE.md` §5, §6.3) as
//! data, so the shell, the settings editor and the compositor all read the same
//! table.

use std::str::FromStr;

use serde::{Deserialize, Serialize};

use crate::geometry::SnapZone;

/// A set of held modifier keys.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default, Serialize, Deserialize,
)]
pub struct Mods {
    pub ctrl: bool,
    pub alt: bool,
    pub shift: bool,
    pub super_: bool,
}

impl Mods {
    /// No modifiers held.
    pub const NONE: Mods = Mods {
        ctrl: false,
        alt: false,
        shift: false,
        super_: false,
    };
    /// The Control modifier.
    pub const CTRL: Mods = Mods {
        ctrl: true,
        ..Mods::NONE
    };
    /// The Alt modifier.
    pub const ALT: Mods = Mods {
        alt: true,
        ..Mods::NONE
    };
    /// The Shift modifier.
    pub const SHIFT: Mods = Mods {
        shift: true,
        ..Mods::NONE
    };
    /// The Super/Logo modifier.
    pub const SUPER: Mods = Mods {
        super_: true,
        ..Mods::NONE
    };

    /// True when no modifier is held.
    pub const fn is_empty(self) -> bool {
        !self.ctrl && !self.alt && !self.shift && !self.super_
    }

    /// True when every modifier set in `other` is also set in `self`.
    pub const fn contains(self, other: Mods) -> bool {
        (!other.ctrl || self.ctrl)
            && (!other.alt || self.alt)
            && (!other.shift || self.shift)
            && (!other.super_ || self.super_)
    }

    /// The union of two modifier sets.
    pub const fn union(self, other: Mods) -> Mods {
        Mods {
            ctrl: self.ctrl || other.ctrl,
            alt: self.alt || other.alt,
            shift: self.shift || other.shift,
            super_: self.super_ || other.super_,
        }
    }
}

impl std::ops::BitOr for Mods {
    type Output = Mods;
    fn bitor(self, rhs: Mods) -> Mods {
        self.union(rhs)
    }
}

impl std::ops::BitOrAssign for Mods {
    fn bitor_assign(&mut self, rhs: Mods) {
        *self = self.union(rhs);
    }
}

/// A non-modifier key.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Key {
    /// A printable character, always stored lower-cased (`'q'`, `'1'`).
    Char(char),
    Tab,
    Escape,
    Enter,
    Space,
    Left,
    Right,
    Up,
    Down,
    Home,
    End,
    PageUp,
    PageDown,
    Delete,
    Backspace,
    /// A function key: `F1` is `Function(1)`, `F12` is `Function(12)`.
    Function(u8),
}

fn modifier_of(token: &str) -> Option<Mods> {
    match token {
        "super" | "mod" | "mod4" | "logo" | "win" | "meta" | "cmd" => Some(Mods::SUPER),
        "ctrl" | "control" => Some(Mods::CTRL),
        "alt" | "mod1" => Some(Mods::ALT),
        "shift" => Some(Mods::SHIFT),
        _ => None,
    }
}

fn parse_key(token: &str) -> Result<Key, BindError> {
    let key = match token {
        "tab" => Key::Tab,
        "escape" | "esc" => Key::Escape,
        "return" | "enter" => Key::Enter,
        "space" => Key::Space,
        "left" => Key::Left,
        "right" => Key::Right,
        "up" => Key::Up,
        "down" => Key::Down,
        "home" => Key::Home,
        "end" => Key::End,
        "pageup" | "pgup" => Key::PageUp,
        "pagedown" | "pgdn" => Key::PageDown,
        "delete" | "del" => Key::Delete,
        "backspace" => Key::Backspace,
        _ => {
            if let Some(digits) = token.strip_prefix('f') {
                if let Ok(n) = digits.parse::<u8>() {
                    if (1..=12).contains(&n) {
                        return Ok(Key::Function(n));
                    }
                }
            }
            let mut chars = token.chars();
            match (chars.next(), chars.next()) {
                (Some(c), None) if c.is_ascii_graphic() => Key::Char(c.to_ascii_lowercase()),
                _ => return Err(BindError::UnknownKey(token.to_owned())),
            }
        }
    };
    Ok(key)
}

/// A modifier set plus a key.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct Binding {
    pub mods: Mods,
    pub key: Key,
}

impl Binding {
    /// Build a binding directly.
    pub const fn new(mods: Mods, key: Key) -> Self {
        Self { mods, key }
    }

    /// Parse `"mods+key"` syntax (case-insensitive).
    pub fn parse(s: &str) -> Result<Binding, BindError> {
        let trimmed = s.trim();
        if trimmed.is_empty() {
            return Err(BindError::Empty);
        }

        let tokens: Vec<String> = trimmed
            .split('+')
            .map(|t| t.trim().to_ascii_lowercase())
            .collect();
        let (last, modifiers) = tokens
            .split_last()
            .expect("split always yields at least one token");

        if last.is_empty() || modifier_of(last).is_some() {
            // `"super+"` or a modifier-only string such as `"ctrl"`.
            return Err(BindError::MissingKey);
        }

        let mut mods = Mods::NONE;
        for token in modifiers {
            if token.is_empty() {
                return Err(BindError::MissingKey);
            }
            let modifier =
                modifier_of(token).ok_or_else(|| BindError::UnknownModifier(token.clone()))?;
            if mods.contains(modifier) {
                return Err(BindError::DuplicateModifier(token.clone()));
            }
            mods |= modifier;
        }

        let key = parse_key(last)?;
        Ok(Binding { mods, key })
    }

    /// Exact-match test: the held modifiers must equal this binding's modifiers.
    pub fn matches(&self, mods: Mods, key: &Key) -> bool {
        self.mods == mods && self.key == *key
    }
}

impl FromStr for Binding {
    type Err = BindError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Binding::parse(s)
    }
}

/// A window-management action a binding can trigger.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum WmAction {
    CloseWindow,
    ToggleMaximize,
    MoveLeft,
    MoveRight,
    SwitchWorkspace(u32),
    NextWorkspace,
    PrevWorkspace,
    Snap(SnapZone),
    /// Flip the active workspace between stacking and tiling.
    ToggleTiling,
    Launcher,
    /// Flip every workspace between stacking and tiling (clears overrides).
    ToggleTilingGlobal,
    /// Minimise the focused window (restore it from the taskbar or Alt+Tab).
    Minimize,
    /// Alt+Tab: focus the next window in most-recently-used order.
    CycleWindows,
    /// Alt+Shift+Tab: the reverse of [`WmAction::CycleWindows`].
    CycleWindowsBack,
    /// Move the focused window to a workspace (zero-based).
    MoveToWorkspace(u32),
}

/// Errors produced while parsing a binding.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum BindError {
    /// The input was empty.
    Empty,
    /// No non-modifier key was present (e.g. `"ctrl"`, `"super+"`).
    MissingKey,
    /// A non-final token was not a known modifier.
    UnknownModifier(String),
    /// The final token was not a known key.
    UnknownKey(String),
    /// The same modifier appeared twice (e.g. `"super+super+q"`).
    DuplicateModifier(String),
}

impl std::fmt::Display for BindError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BindError::Empty => write!(f, "empty keybinding"),
            BindError::MissingKey => write!(f, "keybinding has no key"),
            BindError::UnknownModifier(m) => write!(f, "unknown modifier: {m}"),
            BindError::UnknownKey(k) => write!(f, "unknown key: {k}"),
            BindError::DuplicateModifier(m) => write!(f, "duplicate modifier: {m}"),
        }
    }
}

impl std::error::Error for BindError {}

/// An ordered mapping from bindings to actions.
///
/// The order is insertion order and is stable, which lets the settings editor
/// list shortcuts deterministically. `bind` replaces an existing binding and
/// returns the action it displaced, so a conflict is never silent.
#[derive(Debug, Clone, Default)]
pub struct Keymap {
    bindings: Vec<(Binding, WmAction)>,
}

impl Keymap {
    /// An empty keymap.
    pub fn new() -> Self {
        Self::default()
    }

    /// The Glaze default keymap (`GLAZE.md` §5 P1/P2, §6.3).
    pub fn default_glaze() -> Self {
        fn bind(keymap: &mut Keymap, spec: &str, action: WmAction) {
            let binding = Binding::parse(spec).expect("built-in Glaze binding must parse");
            keymap.bind(binding, action);
        }

        let mut keymap = Keymap::new();
        bind(&mut keymap, "super+shift+q", WmAction::CloseWindow);
        bind(&mut keymap, "super+up", WmAction::ToggleMaximize);
        bind(
            &mut keymap,
            "super+down",
            WmAction::Snap(SnapZone::BottomHalf),
        );
        bind(
            &mut keymap,
            "super+left",
            WmAction::Snap(SnapZone::LeftHalf),
        );
        bind(
            &mut keymap,
            "super+right",
            WmAction::Snap(SnapZone::RightHalf),
        );
        bind(&mut keymap, "super+shift+left", WmAction::MoveLeft);
        bind(&mut keymap, "super+shift+right", WmAction::MoveRight);
        bind(&mut keymap, "super+t", WmAction::ToggleTiling);
        bind(&mut keymap, "super+shift+t", WmAction::ToggleTilingGlobal);
        bind(&mut keymap, "super+h", WmAction::Minimize);
        bind(&mut keymap, "alt+tab", WmAction::CycleWindows);
        bind(&mut keymap, "alt+shift+tab", WmAction::CycleWindowsBack);
        bind(&mut keymap, "super+space", WmAction::Launcher);
        bind(&mut keymap, "super+tab", WmAction::NextWorkspace);
        bind(&mut keymap, "super+shift+tab", WmAction::PrevWorkspace);
        for index in 1..=9u32 {
            bind(
                &mut keymap,
                &format!("super+{index}"),
                WmAction::SwitchWorkspace(index - 1),
            );
            bind(
                &mut keymap,
                &format!("super+shift+{index}"),
                WmAction::MoveToWorkspace(index - 1),
            );
        }

        keymap
    }

    /// Add or replace a binding. Returns the action that was displaced, if any.
    pub fn bind(&mut self, binding: Binding, action: WmAction) -> Option<WmAction> {
        if let Some(slot) = self.bindings.iter_mut().find(|(b, _)| *b == binding) {
            return Some(std::mem::replace(&mut slot.1, action));
        }
        self.bindings.push((binding, action));
        None
    }

    /// The action bound to an exact modifier/key combination.
    pub fn action_for(&self, mods: Mods, key: &Key) -> Option<&WmAction> {
        self.bindings
            .iter()
            .find(|(binding, _)| binding.matches(mods, key))
            .map(|(_, action)| action)
    }

    /// True when `binding` is present.
    pub fn contains(&self, binding: &Binding) -> bool {
        self.bindings.iter().any(|(b, _)| b == binding)
    }

    /// Number of bindings.
    pub fn len(&self) -> usize {
        self.bindings.len()
    }

    /// True when there are no bindings.
    pub fn is_empty(&self) -> bool {
        self.bindings.is_empty()
    }

    /// Bindings in insertion order.
    pub fn iter(&self) -> impl Iterator<Item = &(Binding, WmAction)> {
        self.bindings.iter()
    }

    /// Every binding that occurs more than once. The default keymap has none.
    pub fn duplicate_bindings(&self) -> Vec<Binding> {
        let mut duplicates = Vec::new();
        for (i, (binding, _)) in self.bindings.iter().enumerate() {
            let seen_before = self.bindings[..i].iter().any(|(b, _)| b == binding);
            let seen_after = self.bindings[i + 1..].iter().any(|(b, _)| b == binding);
            if (seen_before || seen_after) && !duplicates.contains(binding) {
                duplicates.push(*binding);
            }
        }
        duplicates
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_reads_modifiers_and_key() {
        let binding = Binding::parse("super+shift+q").unwrap();
        assert_eq!(binding.mods, Mods::SUPER | Mods::SHIFT);
        assert_eq!(binding.key, Key::Char('q'));

        assert_eq!(
            Binding::parse("alt+tab").unwrap(),
            Binding::new(Mods::ALT, Key::Tab)
        );
        assert_eq!(
            Binding::parse("super+1").unwrap(),
            Binding::new(Mods::SUPER, Key::Char('1'))
        );
        assert_eq!(
            Binding::parse("ctrl+alt+t").unwrap(),
            Binding::new(Mods::CTRL | Mods::ALT, Key::Char('t'))
        );
        assert_eq!(
            Binding::parse("super+f5").unwrap(),
            Binding::new(Mods::SUPER, Key::Function(5))
        );
        assert_eq!(
            Binding::parse("q").unwrap(),
            Binding::new(Mods::NONE, Key::Char('q'))
        );
    }

    #[test]
    fn parse_is_case_insensitive_and_trims_whitespace() {
        assert_eq!(
            Binding::parse("SUPER+Shift+Q").unwrap(),
            Binding::parse("super+shift+q").unwrap()
        );
        assert_eq!(
            Binding::parse("  Alt + Tab  ").unwrap(),
            Binding::parse("alt+tab").unwrap()
        );
    }

    #[test]
    fn modifier_aliases_map_to_the_same_modifiers() {
        let super_binding = Binding::parse("super+q").unwrap();
        for alias in ["mod+q", "mod4+q", "logo+q", "win+q", "meta+q", "cmd+q"] {
            assert_eq!(
                Binding::parse(alias).unwrap(),
                super_binding,
                "alias {alias} should equal super"
            );
        }
        assert_eq!(
            Binding::parse("control+q").unwrap(),
            Binding::parse("ctrl+q").unwrap()
        );
    }

    #[test]
    fn modifier_only_and_empty_bindings_are_rejected() {
        assert_eq!(Binding::parse(""), Err(BindError::Empty));
        assert_eq!(Binding::parse("   "), Err(BindError::Empty));
        assert_eq!(Binding::parse("shift"), Err(BindError::MissingKey));
        assert_eq!(Binding::parse("ctrl+alt"), Err(BindError::MissingKey));
        assert_eq!(Binding::parse("super+"), Err(BindError::MissingKey));
        assert_eq!(Binding::parse("super++q"), Err(BindError::MissingKey));
    }

    #[test]
    fn unknown_tokens_are_reported_with_their_context() {
        assert_eq!(
            Binding::parse("hyper+q"),
            Err(BindError::UnknownModifier("hyper".to_owned()))
        );
        assert_eq!(
            Binding::parse("super+bogus"),
            Err(BindError::UnknownKey("bogus".to_owned()))
        );
        assert_eq!(
            Binding::parse("super+f13"),
            Err(BindError::UnknownKey("f13".to_owned()))
        );
    }

    #[test]
    fn duplicate_modifiers_are_rejected() {
        assert_eq!(
            Binding::parse("super+super+q"),
            Err(BindError::DuplicateModifier("super".to_owned()))
        );
        assert_eq!(
            Binding::parse("ctrl+control+q"),
            Err(BindError::DuplicateModifier("control".to_owned()))
        );
    }

    #[test]
    fn matching_is_exact() {
        let binding = Binding::parse("super+q").unwrap();
        assert!(binding.matches(Mods::SUPER, &Key::Char('q')));
        assert!(
            !binding.matches(Mods::SUPER | Mods::SHIFT, &Key::Char('q')),
            "extra modifier must not fire"
        );
        assert!(!binding.matches(Mods::NONE, &Key::Char('q')));
        assert!(!binding.matches(Mods::SUPER, &Key::Char('w')));

        let with_shift = Binding::parse("super+shift+q").unwrap();
        assert!(with_shift.matches(Mods::SUPER | Mods::SHIFT, &Key::Char('q')));
        assert!(!with_shift.matches(Mods::SUPER, &Key::Char('q')));
    }

    #[test]
    fn mods_helpers_behave() {
        assert!(Mods::NONE.is_empty());
        assert!(Mods::SUPER.contains(Mods::SUPER));
        assert!((Mods::SUPER | Mods::SHIFT).contains(Mods::SHIFT));
        assert!(!Mods::SUPER.contains(Mods::SHIFT));
        assert!(
            Mods::SUPER.contains(Mods::NONE),
            "an empty modifier set is trivially contained"
        );
        assert_eq!(
            (Mods::CTRL | Mods::ALT).union(Mods::SHIFT),
            Mods::CTRL | Mods::ALT | Mods::SHIFT
        );
    }

    #[test]
    fn keymap_binds_actions_and_reports_displaced_bindings() {
        let mut keymap = Keymap::new();
        let binding = Binding::parse("super+q").unwrap();
        assert_eq!(keymap.bind(binding, WmAction::CloseWindow), None);
        assert_eq!(
            keymap.bind(binding, WmAction::Launcher),
            Some(WmAction::CloseWindow),
            "rebinding reports the displaced action"
        );
        assert_eq!(keymap.len(), 1, "rebinding does not add a duplicate");
        assert_eq!(
            keymap.action_for(Mods::SUPER, &Key::Char('q')),
            Some(&WmAction::Launcher)
        );
        assert!(keymap.contains(&binding));
    }

    #[test]
    fn action_lookup_is_exact_and_reports_unknowns() {
        let keymap = Keymap::default_glaze();
        assert_eq!(
            keymap.action_for(Mods::SUPER, &Key::Char('q')),
            None,
            "super+q is not bound"
        );
        assert_eq!(
            keymap.action_for(Mods::SUPER | Mods::SHIFT, &Key::Char('q')),
            Some(&WmAction::CloseWindow)
        );
        assert_eq!(
            keymap.action_for(Mods::SUPER, &Key::Char('3')),
            Some(&WmAction::SwitchWorkspace(2))
        );
        assert_eq!(
            keymap.action_for(Mods::SUPER, &Key::Tab),
            Some(&WmAction::NextWorkspace)
        );
        assert_eq!(keymap.action_for(Mods::NONE, &Key::Char('x')), None);
    }

    #[test]
    fn default_keymap_has_no_duplicate_bindings() {
        let keymap = Keymap::default_glaze();
        assert!(
            keymap.duplicate_bindings().is_empty(),
            "duplicates: {:?}",
            keymap.duplicate_bindings()
        );
        assert!(
            keymap.len() >= 11,
            "the default keymap covers the core actions"
        );

        // Every binding is unique.
        let mut keys: Vec<Binding> = keymap.iter().map(|(b, _)| *b).collect();
        let before = keys.len();
        keys.sort();
        keys.dedup();
        assert_eq!(keys.len(), before, "found a duplicate binding");
    }

    #[test]
    fn default_keymap_covers_every_workspace() {
        let keymap = Keymap::default_glaze();
        for index in 0..9u32 {
            let key = Key::Char(char::from_digit(index + 1, 10).unwrap());
            assert_eq!(
                keymap.action_for(Mods::SUPER, &key),
                Some(&WmAction::SwitchWorkspace(index)),
                "super+{} should switch to workspace {index}",
                index + 1
            );
        }
        assert_eq!(keymap.action_for(Mods::SUPER, &Key::Char('0')), None);
    }

    #[test]
    fn from_str_matches_parse() {
        assert_eq!(
            "super+left".parse::<Binding>().unwrap(),
            Binding::parse("super+left").unwrap()
        );
        assert!("bogus".parse::<Binding>().is_err());
    }

    #[test]
    fn bind_error_displays_human_text() {
        assert_eq!(BindError::Empty.to_string(), "empty keybinding");
        assert_eq!(
            BindError::UnknownKey("bogus".to_owned()).to_string(),
            "unknown key: bogus"
        );
    }
}
