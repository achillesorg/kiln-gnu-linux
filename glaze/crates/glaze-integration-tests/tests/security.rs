//! Security tests: `.desktop` `Exec` data must never reach a shell.
//!
//! `GLAZE.md` §7.9 and §9 ("Security") require that a malicious `.desktop` file
//! cannot inject shell commands. The contract is that `Exec` is tokenised into
//! an argument vector in `glaze-service::desktop` and handed to
//! `Command::new(argv[0])` with `argv[1..]` as literal arguments — there is no
//! `sh -c` anywhere in the launch path.

mod common;

use common::{cleanup, options, temp_root, write_desktop};
use glaze_service::desktop::{tokenize_exec, ExecContext};
use glaze_service::{build_command, AppRegistry};

/// True when any argument would ask a shell to interpret a string.
fn mentions_a_shell(argv: &[String]) -> bool {
    argv.iter()
        .any(|a| matches!(a.as_str(), "sh" | "bash" | "/bin/sh" | "/bin/bash" | "-c"))
}

#[test]
fn semicolon_chain_is_inert_argv() {
    let argv = tokenize_exec("app ; rm -rf /", &ExecContext::default()).unwrap();
    assert_eq!(argv, vec!["app", ";", "rm", "-rf", "/"]);
    // Critically, the program is `app` — `rm` is only a literal argument.
    assert_eq!(argv[0], "app");
    assert!(!mentions_a_shell(&argv));
}

#[test]
fn command_substitution_stays_literal() {
    let argv = tokenize_exec("app $(rm -rf /)", &ExecContext::default()).unwrap();
    assert_eq!(argv, vec!["app", "$(rm", "-rf", "/)"]);
    assert_eq!(argv[0], "app");
    assert!(!mentions_a_shell(&argv));
}

#[test]
fn backticks_stay_literal() {
    let argv = tokenize_exec("app `whoami`", &ExecContext::default()).unwrap();
    assert_eq!(argv, vec!["app", "`whoami`"]);
    assert!(!mentions_a_shell(&argv));
}

#[test]
fn quoted_substitution_is_one_literal_argument() {
    // Inside double quotes only `"`, `` ` ``, `$` and `\` are escapes, so the
    // substitution remains a single literal argument after unquoting.
    let argv = tokenize_exec("\"$(rm -rf /)\"", &ExecContext::default()).unwrap();
    assert_eq!(argv, vec!["$(rm -rf /)"]);
    assert!(!mentions_a_shell(&argv));
}

#[test]
fn pipe_and_redirect_stay_literal() {
    let argv = tokenize_exec("app | tee /etc/passwd > /dev/null", &ExecContext::default()).unwrap();
    assert_eq!(argv[0], "app");
    assert!(argv.contains(&"|".to_string()));
    assert!(argv.contains(&">".to_string()));
    assert!(!mentions_a_shell(&argv));
}

#[test]
fn malicious_desktop_entry_never_reaches_a_shell() {
    let root = temp_root("security-endtoend");
    write_desktop(
        &root,
        "evil.desktop",
        "[Desktop Entry]\nType=Application\nName=Evil\nExec=app ; rm -rf /\n",
    );

    // The entry is discovered (it is syntactically valid) and its Exec is
    // carried through the real registry -> launch pipeline.
    let reg = AppRegistry::discover_in(std::slice::from_ref(&root), options("en"));
    let app = reg.get("evil").expect("entry is registered");

    let argv = build_command(app).expect("command builds");
    assert_eq!(argv, vec!["app", ";", "rm", "-rf", "/"]);
    assert_eq!(argv[0], "app");
    assert!(!mentions_a_shell(&argv));

    cleanup(&root);
}
