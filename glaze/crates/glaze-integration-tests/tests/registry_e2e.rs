//! End-to-end registry tests: real `.desktop` files on disk -> real
//! `AppRegistry::discover_in` -> the exact set and order of visible apps.
//!
//! These are genuine integration tests (no `#[cfg(test)]` module): they drive
//! the public `glaze-service` API the shell actually calls, using an XDG-style
//! temporary directory tree.

mod common;

use std::collections::BTreeSet;

use common::{cleanup, options, options_with_no_display, temp_root, write_desktop};
use glaze_service::AppRegistry;

/// Write the shared fixture tree:
///
/// * visible: `alpha`, `beta`, `localised` (with a `Name[de]`), `utils-note`
///   (nested, to exercise id mapping) and `zulu`
/// * excluded: `hidden`, `nodisplay`, `broken` (no `Exec`) and `x11-only`
///   (an entry that only declares a non-Glaze desktop via `OnlyShowIn`)
fn write_fixtures(root: &std::path::Path) {
    write_desktop(
        root,
        "alpha.desktop",
        "[Desktop Entry]\nType=Application\nName=Alpha\nExec=alpha %U\nIcon=alpha\n",
    );
    write_desktop(
        root,
        "beta.desktop",
        "[Desktop Entry]\nType=Application\nName=Beta\nGenericName=Example\nExec=beta --go\n",
    );
    write_desktop(
        root,
        "localised.desktop",
        "[Desktop Entry]\nType=Application\nName=Localised App\nName[de]=Lokalisierte App\nExec=localised\n",
    );
    write_desktop(
        root,
        "utils/note.desktop",
        "[Desktop Entry]\nType=Application\nName=Note\nExec=note --new\n",
    );
    write_desktop(
        root,
        "zulu.desktop",
        "[Desktop Entry]\nType=Application\nName=Zulu\nExec=zulu\n",
    );

    // Excluded: explicitly hidden from menus.
    write_desktop(
        root,
        "hidden.desktop",
        "[Desktop Entry]\nType=Application\nName=Hidden One\nExec=hidden\nHidden=true\n",
    );
    // Excluded: marked NoDisplay (kept out by default, included on request).
    write_desktop(
        root,
        "nodisplay.desktop",
        "[Desktop Entry]\nType=Application\nName=Nodisplay Demo\nExec=nodisplay\nNoDisplay=true\n",
    );
    // Excluded: broken, because it has no Exec at all.
    write_desktop(
        root,
        "broken.desktop",
        "[Desktop Entry]\nType=Application\nName=Broken\nIcon=broken\n",
    );
    // Excluded: this entry only supports a different (X11-era) desktop, which
    // is not the Glaze session, so it must not appear.
    write_desktop(
        root,
        "x11-only.desktop",
        "[Desktop Entry]\nType=Application\nName=X11 Only\nExec=x11only\nOnlyShowIn=KDE;\n",
    );
}

#[test]
fn discovers_only_visible_entries_in_name_order() {
    let root = temp_root("registry-visible");
    write_fixtures(&root);

    let reg = AppRegistry::discover_in(std::slice::from_ref(&root), options("de_DE.UTF-8"));

    let pairs: Vec<(String, String)> = reg
        .apps()
        .iter()
        .map(|a| (a.id.clone(), a.name.clone()))
        .collect();

    // Ordering is by lowercased display name, then id. The German locale makes
    // `localised` render as "Lokalisierte App" (l < n < z).
    assert_eq!(
        pairs,
        vec![
            ("alpha".to_string(), "Alpha".to_string()),
            ("beta".to_string(), "Beta".to_string()),
            ("localised".to_string(), "Lokalisierte App".to_string()),
            ("utils-note".to_string(), "Note".to_string()),
            ("zulu".to_string(), "Zulu".to_string()),
        ]
    );

    // The exact visible id set, with the nested id flattened per the XDG spec.
    let ids: BTreeSet<&str> = reg.apps().iter().map(|a| a.id.as_str()).collect();
    assert_eq!(
        ids,
        BTreeSet::from(["alpha", "beta", "localised", "utils-note", "zulu"])
    );

    for hidden in ["hidden", "nodisplay", "broken", "x11-only"] {
        assert!(
            reg.get(hidden).is_none(),
            "{hidden} must not be visible by default"
        );
    }

    cleanup(&root);
}

#[test]
fn localized_name_falls_back_to_base_for_en() {
    let root = temp_root("registry-en");
    write_fixtures(&root);

    let reg = AppRegistry::discover_in(std::slice::from_ref(&root), options("en"));
    assert_eq!(reg.get("localised").unwrap().name, "Localised App");

    // Swapping only the locale must not change the id set.
    let ids: Vec<&str> = reg.apps().iter().map(|a| a.id.as_str()).collect();
    assert_eq!(
        ids,
        vec!["alpha", "beta", "localised", "utils-note", "zulu"]
    );

    cleanup(&root);
}

#[test]
fn no_display_entries_are_opt_in_only() {
    let root = temp_root("registry-nodisplay");
    write_fixtures(&root);

    let reg = AppRegistry::discover_in(std::slice::from_ref(&root), options_with_no_display("en"));
    let ids: BTreeSet<&str> = reg.apps().iter().map(|a| a.id.as_str()).collect();

    assert!(
        ids.contains("nodisplay"),
        "NoDisplay entry should be included"
    );
    assert!(reg.get("nodisplay").unwrap().no_display);
    // Hidden / broken / foreign-desktop entries stay out regardless.
    assert!(!ids.contains("hidden"));
    assert!(!ids.contains("broken"));
    assert!(!ids.contains("x11-only"));

    cleanup(&root);
}

#[test]
fn user_directory_shadows_system_directory() {
    let user = temp_root("registry-shadow-user");
    let sys = temp_root("registry-shadow-sys");
    write_desktop(
        &user,
        "editor.desktop",
        "[Desktop Entry]\nType=Application\nName=User Editor\nExec=ue\n",
    );
    write_desktop(
        &sys,
        "editor.desktop",
        "[Desktop Entry]\nType=Application\nName=System Editor\nExec=se\n",
    );

    // Precedence: earlier directories win, so the user entry shadows the system
    // one with the same desktop id.
    let reg = AppRegistry::discover_in(&[user.clone(), sys.clone()], options("en"));
    assert_eq!(reg.len(), 1);
    assert_eq!(reg.get("editor").unwrap().name, "User Editor");

    cleanup(&user);
    cleanup(&sys);
}

#[test]
fn missing_directory_is_empty_not_an_error() {
    let missing = temp_root("registry-missing").join("does-not-exist");
    let reg = AppRegistry::discover_in(std::slice::from_ref(&missing), options("en"));
    assert!(reg.is_empty());
}
