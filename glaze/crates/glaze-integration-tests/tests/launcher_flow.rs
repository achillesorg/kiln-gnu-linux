//! End-to-end launcher flow: real `.desktop` fixtures -> `AppRegistry` ->
//! `search_apps` -> `build_command` -> the exact argv that would be spawned.
//!
//! This is the path the shell takes for every menu launch (`GLAZE.md` §4.1,
//! §6.2). The command is asserted to be a plain argv vector — never a shell.

mod common;

use common::{cleanup, options, temp_root, write_desktop};
use glaze_service::{build_command, search_apps, AppRegistry};

fn write_fixtures(root: &std::path::Path) {
    write_desktop(
        root,
        "firefox.desktop",
        "[Desktop Entry]\nType=Application\nName=Firefox\nComment=Browse the web\n\
         Exec=/usr/bin/firefox %u\nIcon=firefox\nKeywords=browser;internet;\n",
    );
    write_desktop(
        root,
        "files.desktop",
        "[Desktop Entry]\nType=Application\nName=Files\nComment=Access and organize files\n\
         Exec=/usr/bin/nautilus %U\nKeywords=file manager;folder;\n",
    );
    write_desktop(
        root,
        "text-editor.desktop",
        "[Desktop Entry]\nType=Application\nName=Text Editor\nComment=Edit text files\n\
         Exec=/usr/bin/gedit %F\nKeywords=text;editor;\n",
    );
}

#[test]
fn search_then_build_argv_for_top_hit() {
    let root = temp_root("launcher-search");
    write_fixtures(&root);
    let reg = AppRegistry::discover_in(std::slice::from_ref(&root), options("en"));

    let hits = search_apps(reg.apps(), "firefox", 8);
    assert_eq!(hits[0].app_id, "firefox");

    let app = reg.get(&hits[0].app_id).expect("top hit is registered");
    let argv = build_command(app).expect("command builds");

    // `%u` is a removable field code, so only the program remains.
    assert_eq!(argv, vec!["/usr/bin/firefox"]);
    // No shell in the launch path: argv[0] is the real program.
    assert_eq!(argv[0], "/usr/bin/firefox");
    assert!(!argv.iter().any(|a| a == "sh" || a == "bash" || a == "-c"));

    cleanup(&root);
}

#[test]
fn keyword_search_resolves_to_the_keyword_owner() {
    let root = temp_root("launcher-keyword");
    write_fixtures(&root);
    let reg = AppRegistry::discover_in(std::slice::from_ref(&root), options("en"));

    let hits = search_apps(reg.apps(), "browser", 8);
    assert_eq!(hits[0].app_id, "firefox");

    let argv = build_command(reg.get(&hits[0].app_id).unwrap()).unwrap();
    assert_eq!(argv, vec!["/usr/bin/firefox"]);

    cleanup(&root);
}

#[test]
fn multi_term_query_selects_the_text_editor() {
    let root = temp_root("launcher-multiterm");
    write_fixtures(&root);
    let reg = AppRegistry::discover_in(std::slice::from_ref(&root), options("en"));

    let hits = search_apps(reg.apps(), "text editor", 8);
    assert_eq!(hits.len(), 1, "only the editor matches both terms");
    assert_eq!(hits[0].app_id, "text-editor");

    let argv = build_command(reg.get(&hits[0].app_id).unwrap()).unwrap();
    assert_eq!(argv, vec!["/usr/bin/gedit"]);

    cleanup(&root);
}

#[test]
fn desktop_file_field_code_expands_to_the_source_path() {
    let root = temp_root("launcher-fieldcode");
    let path = write_desktop(
        &root,
        "desk.desktop",
        "[Desktop Entry]\nType=Application\nName=Desk\nExec=desk %k\n",
    );
    let reg = AppRegistry::discover_in(std::slice::from_ref(&root), options("en"));
    let app = reg.get("desk").unwrap();

    let argv = build_command(app).unwrap();
    assert_eq!(
        argv,
        vec!["desk".to_string(), path.to_string_lossy().into_owned()]
    );

    cleanup(&root);
}

#[test]
fn unrelated_query_yields_no_hits() {
    let root = temp_root("launcher-none");
    write_fixtures(&root);
    let reg = AppRegistry::discover_in(std::slice::from_ref(&root), options("en"));

    assert!(search_apps(reg.apps(), "zzzznope", 8).is_empty());

    cleanup(&root);
}
