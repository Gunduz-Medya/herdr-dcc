//! One adapter per agent kind. An adapter knows how its agent is recognised, how its session id
//! shows up in the argv, how to start it so DCC gets the id and the hooks, and what it can offer.
//! Claude Code is the only agent that was tested. Anything else gets the generic adapter, which
//! has nothing but Herdr's status and the folder.

use std::path::{Path, PathBuf};

use serde_json::{Value, json};

use super::hooks::valid_session_id;
use crate::error::{Error, Result};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Capabilities {
    pub session_id: bool,
    pub hooks: bool,
    pub statusline: bool,
    pub usage: bool,
}

#[derive(Debug, Clone, Default)]
pub struct LaunchRequest {
    pub model: Option<String>,
    pub permission_mode: Option<String>,
    pub extra_args: Vec<String>,
}

pub struct LaunchContext<'a> {
    /// Absolute path of the agent program (the core may not have the user's PATH).
    pub program_path: &'a str,
    pub run_id: &'a str,
    pub settings_path: &'a Path,
    pub dcc_exe: &'a Path,
    pub data_dir: &'a Path,
}

#[derive(Debug, Clone)]
pub struct LaunchPlan {
    pub program: String,
    pub args: Vec<String>,
    /// The settings file content to write at `settings_path`, if the agent takes one.
    pub settings: Option<Value>,
}

pub trait AgentAdapter {
    fn kind(&self) -> &'static str;
    /// Does this terminal run this kind of agent, judged by the program name and its argv.
    fn matches(&self, program: &str, argv: &[String]) -> bool;
    fn session_id_from_argv(&self, argv: &[String]) -> Option<String>;
    fn launch_plan(&self, ctx: &LaunchContext, req: &LaunchRequest) -> Result<LaunchPlan>;
    fn capabilities(&self) -> Capabilities;
}

fn basename(s: &str) -> &str {
    s.rsplit(['/', '\\'])
        .next()
        .unwrap_or(s)
        .trim_end_matches(".exe")
}

/// Quotes one word for the shell line of a hook command (POSIX single quotes, Windows double quotes).
/// The Windows form is untested.
pub fn quote(s: &str) -> String {
    let plain = !s.is_empty()
        && s.chars().all(|c| {
            c.is_ascii_alphanumeric()
                || matches!(
                    c,
                    '_' | '.' | '/' | ':' | '=' | '@' | '%' | '+' | '-' | '\\'
                )
        });
    if plain && !cfg!(windows) {
        s.to_string()
    } else if cfg!(windows) {
        format!("\"{}\"", s.replace('"', "\\\""))
    } else {
        format!("'{}'", s.replace('\'', "'\\''"))
    }
}

/// Looks the program up on this process's PATH.
pub fn find_program(name: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&path) {
        for candidate in [name.to_string(), format!("{name}.exe")] {
            let p = dir.join(&candidate);
            if p.is_file() {
                return Some(p);
            }
        }
    }
    None
}

pub struct ClaudeAdapter;

const HOOK_EVENTS: [&str; 6] = [
    "SessionStart",
    "UserPromptSubmit",
    "PreToolUse",
    "Notification",
    "Stop",
    "SessionEnd",
];

impl ClaudeAdapter {
    /// The settings file DCC hands to `claude --settings`. Hooks and the status line call back into dcc.
    pub fn settings_json(dcc_exe: &Path, data_dir: &Path) -> Value {
        let base = format!(
            "{} --data-dir {} agent",
            quote(&dcc_exe.to_string_lossy()),
            quote(&data_dir.to_string_lossy())
        );
        let hook = json!([{ "matcher": "*", "hooks": [{ "type": "command", "command": format!("{base} hook-log"), "timeout": 5 }] }]);
        let mut hooks = serde_json::Map::new();
        for event in HOOK_EVENTS {
            hooks.insert(event.to_string(), hook.clone());
        }
        json!({ "hooks": hooks, "statusLine": { "type": "command", "command": format!("{base} statusline-log") } })
    }
}

impl AgentAdapter for ClaudeAdapter {
    fn kind(&self) -> &'static str {
        "claude"
    }

    fn matches(&self, program: &str, argv: &[String]) -> bool {
        matches!(basename(program), "claude" | "claude-code")
            || argv.first().is_some_and(|a| basename(a) == "claude")
    }

    fn session_id_from_argv(&self, argv: &[String]) -> Option<String> {
        for (i, a) in argv.iter().enumerate() {
            if a == "--session-id" {
                return argv.get(i + 1).filter(|v| valid_session_id(v)).cloned();
            }
            if let Some(v) = a.strip_prefix("--session-id=") {
                return Some(v.to_string()).filter(|v| valid_session_id(v));
            }
        }
        None
    }

    fn launch_plan(&self, ctx: &LaunchContext, req: &LaunchRequest) -> Result<LaunchPlan> {
        let mut args = vec![
            "--session-id".to_string(),
            ctx.run_id.to_string(),
            "--settings".to_string(),
            ctx.settings_path.to_string_lossy().to_string(),
        ];
        if let Some(m) = &req.model {
            args.extend(["--model".to_string(), m.clone()]);
        }
        if let Some(p) = &req.permission_mode {
            args.extend(["--permission-mode".to_string(), p.clone()]);
        }
        args.extend(req.extra_args.iter().cloned());
        Ok(LaunchPlan {
            program: ctx.program_path.to_string(),
            args,
            settings: Some(Self::settings_json(ctx.dcc_exe, ctx.data_dir)),
        })
    }

    fn capabilities(&self) -> Capabilities {
        Capabilities {
            session_id: true,
            hooks: true,
            statusline: true,
            usage: true,
        }
    }
}

/// For every agent that is not Claude Code: Herdr's status and the folder, nothing else.
pub struct GenericAdapter;

impl AgentAdapter for GenericAdapter {
    fn kind(&self) -> &'static str {
        "other"
    }
    fn matches(&self, _program: &str, _argv: &[String]) -> bool {
        true
    }
    fn session_id_from_argv(&self, _argv: &[String]) -> Option<String> {
        None
    }
    fn launch_plan(&self, _ctx: &LaunchContext, _req: &LaunchRequest) -> Result<LaunchPlan> {
        Err(Error::usage("DCC can only start Claude Code agents so far"))
    }
    fn capabilities(&self) -> Capabilities {
        Capabilities {
            session_id: false,
            hooks: false,
            statusline: false,
            usage: false,
        }
    }
}

pub fn adapter_for(program: &str, argv: &[String]) -> Box<dyn AgentAdapter> {
    if ClaudeAdapter.matches(program, argv) {
        Box::new(ClaudeAdapter)
    } else {
        Box::new(GenericAdapter)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(v: &[&str]) -> Vec<String> {
        v.iter().map(|x| x.to_string()).collect()
    }

    #[test]
    fn claude_is_recognised_by_program_or_argv() {
        assert!(ClaudeAdapter.matches("claude-code", &[]));
        assert!(ClaudeAdapter.matches("zsh", &s(&["/home/u/.local/bin/claude", "--model", "x"])));
        assert!(!ClaudeAdapter.matches("zsh", &s(&["nvim"])));
        assert_eq!(adapter_for("nvim", &[]).kind(), "other");
        assert_eq!(adapter_for("x", &s(&["claude.exe"])).kind(), "claude");
    }

    #[test]
    fn session_id_from_argv_only_when_it_looks_like_one() {
        let a = ClaudeAdapter;
        assert_eq!(
            a.session_id_from_argv(&s(&["claude", "--session-id", "abcdef12-3456"]))
                .as_deref(),
            Some("abcdef12-3456")
        );
        assert_eq!(
            a.session_id_from_argv(&s(&["claude", "--session-id=abcdef12-3456"]))
                .as_deref(),
            Some("abcdef12-3456")
        );
        assert_eq!(
            a.session_id_from_argv(&s(&["claude", "--session-id", "../x"])),
            None
        );
        assert_eq!(
            a.session_id_from_argv(&s(&["claude", "--model", "haiku"])),
            None
        );
        assert_eq!(
            GenericAdapter.session_id_from_argv(&s(&["x", "--session-id", "abcdef12-3456"])),
            None
        );
    }

    #[test]
    fn the_launch_plan_has_the_id_the_settings_and_the_options() {
        let ctx = LaunchContext {
            program_path: "/bin/claude",
            run_id: "abcdef12-3456",
            settings_path: Path::new("/d/s.json"),
            dcc_exe: Path::new("/x/dcc"),
            data_dir: Path::new("/d"),
        };
        let plan = ClaudeAdapter
            .launch_plan(
                &ctx,
                &LaunchRequest {
                    model: Some("haiku".into()),
                    permission_mode: Some("default".into()),
                    extra_args: s(&["--bare"]),
                },
            )
            .unwrap();
        assert_eq!(plan.program, "/bin/claude");
        assert_eq!(
            plan.args,
            s(&[
                "--session-id",
                "abcdef12-3456",
                "--settings",
                "/d/s.json",
                "--model",
                "haiku",
                "--permission-mode",
                "default",
                "--bare"
            ])
        );
        let st = plan.settings.unwrap();
        for e in HOOK_EVENTS {
            assert!(
                st["hooks"][e][0]["hooks"][0]["command"]
                    .as_str()
                    .unwrap()
                    .ends_with("agent hook-log"),
                "{e}"
            );
        }
        assert!(
            st["statusLine"]["command"]
                .as_str()
                .unwrap()
                .contains("agent statusline-log")
        );
        assert!(
            GenericAdapter
                .launch_plan(&ctx, &LaunchRequest::default())
                .is_err()
        );
    }

    #[cfg(unix)]
    #[test]
    fn quoting_keeps_odd_paths_in_one_word() {
        assert_eq!(quote("/a/b-c.d"), "/a/b-c.d");
        assert_eq!(quote("/a b/c"), "'/a b/c'");
        assert_eq!(quote("it's"), "'it'\\''s'");
        assert_eq!(quote(""), "''");
    }

    #[test]
    fn capabilities() {
        assert!(ClaudeAdapter.capabilities().hooks && !GenericAdapter.capabilities().hooks);
    }
}
