//! Running a session/power action — the only platform-specific part.
//!
//! Kept deliberately thin and obviously correct: it refuses anything Glaze has
//! not verified, builds the argv with [`crate::command::command_for`], and
//! spawns it **without a shell**. It never waits for the child, because a
//! successful shutdown/reboot will tear this process down anyway.
//!
//! This is Unix-only. On other hosts every action is reported
//! [`crate::PowerError::Unsupported`] so calling code has one uniform result
//! type.

use crate::actions::SessionAction;
use crate::env::SessionEnv;
use crate::PowerError;

/// Run `action` in `env`.
///
/// Returns [`PowerError::Unsupported`] when
/// [`crate::command::unavailable_reason`] is `Some`, [`PowerError::Spawn`] when
/// the process could not be started, and `Ok(())` once the process has been
/// spawned.
#[cfg(unix)]
pub fn invoke(action: SessionAction, env: &SessionEnv) -> Result<(), PowerError> {
    use crate::command::{command_for, unavailable_reason};

    if let Some(reason) = unavailable_reason(action, env) {
        return Err(PowerError::Unsupported(reason));
    }

    let argv = command_for(action, env);
    let Some((program, args)) = argv.split_first() else {
        return Err(PowerError::Unsupported("no command to run"));
    };

    use std::process::{Command, Stdio};
    Command::new(program)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map(|_child| ())
        .map_err(|e| PowerError::Spawn(e.to_string()))
}

/// Non-Unix hosts have no session/power integration; refuse uniformly.
#[cfg(not(unix))]
pub fn invoke(_action: SessionAction, _env: &SessionEnv) -> Result<(), PowerError> {
    Err(PowerError::Unsupported(
        "session/power actions are only supported on Unix hosts",
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn invoke_refuses_actions_glaze_cannot_verify() {
        // `switch user` is never available on any host, so this test is portable
        // and never spawns anything or changes the session.
        let env = SessionEnv::new(Some("1".into()), true, true, true, true);
        let err = invoke(SessionAction::SwitchUser, &env).unwrap_err();
        assert!(matches!(err, PowerError::Unsupported(_)), "{err:?}");
    }

    #[cfg(not(unix))]
    #[test]
    fn invoke_is_unsupported_off_unix() {
        let env = SessionEnv::new(Some("1".into()), true, true, true, true);
        let err = invoke(SessionAction::Lock, &env).unwrap_err();
        assert!(matches!(err, PowerError::Unsupported(_)), "{err:?}");
    }
}
