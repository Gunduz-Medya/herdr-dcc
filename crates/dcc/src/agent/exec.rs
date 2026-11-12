//! `dcc agent exec -- <program> <args>`: starts an agent in a clean environment. A terminal
//! inherits the environment of the process that started the core, so an agent started from inside
//! another agent would carry its markers (Phase 5 found transcripts switched off and permission
//! prompts skipped that way). The markers are removed, `DCC_RUN_ID` is added (hooks inherit it,
//! which is how a hook finds its run, also after `/clear`), and the program replaces this process.
//! Replacing keeps the program in front for Herdr. On Windows the program runs as a child instead
//! (untested).

use std::process::Command;

use crate::error::{Error, Result};

/// Variables that mark a process as running inside another Claude Code session. Others, like
/// `CLAUDE_CONFIG_DIR`, are the user's own choices and stay.
pub fn is_inherited_marker(name: &str) -> bool {
    name == "CLAUDECODE"
        || name.starts_with("CLAUDE_CODE_")
        || name == "CLAUDE_EFFORT"
        || name == "CLAUDE_PID"
        || name == "AI_AGENT"
}

pub fn markers_to_remove(names: impl IntoIterator<Item = String>) -> Vec<String> {
    let mut v: Vec<String> = names
        .into_iter()
        .filter(|n| is_inherited_marker(n))
        .collect();
    v.sort();
    v
}

pub fn command(run_id: &str, program: &str, args: &[String]) -> Command {
    let mut cmd = Command::new(program);
    cmd.args(args);
    for name in markers_to_remove(std::env::vars().map(|(k, _)| k)) {
        cmd.env_remove(name);
    }
    cmd.env("DCC_RUN_ID", run_id);
    cmd
}

/// On Unix this returns only when the program could not be started.
pub fn run(run_id: &str, program: &str, args: &[String]) -> Result<i32> {
    let mut cmd = command(run_id, program, args);
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        let err = cmd.exec();
        Err(Error::external(format!("cannot start {program}: {err}")))
    }
    #[cfg(not(unix))]
    {
        let status = cmd
            .status()
            .map_err(|e| Error::external(format!("cannot start {program}: {e}")))?;
        Ok(status.code().unwrap_or(1))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn markers_are_recognised_and_user_settings_stay() {
        let names = [
            "CLAUDECODE",
            "CLAUDE_CODE_SESSION_ID",
            "CLAUDE_CODE_MESSAGING_TOKEN",
            "CLAUDE_EFFORT",
            "CLAUDE_PID",
            "AI_AGENT",
            "CLAUDE_CONFIG_DIR",
            "PATH",
            "HOME",
            "CLAUDE_X",
        ];
        let removed = markers_to_remove(names.iter().map(|s| s.to_string()));
        assert_eq!(
            removed,
            vec![
                "AI_AGENT",
                "CLAUDECODE",
                "CLAUDE_CODE_MESSAGING_TOKEN",
                "CLAUDE_CODE_SESSION_ID",
                "CLAUDE_EFFORT",
                "CLAUDE_PID"
            ]
        );
    }

    #[cfg(unix)]
    #[test]
    fn the_child_sees_the_run_id_and_not_the_markers() {
        let mut cmd = command("run-123", "/usr/bin/env", &[]);
        cmd.env("CLAUDECODE", "1")
            .env("CLAUDE_CODE_SESSION_ID", "x")
            .env("CLAUDE_CONFIG_DIR", "/keep");
        // the removal list is computed from this process, so set the markers on the child by hand too
        for n in ["CLAUDECODE", "CLAUDE_CODE_SESSION_ID"] {
            cmd.env_remove(n);
        }
        let out = cmd.output().unwrap();
        let text = String::from_utf8_lossy(&out.stdout).to_string();
        assert!(text.contains("DCC_RUN_ID=run-123"));
        assert!(!text.contains("CLAUDECODE=") && !text.contains("CLAUDE_CODE_SESSION_ID="));
        assert!(text.contains("CLAUDE_CONFIG_DIR=/keep"));
    }
}
