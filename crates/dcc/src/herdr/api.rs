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

/// A terminal that carries a label (`hc run --label key=value`), live or finished.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LabeledTerminal {
    pub address: String,
    pub value: String,
    pub live: bool,
    pub exit_code: Option<i64>,
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
    /// Terminals that carry the label `key`, with the label's value.
    fn labeled(&self, key: &str) -> Result<Vec<LabeledTerminal>>;
    /// Herdr's agent status of a live terminal (working, blocked, idle, done), `None` when it has none.
    fn terminal_status(&self, terminal: &str) -> Result<Option<String>>;
    /// `hc run --bg --json --name <name> --cwd <cwd> --label k=v ... -- <argv>`. With the labels
    /// `herdr.space` and `herdr.tab` the terminal joins that tab, and unlike a terminal opened by
    /// `terminal.open` it keeps its exit code when the program ends (Phase 0 test 4).
    /// Returns the terminal address.
    fn run_labeled(
        &self,
        name: &str,
        cwd: &str,
        labels: &[(String, String)],
        argv: &[String],
    ) -> Result<String>;
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

/// The status text of a terminal row, either a plain string or an object with `state`.
pub fn status_of(t: &Value) -> Option<String> {
    match t.get("status") {
        Some(Value::String(x)) if !x.is_empty() => Some(x.clone()),
        Some(Value::Object(o)) => o.get("state").and_then(Value::as_str).map(str::to_string),
        _ => None,
    }
}

/// One `hc ls` row and the matching `hc get` document, as a labeled terminal when it carries `key`.
pub fn labeled_from(row: &Value, detail: &Value, key: &str) -> Option<LabeledTerminal> {
    let value = detail.get("labels")?.get(key)?.as_str()?.to_string();
    Some(LabeledTerminal {
        address: text(row, "address"),
        value,
        live: text(row, "state") == "live",
        exit_code: detail.get("exit_code").and_then(Value::as_i64),
    })
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
    fn labeled(&self, key: &str) -> Result<Vec<LabeledTerminal>> {
        // `hc ls` does not list labels, `hc get` does. Only terminals dcc named itself are asked.
        let all = Self::run_json("hc", &["ls", "--all", "--json"])?;
        let mut out = Vec::new();
        for row in all
            .get("terminals")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default()
        {
            if !text(&row, "name").starts_with("dcc-") {
                continue;
            }
            let address = text(&row, "address");
            let Ok(detail) = Self::run_json("hc", &["get", &address, "--json"]) else {
                continue;
            };
            out.extend(labeled_from(&row, &detail, key));
        }
        Ok(out)
    }
    fn terminal_status(&self, terminal: &str) -> Result<Option<String>> {
        let all = Self::run_json("hc", &["ls", "--json"])?;
        let t = all
            .get("terminals")
            .and_then(Value::as_array)
            .and_then(|a| a.iter().find(|t| text(t, "address") == terminal))
            .ok_or_else(|| Error::not_found(format!("no live terminal {terminal}")))?;
        Ok(status_of(t))
    }
    fn run_labeled(
        &self,
        name: &str,
        cwd: &str,
        labels: &[(String, String)],
        argv: &[String],
    ) -> Result<String> {
        let mut args: Vec<String> = vec![
            "run".into(),
            "--bg".into(),
            "--json".into(),
            "--name".into(),
            name.into(),
            "--cwd".into(),
            cwd.into(),
        ];
        for (k, v) in labels {
            args.push("--label".into());
            args.push(format!("{k}={v}"));
        }
        args.push("--".into());
        args.extend(argv.iter().cloned());
        let refs: Vec<&str> = args.iter().map(String::as_str).collect();
        let v = Self::run_json("hc", &refs)?;
        v.get("address")
            .and_then(Value::as_str)
            .map(str::to_string)
            .ok_or_else(|| Error::external("hc run answered without an address"))
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
    fn labels_come_from_the_detail_document_and_status_from_either_shape() {
        let live = json!({"address":"t1","state":"live"});
        let gone = json!({"address":"t2","state":"finished"});
        let l = labeled_from(
            &live,
            &json!({"labels":{"dcc.forward":"a/b"}}),
            "dcc.forward",
        )
        .unwrap();
        assert!(l.live && l.value == "a/b" && l.exit_code.is_none());
        let g = labeled_from(
            &gone,
            &json!({"labels":{"dcc.forward":"a/c"},"exit_code":255}),
            "dcc.forward",
        )
        .unwrap();
        assert!(!g.live && g.exit_code == Some(255));
        assert!(labeled_from(&live, &json!({"labels":{"other":"x"}}), "dcc.forward").is_none());
        assert!(labeled_from(&live, &json!({}), "dcc.forward").is_none());
        assert_eq!(
            status_of(&json!({"status":"blocked"})).as_deref(),
            Some("blocked")
        );
        assert_eq!(
            status_of(&json!({"status":{"state":"idle"}})).as_deref(),
            Some("idle")
        );
        assert_eq!(status_of(&json!({})), None);
    }

    #[test]
    fn parses_hc_ls() {
        let v = json!({"terminals":[{"address":"t1","agent":"zsh","cwd":"/a","state":"live"},{"address":"t2","agent":"nvim","state":"finished"}]});
        let t = parse_terminals(&v);
        assert!(t[0].live && !t[1].live);
        assert_eq!(cwd_map(&t).len(), 1);
    }
}
