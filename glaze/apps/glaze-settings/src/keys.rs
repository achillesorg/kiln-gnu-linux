//! The 20 Glaze keyboard shortcuts, their defaults, parsing, display and
//! conflict detection.
//!
//! Defaults come from `glaze_wm::keybindings::Keymap::default_glaze` (15 window
//! actions plus the two workspace families) and the session's Sway config
//! (terminal, fullscreen) plus the compositor's exit chord. The two workspace
//! families are edited as a modifier prefix: `super` means `Super+1` ...
//! `Super+9`.
//!
//! The user's choices are stored in the settings sidecar as
//! `[keybindings] <action-id> = "super+shift+q"` (an empty string disables a
//! shortcut). No Glaze component reads that table yet: see
//! `docs/settings-needs.md`.

use std::collections::BTreeMap;

use glaze_wm::keybindings::{Binding, Key, Mods};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// One chord.
    Single,
    /// A modifier prefix applied to the digits 1..=9.
    Family,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Action {
    pub id: &'static str,
    pub entry_id: &'static str,
    pub title: &'static str,
    pub keywords: &'static str,
    pub default: &'static str,
    pub kind: Kind,
}

const fn a(
    id: &'static str,
    entry_id: &'static str,
    title: &'static str,
    keywords: &'static str,
    default: &'static str,
    kind: Kind,
) -> Action {
    Action {
        id,
        entry_id,
        title,
        keywords,
        default,
        kind,
    }
}

use Kind::{Family, Single};

pub const ACTIONS: [Action; 20] = [
    a(
        "close-window",
        "keys.close-window",
        "Close window",
        "shortcut quit kill",
        "super+shift+q",
        Single,
    ),
    a(
        "toggle-maximize",
        "keys.toggle-maximize",
        "Maximise or restore",
        "shortcut maximize zoom",
        "super+up",
        Single,
    ),
    a(
        "snap-left",
        "keys.snap-left",
        "Snap to left half",
        "shortcut snap half tile",
        "super+left",
        Single,
    ),
    a(
        "snap-right",
        "keys.snap-right",
        "Snap to right half",
        "shortcut snap half tile",
        "super+right",
        Single,
    ),
    a(
        "snap-bottom",
        "keys.snap-bottom",
        "Snap to bottom half",
        "shortcut snap half",
        "super+down",
        Single,
    ),
    a(
        "move-left",
        "keys.move-left",
        "Move window left",
        "shortcut move",
        "super+shift+left",
        Single,
    ),
    a(
        "move-right",
        "keys.move-right",
        "Move window right",
        "shortcut move",
        "super+shift+right",
        Single,
    ),
    a(
        "toggle-tiling",
        "keys.toggle-tiling",
        "Toggle tiling (workspace)",
        "shortcut tile float stacking",
        "super+t",
        Single,
    ),
    a(
        "toggle-tiling-global",
        "keys.toggle-tiling-global",
        "Toggle tiling (everywhere)",
        "shortcut tile float global",
        "super+shift+t",
        Single,
    ),
    a(
        "minimize",
        "keys.minimize",
        "Minimise window",
        "shortcut minimize hide",
        "super+h",
        Single,
    ),
    a(
        "cycle-windows",
        "keys.cycle-windows",
        "Switch windows",
        "shortcut alt tab cycle",
        "alt+tab",
        Single,
    ),
    a(
        "cycle-windows-back",
        "keys.cycle-windows-back",
        "Switch windows backwards",
        "shortcut alt shift tab cycle",
        "alt+shift+tab",
        Single,
    ),
    a(
        "launcher",
        "keys.launcher",
        "Open Start menu",
        "shortcut launcher search apps",
        "super+space",
        Single,
    ),
    a(
        "next-workspace",
        "keys.next-workspace",
        "Next workspace",
        "shortcut desktop virtual",
        "super+tab",
        Single,
    ),
    a(
        "prev-workspace",
        "keys.prev-workspace",
        "Previous workspace",
        "shortcut desktop virtual",
        "super+shift+tab",
        Single,
    ),
    a(
        "switch-workspace",
        "keys.switch-workspace",
        "Go to workspace 1-9",
        "shortcut desktop virtual number",
        "super",
        Family,
    ),
    a(
        "move-to-workspace",
        "keys.move-to-workspace",
        "Move window to workspace 1-9",
        "shortcut send desktop number",
        "super+shift",
        Family,
    ),
    a(
        "terminal",
        "keys.terminal",
        "Open terminal",
        "shortcut console foot",
        "super+return",
        Single,
    ),
    a(
        "fullscreen",
        "keys.fullscreen",
        "Toggle fullscreen",
        "shortcut full screen",
        "super+f",
        Single,
    ),
    a(
        "exit-session",
        "keys.exit-session",
        "Exit session",
        "shortcut log out quit compositor",
        "ctrl+alt+backspace",
        Single,
    ),
];

pub fn action(id: &str) -> Option<&'static Action> {
    ACTIONS.iter().find(|a| a.id == id)
}

/// Parse a modifier prefix (`"super+shift"`).
pub fn parse_mods(s: &str) -> Result<Mods, String> {
    let s = s.trim();
    if s.is_empty() {
        return Err("choose at least one modifier".into());
    }
    // Reuse the binding parser with a dummy key so the modifier names match.
    Binding::parse(&format!("{s}+1"))
        .map(|b| b.mods)
        .map_err(|e| e.to_string())
}

/// Parse and validate a user-typed chord for an action.
pub fn parse_for(action: &Action, s: &str) -> Result<Spec, String> {
    let s = s.trim();
    if s.is_empty() {
        return Ok(Spec::Disabled);
    }
    match action.kind {
        Kind::Single => {
            let b = Binding::parse(s).map_err(|e| e.to_string())?;
            if b.mods.is_empty() && !matches!(b.key, Key::Function(_)) {
                return Err(
                    "add a modifier (Super, Ctrl or Alt): a bare key would be taken from every app"
                        .into(),
                );
            }
            if b.mods == Mods::SHIFT && !matches!(b.key, Key::Function(_)) {
                return Err("Shift alone is not enough: it would steal typed capitals".into());
            }
            Ok(Spec::Chord(b))
        }
        Kind::Family => {
            let m = parse_mods(s)?;
            if m == Mods::SHIFT {
                return Err("Shift alone would steal the digits from every app".into());
            }
            Ok(Spec::Family(m))
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Spec {
    Disabled,
    Chord(Binding),
    Family(Mods),
}

impl Spec {
    /// The concrete chords this spec occupies.
    pub fn chords(&self) -> Vec<Binding> {
        match self {
            Spec::Disabled => Vec::new(),
            Spec::Chord(b) => vec![*b],
            Spec::Family(m) => (1..=9)
                .map(|d| Binding::new(*m, Key::Char(char::from(b'0' + d))))
                .collect(),
        }
    }

    /// Canonical stored form (`super+shift+q`, `super+shift`).
    pub fn to_config(self) -> String {
        match self {
            Spec::Disabled => String::new(),
            Spec::Chord(b) => {
                let mut parts = mods_tokens(b.mods);
                parts.push(key_token(&b.key));
                parts.join("+")
            }
            Spec::Family(m) => mods_tokens(m).join("+"),
        }
    }

    /// Human form (`Super+Shift+Q`, `Super+1…9`).
    pub fn display(&self) -> String {
        match self {
            Spec::Disabled => "Disabled".into(),
            Spec::Chord(b) => {
                let mut parts = mods_labels(b.mods);
                parts.push(key_label(&b.key));
                parts.join("+")
            }
            Spec::Family(m) => {
                let mut parts = mods_labels(*m);
                parts.push("1…9".into());
                parts.join("+")
            }
        }
    }
}

fn mods_tokens(m: Mods) -> Vec<String> {
    let mut v = Vec::new();
    if m.ctrl {
        v.push("ctrl".to_string());
    }
    if m.alt {
        v.push("alt".to_string());
    }
    if m.super_ {
        v.push("super".to_string());
    }
    if m.shift {
        v.push("shift".to_string());
    }
    // Keep the conventional order used by the defaults: super before shift,
    // ctrl before alt.
    v.sort_by_key(|t| match t.as_str() {
        "ctrl" => 0,
        "alt" => 1,
        "super" => 2,
        _ => 3,
    });
    v
}

fn mods_labels(m: Mods) -> Vec<String> {
    mods_tokens(m)
        .into_iter()
        .map(|t| match t.as_str() {
            "ctrl" => "Ctrl".to_string(),
            "alt" => "Alt".to_string(),
            "super" => "Super".to_string(),
            _ => "Shift".to_string(),
        })
        .collect()
}

pub fn key_token(k: &Key) -> String {
    match k {
        Key::Char(c) => c.to_string(),
        Key::Tab => "tab".into(),
        Key::Escape => "escape".into(),
        Key::Enter => "return".into(),
        Key::Space => "space".into(),
        Key::Left => "left".into(),
        Key::Right => "right".into(),
        Key::Up => "up".into(),
        Key::Down => "down".into(),
        Key::Home => "home".into(),
        Key::End => "end".into(),
        Key::PageUp => "pageup".into(),
        Key::PageDown => "pagedown".into(),
        Key::Delete => "delete".into(),
        Key::Backspace => "backspace".into(),
        Key::Function(n) => format!("f{n}"),
    }
}

fn key_label(k: &Key) -> String {
    match k {
        Key::Char(c) => c.to_ascii_uppercase().to_string(),
        Key::Enter => "Return".into(),
        Key::PageUp => "Page Up".into(),
        Key::PageDown => "Page Down".into(),
        Key::Function(n) => format!("F{n}"),
        other => {
            let t = key_token(other);
            let mut c = t.chars();
            c.next()
                .map(|f| f.to_ascii_uppercase().to_string() + c.as_str())
                .unwrap_or_default()
        }
    }
}

/// The effective keymap: defaults overridden by the sidecar table.
pub fn effective(table: Option<&toml::Table>) -> BTreeMap<&'static str, Spec> {
    let mut out = BTreeMap::new();
    for act in &ACTIONS {
        let user = table
            .and_then(|t| t.get(act.id))
            .and_then(|v| v.as_str())
            .and_then(|s| parse_for(act, s).ok());
        let spec = user.unwrap_or_else(|| parse_for(act, act.default).expect("default parses"));
        out.insert(act.id, spec);
    }
    out
}

/// A clash between two actions (or an action and a reserved system chord).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Conflict {
    pub action: &'static str,
    /// The other action id, or `None` for a reserved system chord.
    pub other: Option<&'static str>,
    pub chord: String,
    pub reason: String,
}

/// Chords the system owns regardless of Glaze (VT switching).
pub fn reserved(b: &Binding) -> Option<&'static str> {
    if b.mods == (Mods::CTRL | Mods::ALT) && matches!(b.key, Key::Function(_)) {
        return Some("switches virtual terminals");
    }
    if b.mods == (Mods::CTRL | Mods::ALT) && b.key == Key::Delete {
        return Some("is reserved by the system");
    }
    None
}

/// Every conflict in a keymap, reported once per pair, in `ACTIONS` order.
pub fn conflicts(map: &BTreeMap<&'static str, Spec>) -> Vec<Conflict> {
    let mut out = Vec::new();
    let ordered: Vec<(&'static str, Vec<Binding>)> = ACTIONS
        .iter()
        .map(|a| (a.id, map.get(a.id).map(Spec::chords).unwrap_or_default()))
        .collect();
    for (i, (id, chords)) in ordered.iter().enumerate() {
        for c in chords {
            if let Some(why) = reserved(c) {
                out.push(Conflict {
                    action: id,
                    other: None,
                    chord: Spec::Chord(*c).display(),
                    reason: format!("{} {why}", Spec::Chord(*c).display()),
                });
            }
            for (other, oc) in &ordered[i + 1..] {
                if oc.contains(c) {
                    let other_title = action(other).map(|a| a.title).unwrap_or(other);
                    out.push(Conflict {
                        action: id,
                        other: Some(other),
                        chord: Spec::Chord(*c).display(),
                        reason: format!(
                            "{} is also used by \"{other_title}\"",
                            Spec::Chord(*c).display()
                        ),
                    });
                }
            }
        }
    }
    // A family overlapping a single chord reports each digit; keep the first.
    let mut seen = std::collections::HashSet::new();
    out.retain(|c| seen.insert((c.action, c.other)));
    out
}

/// Conflicts that a proposed spec for `id` would create with the rest of `map`.
pub fn conflicts_if(
    map: &BTreeMap<&'static str, Spec>,
    id: &'static str,
    spec: Spec,
) -> Vec<Conflict> {
    let mut m = map.clone();
    m.insert(id, spec);
    conflicts(&m)
        .into_iter()
        .filter(|c| c.action == id || c.other == Some(id))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn there_are_twenty_actions_and_defaults_parse_without_conflicts() {
        assert_eq!(ACTIONS.len(), 20);
        let map = effective(None);
        assert_eq!(map.len(), 20);
        assert!(conflicts(&map).is_empty(), "{:?}", conflicts(&map));
    }

    #[test]
    fn defaults_match_the_glaze_wm_keymap() {
        let wm = glaze_wm::keybindings::Keymap::default_glaze();
        let map = effective(None);
        for (b, _) in wm.iter() {
            assert!(
                map.values().any(|s| s.chords().contains(b)),
                "glaze-wm binding {b:?} missing from the settings defaults"
            );
        }
    }

    #[test]
    fn canonical_and_display_forms() {
        let act = action("close-window").unwrap();
        let s = parse_for(act, "Shift+Super+Q").unwrap();
        assert_eq!(s.to_config(), "super+shift+q");
        assert_eq!(s.display(), "Super+Shift+Q");
        let fam = parse_for(action("switch-workspace").unwrap(), "super").unwrap();
        assert_eq!(fam.display(), "Super+1…9");
        assert_eq!(fam.chords().len(), 9);
        assert_eq!(parse_for(act, "").unwrap(), Spec::Disabled);
        assert_eq!(Spec::Disabled.display(), "Disabled");
    }

    #[test]
    fn bad_chords_are_rejected_with_reasons() {
        let act = action("launcher").unwrap();
        assert!(parse_for(act, "q").unwrap_err().contains("modifier"));
        assert!(parse_for(act, "shift+a").unwrap_err().contains("Shift"));
        assert!(parse_for(act, "super+nope").is_err());
        assert!(parse_for(act, "hyper+q").is_err());
        assert!(parse_for(act, "f12").is_ok());
        assert!(parse_for(action("switch-workspace").unwrap(), "shift").is_err());
    }

    #[test]
    fn conflicts_are_found_between_actions_families_and_reserved_chords() {
        let mut t = toml::Table::new();
        t.insert("minimize".into(), "super+t".into());
        let map = effective(Some(&t));
        let c = conflicts(&map);
        assert_eq!(c.len(), 1);
        assert_eq!(c[0].action, "toggle-tiling");
        assert_eq!(c[0].other, Some("minimize"));
        assert!(c[0].reason.contains("Super+T"));

        // A single chord on a family digit.
        let base = effective(None);
        let clash = conflicts_if(
            &base,
            "terminal",
            parse_for(action("terminal").unwrap(), "super+3").unwrap(),
        );
        assert_eq!(clash.len(), 1);
        assert_eq!(clash[0].action, "switch-workspace");

        // A family moved onto another family's modifiers.
        let fam = conflicts_if(&base, "move-to-workspace", Spec::Family(Mods::SUPER));
        assert_eq!(fam.len(), 1);

        // Reserved VT chord.
        let vt = conflicts_if(
            &base,
            "launcher",
            parse_for(action("launcher").unwrap(), "ctrl+alt+f2").unwrap(),
        );
        assert_eq!(vt.len(), 1);
        assert!(vt[0].other.is_none());

        // Disabling removes a clash.
        let mut t = toml::Table::new();
        t.insert("minimize".into(), "super+t".into());
        t.insert("toggle-tiling".into(), "".into());
        assert!(conflicts(&effective(Some(&t))).is_empty());
    }

    #[test]
    fn invalid_stored_values_fall_back_to_defaults() {
        let mut t = toml::Table::new();
        t.insert("close-window".into(), "nonsense+".into());
        let map = effective(Some(&t));
        assert_eq!(map["close-window"].to_config(), "super+shift+q");
    }
}
