//! The calls dcc makes to Herdr, behind a trait so the restore logic can be tested without a
//! window. `CliHerdr` runs the real programs and inherits the environment, so the isolation
//! variables (`HERDR_APP_HOME`, `HC_SESSION`) decide which window and core it talks to.

use std::collections::HashMap;
use std::process::Command;

use serde_json::Value;

use super::isolation;
use crate::error::{Error, Result};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TermRef {
    pub terminal: String,
    pub pane: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TabInfo {
    pub id: String,
    pub name: String,
    /// In tree order (first pane, then the panes split from it, and so on).
    pub terminals: Vec<TermRef>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpaceInfo {
    pub id: String,
    pub name: String,
    pub folder: String,
    pub tabs: Vec<TabInfo>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TerminalInfo {
    pub address: String,
    pub cwd: Option<String>,
    /// Name of the program in front, for example `zsh`, `nvim`, `lazygit`.
    pub program: String,
    pub live: bool,
}

pub trait Herdr {
    fn spaces(&self) -> Result<Vec<SpaceInfo>>;
    /// Live terminals of this core.
    fn terminals(&self) -> Result<Vec<TerminalInfo>>;
    /// `herdr-app do <name> '<json object>'`, returns the JSON it prints.
    fn app_do(&self, name: &str, args: Value) -> Result<Value>;
    /// `hc send <terminal> <text>`: pastes the text and presses Enter.
    fn send(&self, terminal: &str, text: &str) -> Result<()>;
    fn kill(&self, terminal: &str) -> Result<()>;
}

pub fn is_plain_shell(program: &str) -> bool {
    let base = program
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or(program)
        .trim_end_matches(".exe");
    matches!(
        base,
        "zsh" | "bash" | "fish" | "sh" | "dash" | "ksh" | "nu" | "pwsh" | "powershell" | "cmd"
    )
}

fn text(v: &Value, key: &str) -> String {
    v.get(key)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string()
}

pub fn parse_spaces(v: &Value) -> Vec<SpaceInfo> {
    let mut out = Vec::new();
    for s in v
        .get("spaces")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
    {
        let tabs = s
            .get("tabs")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default()
            .iter()
            .map(|t| TabInfo {
                id: text(t, "id"),
                name: text(t, "name"),
                terminals: t
                    .get("terminals")
                    .and_then(Value::as_array)
                    .cloned()
                    .unwrap_or_default()
                    .iter()
                    .map(|x| TermRef {
                        terminal: text(x, "terminal"),
                        pane: text(x, "pane"),
                    })
                    .collect(),
            })
            .collect();
        out.push(SpaceInfo {
            id: text(&s, "id"),
            name: text(&s, "name"),
            folder: text(&s, "folder"),
            tabs,
        });
    }
    out
}

pub fn parse_terminals(v: &Value) -> Vec<TerminalInfo> {
    v.get("terminals")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
        .iter()
        .map(|t| TerminalInfo {
            address: text(t, "address"),
            cwd: t.get("cwd").and_then(Value::as_str).map(str::to_string),
            program: text(t, "agent"),
            live: text(t, "state") == "live",
        })
        .collect()
}

/// Cwd per terminal address, for the snapshot.
pub fn cwd_map(terminals: &[TerminalInfo]) -> HashMap<String, String> {
    terminals
        .iter()
        .filter_map(|t| Some((t.address.clone(), t.cwd.clone()?)))
        .collect()
}

pub struct CliHerdr;

impl CliHerdr {
    /// Refuses to run when isolation is required (`DCC_REQUIRE_ISOLATION=1`) and not set up.
    pub fn new() -> Result<Self> {
        isolation::require()?;
        Ok(Self)
    }

    fn run(program: &str, args: &[&str]) -> Result<Vec<u8>> {
        let out = Command::new(program)
            .args(args)
            .output()
            .map_err(|e| Error::external(format!("cannot run {program}: {e}")))?;
        if !out.status.success() {
            return Err(Error::external(format!(
                "{program} {}: {}",
                args.first().unwrap_or(&""),
                String::from_utf8_lossy(&out.stderr).trim()
            )));
        }
        Ok(out.stdout)
    }

    fn run_json(program: &str, args: &[&str]) -> Result<Value> {
        let bytes = Self::run(program, args)?;
        Ok(serde_json::from_slice(&bytes)?)
    }
}

impl Herdr for CliHerdr {
    fn spaces(&self) -> Result<Vec<SpaceInfo>> {
        Ok(parse_spaces(&Self::run_json("herdr-app", &["ls"])?))
    }
    fn terminals(&self) -> Result<Vec<TerminalInfo>> {
        Ok(parse_terminals(&Self::run_json("hc", &["ls", "--json"])?))
    }
    fn app_do(&self, name: &str, args: Value) -> Result<Value> {
        Self::run_json("herdr-app", &["do", name, &args.to_string()])
    }
    fn send(&self, terminal: &str, text: &str) -> Result<()> {
        Self::run("hc", &["send", terminal, text]).map(|_| ())
    }
    fn kill(&self, terminal: &str) -> Result<()> {
        Self::run("hc", &["kill", terminal]).map(|_| ())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn shells_are_recognised() {
        for s in ["zsh", "/bin/zsh", "bash", "pwsh.exe"] {
            assert!(is_plain_shell(s), "{s}");
        }
        for s in ["nvim", "lazygit", "claude", ""] {
            assert!(!is_plain_shell(s), "{s}");
        }
    }

    #[test]
    fn parses_herdr_app_ls() {
        let v = json!({"spaces":[{"id":"s1","name":"code","folder":"/f","tabs":[{"id":"t1","name":"code","terminals":[{"terminal":"t7","pane":"p1"},{"terminal":"t9","pane":"p2"}]}]}]});
        let s = parse_spaces(&v);
        assert_eq!(s.len(), 1);
        assert_eq!(
            s[0].tabs[0].terminals[1],
            TermRef {
                terminal: "t9".into(),
                pane: "p2".into()
            }
        );
    }

    #[test]
    fn parses_hc_ls() {
        let v = json!({"terminals":[{"address":"t1","agent":"zsh","cwd":"/a","state":"live"},{"address":"t2","agent":"nvim","state":"finished"}]});
        let t = parse_terminals(&v);
        assert!(t[0].live && !t[1].live);
        assert_eq!(cwd_map(&t).len(), 1);
    }
}
