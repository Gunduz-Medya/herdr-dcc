//! The operations behind the `dcc ssh` commands.

use std::path::Path;
use std::process::Command;

use serde_json::{Value, json};

use super::askpass::AskpassServer;
use super::model::{Forward, Profile, SavedCommand};
use super::scan;
use crate::error::{Error, Result};
use crate::herdr::api::Herdr;
use crate::herdr::tabs::{TabSpec, run_in_new_tab};
use crate::secret::SecretBackend;
use crate::store::Store;

pub const SPACE: &str = "ssh";
pub const FORWARD_LABEL: &str = "dcc.forward";
pub const SESSION_LABEL: &str = "dcc.ssh";

fn home() -> String {
    std::env::var("HOME").unwrap_or_else(|_| "/".into())
}

fn ssh_program() -> String {
    std::env::var("DCC_SSH_PROGRAM").unwrap_or_else(|_| "ssh".into())
}

fn runner_argv(exe: &Path, data_dir: &Path, profile: &str, forward: Option<&str>) -> Vec<String> {
    let mut a: Vec<String> = vec![
        exe.to_string_lossy().into(),
        "--data-dir".into(),
        data_dir.to_string_lossy().into(),
        "ssh".into(),
        "run".into(),
        profile.into(),
    ];
    if let Some(f) = forward {
        a.extend(["--forward".into(), f.into()]);
    }
    a
}

fn need_secret(backend: &dyn SecretBackend, p: &Profile) -> Result<()> {
    if p.auth.needs_secret() && !backend.exists(&p.secret_name())? {
        return Err(Error::conflict(format!(
            "no {} stored for profile {}, run `dcc ssh profile secret {}` in a terminal",
            p.auth.as_str(),
            p.name,
            p.name
        )));
    }
    Ok(())
}

/// Opens a tab with an ssh session.
pub fn connect(
    store: &Store,
    h: &dyn Herdr,
    backend: &dyn SecretBackend,
    profile: &str,
    exe: &Path,
    data_dir: &Path,
) -> Result<Value> {
    let p = store.ssh_profile(profile)?;
    p.session_args()?;
    need_secret(backend, &p)?;
    let tab = format!("ssh {}", p.name);
    let argv = runner_argv(exe, data_dir, &p.name, None);
    let labels = vec![(SESSION_LABEL.to_string(), p.name.clone())];
    let placed = run_in_new_tab(
        h,
        &TabSpec {
            space: SPACE,
            tab_name: &tab,
            cwd: &home(),
            run_name: &format!("dcc-ssh-{}", p.name),
            extra_labels: &labels,
            argv: &argv,
            what: "ssh",
        },
    )?;
    Ok(
        json!({ "profile": p.name, "terminal": placed.terminal, "space": SPACE, "tab": tab, "warnings": placed.warnings }),
    )
}

/// Runs ssh in this process's terminal and returns its exit code. Started by the terminals that
/// `connect` and `forward_start` open.
pub fn run_session(
    store: &Store,
    backend: &dyn SecretBackend,
    profile: &str,
    forward: Option<&str>,
    exe: &Path,
) -> Result<i32> {
    let p = store.ssh_profile(profile)?;
    let args = match forward {
        Some(name) => p.forward_args(&store.forward(profile, name)?)?,
        None => p.session_args()?,
    };
    let server = if p.auth.needs_secret() {
        let secret = backend
            .reveal(&p.secret_name())?
            .ok_or_else(|| Error::conflict("no secret stored for this profile"))?;
        Some(AskpassServer::start(exe, p.auth, secret)?)
    } else {
        None
    };
    let mut cmd = Command::new(ssh_program());
    cmd.args(&args);
    if let Some(s) = &server {
        cmd.envs(s.env());
    }
    let status = cmd
        .status()
        .map_err(|e| Error::external(format!("cannot run ssh: {e}")))?;
    drop(server);
    Ok(status.code().unwrap_or(255))
}

fn forward_key(profile: &str, name: &str) -> String {
    format!("{profile}/{name}")
}

pub fn forward_start(
    store: &Store,
    h: &dyn Herdr,
    backend: &dyn SecretBackend,
    profile: &str,
    name: &str,
    exe: &Path,
    data_dir: &Path,
) -> Result<Value> {
    let p = store.ssh_profile(profile)?;
    let f = store.forward(profile, name)?;
    p.forward_args(&f)?;
    need_secret(backend, &p)?;
    let key = forward_key(profile, name);
    if h.labeled(FORWARD_LABEL)?
        .iter()
        .any(|t| t.value == key && t.live)
    {
        return Err(Error::conflict(format!("forward {key} is already running")));
    }
    let argv = runner_argv(exe, data_dir, profile, Some(name));
    let labels = vec![(FORWARD_LABEL.to_string(), key.clone())];
    let terminal = h.run_labeled(
        &format!("dcc-fwd-{profile}-{name}"),
        &home(),
        &labels,
        &argv,
    )?;
    Ok(json!({ "forward": key, "terminal": terminal }))
}

pub fn forward_stop(h: &dyn Herdr, profile: &str, name: &str) -> Result<Value> {
    let key = forward_key(profile, name);
    let live: Vec<_> = h
        .labeled(FORWARD_LABEL)?
        .into_iter()
        .filter(|t| t.value == key && t.live)
        .collect();
    if live.is_empty() {
        return Err(Error::not_found(format!("forward {key} is not running")));
    }
    for t in &live {
        h.kill(&t.address)?;
    }
    Ok(json!({ "forward": key, "stopped": live.len() }))
}

/// Every stored forward with its state from the core: running, failed (ssh ended with an error),
/// stopped (ended or killed) or idle (never started).
pub fn forwards_status(store: &Store, h: &dyn Herdr) -> Result<Value> {
    let terms = h.labeled(FORWARD_LABEL)?;
    let rows: Vec<Value> = store
        .forwards()?
        .iter()
        .map(|f| {
            let key = forward_key(&f.profile, &f.name);
            let mine: Vec<_> = terms.iter().filter(|t| t.value == key).collect();
            let (state, terminal, exit) = if let Some(t) = mine.iter().find(|t| t.live) {
                ("running", Some(t.address.clone()), None)
            } else if let Some(t) = mine.last() {
                let failed = t.exit_code.is_some_and(|c| c != 0 && c != 129 && c != 130 && c != 143);
                (if failed { "failed" } else { "stopped" }, Some(t.address.clone()), t.exit_code)
            } else {
                ("idle", None, None)
            };
            json!({
                "profile": f.profile, "name": f.name, "kind": f.kind.as_str(),
                "bind": format!("{}:{}", f.bind_host, f.bind_port),
                "target": if f.target_host.is_empty() { Value::Null } else { json!(format!("{}:{}", f.target_host, f.target_port)) },
                "state": state, "terminal": terminal, "exit_code": exit,
            })
        })
        .collect();
    Ok(json!({ "forwards": rows }))
}

/// A command is one line of text. It is checked for secrets before it is stored.
pub fn add_command(
    store: &Store,
    name: &str,
    command: &str,
    profile: Option<&str>,
) -> Result<Value> {
    check_command_text(command)?;
    store.add_saved_command(&SavedCommand {
        name: name.into(),
        command: command.into(),
        profile: profile.map(str::to_string),
    })?;
    Ok(json!({ "name": name, "saved": true }))
}

fn check_command_text(command: &str) -> Result<()> {
    if command.trim().is_empty() {
        return Err(Error::usage("the command is empty"));
    }
    if command.chars().any(|c| c.is_control()) {
        return Err(Error::usage(
            "a saved command is one line, without control characters",
        ));
    }
    if let Some(kind) = scan::find_secret(command) {
        return Err(Error::usage(format!(
            "the command looks like it holds a secret ({kind}), keep secrets out of saved commands and use an environment variable or the Keychain"
        )));
    }
    Ok(())
}

/// Types the saved command into a live terminal. A terminal that waits on a prompt (Herdr status
/// blocked) is left alone unless `force` is set.
pub fn run_command(
    store: &Store,
    h: &dyn Herdr,
    name: &str,
    terminal: &str,
    force: bool,
) -> Result<Value> {
    let c = store.saved_command(name)?;
    check_command_text(&c.command)?;
    let status = h.terminal_status(terminal)?;
    if status.as_deref() == Some("blocked") && !force {
        return Err(Error::conflict(
            "that terminal is waiting on a prompt, nothing was typed (use --force to type anyway)",
        ));
    }
    h.send(terminal, &c.command)?;
    Ok(
        json!({ "command": name, "terminal": terminal, "forced": force && status.as_deref() == Some("blocked") }),
    )
}

pub fn forward_row(f: &Forward) -> Value {
    json!({
        "profile": f.profile, "name": f.name, "kind": f.kind.as_str(),
        "bind": format!("{}:{}", f.bind_host, f.bind_port),
        "target": if f.target_host.is_empty() { Value::Null } else { json!(format!("{}:{}", f.target_host, f.target_port)) },
    })
}

#[cfg(test)]
mod tests {
    use super::super::model::{Auth, ForwardKind, tests::profile};
    use super::*;
    use crate::herdr::fake::FakeHerdr;
    use crate::secret::MemoryBackend;
    use std::path::PathBuf;

    fn setup() -> (Store, FakeHerdr, MemoryBackend) {
        let s = Store::open_in_memory().unwrap();
        let mut p = profile();
        p.auth = Auth::Password;
        s.add_ssh_profile(&p).unwrap();
        (s, FakeHerdr::new(), MemoryBackend::new())
    }

    fn forward() -> Forward {
        Forward {
            id: 0,
            profile: "prod".into(),
            name: "web".into(),
            kind: ForwardKind::Local,
            bind_host: "127.0.0.1".into(),
            bind_port: 8080,
            target_host: "10.0.0.1".into(),
            target_port: 80,
        }
    }

    #[test]
    fn connect_needs_the_secret_then_opens_a_tab_that_runs_the_runner() {
        let (s, h, b) = setup();
        let exe = PathBuf::from("/x/dcc");
        let data = PathBuf::from("/x/data");
        let e = connect(&s, &h, &b, "prod", &exe, &data).unwrap_err();
        assert_eq!(e.kind, "conflict");
        assert!(h.launched().is_empty());
        b.set("ssh.prod", "pw").unwrap();
        let r = connect(&s, &h, &b, "prod", &exe, &data).unwrap();
        assert_eq!(r["tab"], "ssh prod");
        let l = h.launched();
        assert_eq!(l.len(), 1);
        assert_eq!(l[0].1[3..], ["ssh", "run", "prod"]);
        assert!(l[0].1.iter().all(|a| !a.contains("pw")));
        assert!(l[0].2.iter().any(|(k, v)| k == "dcc.ssh" && v == "prod"));
    }

    #[test]
    fn a_forward_starts_once_shows_as_running_and_stops() {
        let (s, h, b) = setup();
        b.set("ssh.prod", "pw").unwrap();
        s.add_forward(&forward()).unwrap();
        let exe = PathBuf::from("/x/dcc");
        let data = PathBuf::from("/x/data");
        let st = forwards_status(&s, &h).unwrap();
        assert_eq!(st["forwards"][0]["state"], "idle");
        let r = forward_start(&s, &h, &b, "prod", "web", &exe, &data).unwrap();
        let term = r["terminal"].as_str().unwrap().to_string();
        assert_eq!(
            h.launched()[0].1[3..],
            ["ssh", "run", "prod", "--forward", "web"]
        );
        assert_eq!(
            forward_start(&s, &h, &b, "prod", "web", &exe, &data)
                .unwrap_err()
                .kind,
            "conflict"
        );
        let st = forwards_status(&s, &h).unwrap();
        assert_eq!(st["forwards"][0]["state"], "running");
        assert_eq!(st["forwards"][0]["terminal"], term.as_str());
        forward_stop(&h, "prod", "web").unwrap();
        assert_eq!(h.killed(), vec![term]);
        assert_eq!(
            forwards_status(&s, &h).unwrap()["forwards"][0]["state"],
            "stopped"
        );
        assert_eq!(
            forward_stop(&h, "prod", "web").unwrap_err().kind,
            "not_found"
        );
    }

    #[test]
    fn saved_commands_reject_secrets_and_multiple_lines() {
        let (s, _h, _b) = setup();
        assert!(add_command(&s, "uptime", "uptime", None).is_ok());
        let e = add_command(&s, "leak", "export DB_PASSWORD=hunter2", None).unwrap_err();
        assert_eq!(e.kind, "usage");
        assert!(!e.message.contains("hunter2"));
        assert!(add_command(&s, "two", "ls\nrm -rf x", None).is_err());
        assert!(add_command(&s, "empty", "  ", None).is_err());
        assert_eq!(s.saved_commands().unwrap().len(), 1);
    }

    #[test]
    fn a_command_is_typed_into_the_chosen_terminal_unless_it_waits_on_a_prompt() {
        let (s, h, _b) = setup();
        add_command(&s, "uptime", "uptime", None).unwrap();
        let term = h.run_labeled("n", "/", &[], &["zsh".to_string()]).unwrap();
        let r = run_command(&s, &h, "uptime", &term, false).unwrap();
        assert_eq!(r["forced"], false);
        assert_eq!(h.typed(), vec![(term.clone(), "uptime".to_string())]);
        h.set_status(&term, "blocked");
        let e = run_command(&s, &h, "uptime", &term, false).unwrap_err();
        assert_eq!(e.kind, "conflict");
        assert_eq!(h.typed().len(), 1);
        assert_eq!(
            run_command(&s, &h, "uptime", &term, true).unwrap()["forced"],
            true
        );
        assert_eq!(h.typed().len(), 2);
        assert_eq!(
            run_command(&s, &h, "uptime", "t99", false)
                .unwrap_err()
                .kind,
            "not_found"
        );
        assert_eq!(
            run_command(&s, &h, "nope", &term, false).unwrap_err().kind,
            "not_found"
        );
    }
}
