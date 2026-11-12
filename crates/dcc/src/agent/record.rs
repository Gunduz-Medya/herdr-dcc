//! The snapshot. One call reads the core, updates the runs in SQLite and returns the rows the
//! panel draws: the runs that are open and the ones that ended recently, with their last state.
//!
//! Where facts come from:
//! - status, exit code, reason, folder, argv: the core (`hc ls --all`, `hc get`)
//! - space and tab names: `herdr-app ls`
//! - branch, worktree and change counts: git, live for open runs, kept for closed ones
//! - session ids and the last event: the hooks of the run (`DCC_RUN_ID`), or argv for agents
//!   DCC did not start
//!
//! Without `write` everything runs inside a transaction that is rolled back, so a read only call
//! changes nothing.

use std::collections::{HashMap, HashSet};
use std::process::Command;

use serde_json::{Value, json};

use super::adapter::adapter_for;
use super::db::Run;
use super::state::{DccState, StateInput, map_state};
use crate::error::{Error, Result};
use crate::git;
use crate::herdr::api::is_plain_shell;
use crate::store::Store;

#[derive(Debug, Clone, Default)]
pub struct CoreTerminal {
    pub address: String,
    pub live: bool,
    pub program: String,
    pub cwd: Option<String>,
    /// Herdr's agent status: working, blocked, idle, done, error.
    pub status: Option<String>,
    pub created_ms: Option<i64>,
    pub finished_ms: Option<i64>,
    pub exit_code: Option<i64>,
    pub reason: Option<String>,
    pub signal: Option<i64>,
    /// Argv of the program in front.
    pub argv: Vec<String>,
    /// The `dcc.run` label of a terminal DCC started.
    pub label_run: Option<String>,
    pub space: Option<String>,
    pub tab: Option<String>,
    pub pane: Option<String>,
}

pub trait AgentSource {
    fn terminals(&self) -> Result<Vec<CoreTerminal>>;
}

#[derive(Debug, Clone)]
pub struct RecordOptions {
    pub write: bool,
    pub now_ms: i64,
    pub closed_limit: i64,
    pub herdr_session: Option<String>,
}

/// An agent the panel should show: it reports a status, DCC started it, or it runs a known agent.
fn is_agent(t: &CoreTerminal) -> bool {
    t.live
        && (t.status.is_some()
            || t.label_run.is_some()
            || adapter_for(&t.program, &t.argv).kind() != "other")
}

fn terminal_matches_run(t: &CoreTerminal, run: &Run) -> bool {
    match (t.created_ms, run.terminal_created_ms) {
        (Some(a), Some(b)) => a == b,
        _ => true,
    }
}

struct Closing<'a> {
    state: DccState,
    reason: &'a str,
    exit_code: Option<i64>,
}

fn close(store: &Store, run: &mut Run, c: Closing, now: i64, finished: Option<i64>) -> Result<()> {
    let ended = finished.unwrap_or(now);
    run.ended_ms = Some(ended);
    if run.last_state != c.state.as_str() {
        run.last_state_ms = ended;
    }
    run.last_state = c.state.as_str().to_string();
    run.end_reason = Some(c.reason.to_string());
    run.exit_code = c.exit_code;
    run.herdr_status = None;
    store.update_run(run)?;
    store.end_open_sessions(&run.id, None, ended, "run_ended")?;
    Ok(())
}

fn refresh_git(run: &mut Run, cwd: Option<&str>) {
    let Some(cwd) = cwd else { return };
    run.cwd = Some(cwd.to_string());
    if let Some(g) = git::info(cwd) {
        run.repo_root = Some(g.repo_root.clone());
        run.branch = g.branch.clone();
        run.worktree_kind = Some(g.worktree_kind().to_string());
        run.changed_files = g.changed.map(|n| n as i64);
        run.staged = g.staged.map(|n| n as i64);
        run.unstaged = g.unstaged.map(|n| n as i64);
        run.untracked = g.untracked.map(|n| n as i64);
    }
}

fn update_live(
    store: &Store,
    run: &mut Run,
    t: &CoreTerminal,
    reliable: bool,
    opts: &RecordOptions,
) -> Result<()> {
    run.terminal_created_ms = run.terminal_created_ms.or(t.created_ms);
    run.pane_id = t.pane.clone().or(run.pane_id.take());
    run.space_name = t.space.clone().or(run.space_name.take());
    run.tab_name = t.tab.clone().or(run.tab_name.take());
    if opts.herdr_session.is_some() {
        run.herdr_session = opts.herdr_session.clone();
    }
    refresh_git(run, t.cwd.as_deref());

    // The agent left and a plain shell is back in front (an agent started with `terminal.open`
    // lives inside a shell, so the terminal itself never ends).
    if run.herdr_status.is_some() && t.status.is_none() && is_plain_shell(&t.program) {
        run.herdr_status = None;
        return close(
            store,
            run,
            Closing {
                state: DccState::Completed,
                reason: "agent_exited",
                exit_code: None,
            },
            opts.now_ms,
            None,
        );
    }
    run.herdr_status = t.status.clone();
    let last = store.last_event(&run.id)?;
    let (state, _) = map_state(&StateInput {
        live: true,
        status: t.status.as_deref(),
        status_unreliable: !reliable,
        claude_foreground: adapter_for(&t.program, &t.argv).kind() == "claude",
        last_hook: last.as_ref().map(|e| e.kind.as_str()),
        ..Default::default()
    });
    if run.last_state != state.as_str() {
        run.last_state = state.as_str().to_string();
        run.last_state_ms = opts.now_ms;
    }
    store.update_run(run)
}

fn new_external_run(
    store: &Store,
    t: &CoreTerminal,
    reliable: bool,
    opts: &RecordOptions,
) -> Result<()> {
    let adapter = adapter_for(&t.program, &t.argv);
    let started = t.created_ms.unwrap_or(opts.now_ms);
    let mut run = Run {
        id: format!("ext-{}-{}", t.address, started),
        agent_kind: adapter.kind().to_string(),
        launch_mode: "external".to_string(),
        started_ms: started,
        last_state: DccState::Unknown.as_str().to_string(),
        last_state_ms: opts.now_ms,
        terminal_addr: Some(t.address.clone()),
        terminal_created_ms: t.created_ms,
        ..Default::default()
    };
    store.insert_run(&run)?;
    if let Some(sid) = adapter.session_id_from_argv(&t.argv) {
        run.launch_session_id = Some(sid.clone());
        store.insert_session(&run.id, &sid, "argv", started)?;
    }
    update_live(store, &mut run, t, reliable, opts)
}

fn note_code(run: &Run, state: &str) -> String {
    if run.ended_ms.is_some() {
        return match run.end_reason.as_deref() {
            Some(r) => r.to_string(),
            None => "ended".to_string(),
        };
    }
    match (state, run.herdr_status.as_deref()) {
        ("Running", _) => "working",
        ("Waiting", Some("blocked")) => "needs_permission",
        ("Waiting", _) => "ready_for_input",
        ("Completed", _) => "turn_done",
        ("Unknown", None) if run.agent_kind == "claude" => "no_status_claude",
        ("Unknown", _) => "no_status",
        _ => "other",
    }
    .to_string()
}

fn row(store: &Store, run: &Run, shared: &HashSet<String>) -> Result<Value> {
    let sessions = store.sessions_of(&run.id)?;
    let latest = sessions.last();
    let last = store.last_event(&run.id)?;
    let git_ok = run.repo_root.is_some();
    Ok(json!({
        "run_id": run.id, "kind": run.agent_kind, "launch_mode": run.launch_mode,
        "dcc_state": run.last_state, "state_since_ms": run.last_state_ms, "note_code": note_code(run, &run.last_state),
        "herdr_status": run.herdr_status, "started_ms": run.started_ms, "ended_ms": run.ended_ms,
        "end_reason": run.end_reason, "exit_code": run.exit_code,
        "session_id": latest.map(|s| s.id.clone()), "session_id_source": latest.map(|s| s.id_source.clone()).unwrap_or_else(|| "none".into()),
        "session_count": sessions.len(),
        "last_event": last.as_ref().map(|e| e.kind.clone()), "last_event_ms": last.as_ref().map(|e| e.ts_ms),
        "cwd": run.cwd, "space": run.space_name, "tab": run.tab_name, "terminal": run.terminal_addr,
        "shared_folder": run.ended_ms.is_none() && run.cwd.as_ref().is_some_and(|c| shared.contains(c)),
        "git": if git_ok { json!({ "ok": true, "repo_root": run.repo_root, "branch": run.branch, "worktree_kind": run.worktree_kind,
                    "changed_files": run.changed_files, "staged": run.staged, "unstaged": run.unstaged, "untracked": run.untracked }) } else { json!({ "ok": false }) },
    }))
}

/// Updates the records from the core and returns the document the panel draws.
pub fn snapshot(store: &Store, src: &dyn AgentSource, opts: &RecordOptions) -> Result<Value> {
    let tx = store.conn().unchecked_transaction()?;
    let reliable = store.state("status_reliable")?.as_deref() != Some("false");
    let terms = src.terminals()?;
    let by_addr: HashMap<&str, &CoreTerminal> =
        terms.iter().map(|t| (t.address.as_str(), t)).collect();

    let mut linked: HashSet<String> = HashSet::new();
    for mut run in store.open_runs()? {
        let Some(addr) = run.terminal_addr.clone() else {
            // started by DCC, the terminal is not recorded yet: attach it through its label
            if let Some(t) = terms
                .iter()
                .find(|t| t.label_run.as_deref() == Some(run.id.as_str()))
            {
                run.terminal_addr = Some(t.address.clone());
                linked.insert(t.address.clone());
                update_live(store, &mut run, t, reliable, opts)?;
            } else if opts.now_ms - run.started_ms > 60_000 {
                close(
                    store,
                    &mut run,
                    Closing {
                        state: DccState::Stopped,
                        reason: "lost",
                        exit_code: None,
                    },
                    opts.now_ms,
                    None,
                )?;
            }
            continue;
        };
        match by_addr
            .get(addr.as_str())
            .filter(|t| terminal_matches_run(t, &run))
        {
            None => close(
                store,
                &mut run,
                Closing {
                    state: DccState::Stopped,
                    reason: "lost",
                    exit_code: None,
                },
                opts.now_ms,
                None,
            )?,
            Some(t) if !t.live => {
                let last = store.last_event(&run.id)?;
                let (state, code) = map_state(&StateInput {
                    live: false,
                    exit_code: t.exit_code,
                    reason: t.reason.as_deref(),
                    signal: t.signal,
                    last_hook: last.as_ref().map(|e| e.kind.as_str()),
                    ..Default::default()
                });
                close(
                    store,
                    &mut run,
                    Closing {
                        state,
                        reason: code,
                        exit_code: t.exit_code,
                    },
                    opts.now_ms,
                    t.finished_ms,
                )?;
            }
            Some(t) => {
                linked.insert(addr);
                update_live(store, &mut run, t, reliable, opts)?;
            }
        }
    }
    for t in terms
        .iter()
        .filter(|t| is_agent(t) && !linked.contains(&t.address))
    {
        // an ended run keeps its terminal address, so only open ones count here
        let open_here = store.open_runs()?.iter().any(|r| {
            r.terminal_addr.as_deref() == Some(t.address.as_str()) && terminal_matches_run(t, r)
        });
        if !open_here {
            let closed_same = store.recent_closed_runs(500)?.iter().any(|r| {
                r.terminal_addr.as_deref() == Some(t.address.as_str())
                    && r.terminal_created_ms.is_some()
                    && r.terminal_created_ms == t.created_ms
            });
            if !closed_same {
                new_external_run(store, t, reliable, opts)?;
            }
        }
    }

    let open = store.open_runs()?;
    let mut folders: HashMap<&str, usize> = HashMap::new();
    for r in &open {
        if let Some(c) = r.cwd.as_deref() {
            *folders.entry(c).or_default() += 1;
        }
    }
    let shared: HashSet<String> = folders
        .iter()
        .filter(|(_, n)| **n > 1)
        .map(|(c, _)| c.to_string())
        .collect();
    let mut rows = Vec::new();
    for r in &open {
        rows.push(row(store, r, &shared)?);
    }
    for r in store.recent_closed_runs(opts.closed_limit)? {
        rows.push(row(store, &r, &shared)?);
    }
    let mut warnings = Vec::new();
    if !reliable {
        warnings.push(
            "the status channel failed its contract test, live states are Unknown".to_string(),
        );
    }
    for c in &shared {
        warnings.push(format!(
            "more than one agent runs in {c}, changed files cannot be told apart"
        ));
    }
    warnings.sort();
    if opts.write {
        tx.commit()?;
    }
    Ok(
        json!({ "generated_ms": opts.now_ms, "status_reliable": reliable, "runs": rows, "warnings": warnings }),
    )
}

// the real source

fn run_json(program: &str, args: &[&str]) -> Result<Value> {
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
    Ok(serde_json::from_slice(&out.stdout)?)
}

fn s(v: &Value, k: &str) -> Option<String> {
    v.get(k)
        .and_then(Value::as_str)
        .filter(|x| !x.is_empty())
        .map(str::to_string)
}

fn status_state(v: &Value) -> Option<String> {
    match v.get("status") {
        Some(Value::String(x)) if !x.is_empty() => Some(x.clone()),
        Some(Value::Object(o)) => o.get("state").and_then(Value::as_str).map(str::to_string),
        _ => None,
    }
}

pub struct CliSource;

impl AgentSource for CliSource {
    fn terminals(&self) -> Result<Vec<CoreTerminal>> {
        let all = run_json("hc", &["ls", "--all", "--json"])?;
        let mut places: HashMap<String, (String, String, String)> = HashMap::new();
        for sp in crate::herdr::api::parse_spaces(&run_json("herdr-app", &["ls"])?) {
            for tab in &sp.tabs {
                for t in &tab.terminals {
                    places.insert(
                        t.terminal.clone(),
                        (sp.name.clone(), tab.name.clone(), t.pane.clone()),
                    );
                }
            }
        }
        let mut out = Vec::new();
        for t in all
            .get("terminals")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default()
        {
            let address = s(&t, "address").unwrap_or_default();
            let live = s(&t, "state").as_deref() == Some("live");
            let mut term = CoreTerminal {
                address: address.clone(),
                live,
                program: s(&t, "agent").unwrap_or_default(),
                cwd: s(&t, "cwd"),
                status: status_state(&t),
                created_ms: t.get("created_ms").and_then(Value::as_i64),
                finished_ms: t.get("finished_ms").and_then(Value::as_i64),
                exit_code: t.get("exit_code").and_then(Value::as_i64),
                reason: s(&t, "reason"),
                signal: t.get("signal").and_then(Value::as_i64),
                ..Default::default()
            };
            if let Some((sp, tab, pane)) = places.get(&address) {
                term.space = Some(sp.clone());
                term.tab = Some(tab.clone());
                term.pane = Some(pane.clone());
            }
            if live && (term.status.is_some() || !is_plain_shell(&term.program)) {
                // only terminals that may be agents get the second, more detailed call
                if let Ok(d) = run_json("hc", &["get", &address, "--json"]) {
                    term.argv = d
                        .get("foreground")
                        .and_then(|f| f.get("argv"))
                        .and_then(Value::as_array)
                        .map(|a| {
                            a.iter()
                                .filter_map(|x| x.as_str().map(str::to_string))
                                .collect()
                        })
                        .unwrap_or_default();
                    term.label_run = d
                        .get("labels")
                        .and_then(|l| l.get("dcc.run"))
                        .and_then(Value::as_str)
                        .map(str::to_string);
                    term.status = status_state(&d).or(term.status);
                    term.created_ms = d
                        .get("created_ms")
                        .and_then(Value::as_i64)
                        .or(term.created_ms);
                    if term.cwd.is_none() {
                        term.cwd = d
                            .get("foreground")
                            .and_then(|f| s(f, "cwd"))
                            .or_else(|| s(&d, "cwd"));
                    }
                }
            }
            out.push(term);
        }
        Ok(out)
    }
}

/// A source made of a fixed list, for tests.
pub struct FixedSource(pub Vec<CoreTerminal>);

impl AgentSource for FixedSource {
    fn terminals(&self) -> Result<Vec<CoreTerminal>> {
        Ok(self.0.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::hooks;
    use crate::util::{scratch_repo, sh};

    const SID: &str = "abcdef12-3456-4789-abcd-ef0123456789";

    fn opts(now: i64) -> RecordOptions {
        RecordOptions {
            write: true,
            now_ms: now,
            closed_limit: 10,
            herdr_session: Some("t".into()),
        }
    }

    fn live(addr: &str, status: Option<&str>, cwd: &str, created: i64) -> CoreTerminal {
        CoreTerminal {
            address: addr.into(),
            live: true,
            program: "claude-code".into(),
            cwd: Some(cwd.into()),
            status: status.map(str::to_string),
            created_ms: Some(created),
            argv: vec!["claude".into(), "--session-id".into(), SID.into()],
            space: Some("sp".into()),
            tab: Some("1".into()),
            pane: Some(format!("p-{addr}")),
            ..Default::default()
        }
    }

    fn finished(
        addr: &str,
        exit: Option<i64>,
        reason: &str,
        signal: Option<i64>,
        created: i64,
    ) -> CoreTerminal {
        CoreTerminal {
            address: addr.into(),
            live: false,
            program: "python3".into(),
            created_ms: Some(created),
            finished_ms: Some(900),
            exit_code: exit,
            reason: Some(reason.into()),
            signal,
            ..Default::default()
        }
    }

    fn dcc_run(id: &str, addr: Option<&str>, created: i64) -> Run {
        Run {
            id: id.into(),
            agent_kind: "claude".into(),
            launch_mode: "dcc".into(),
            launch_session_id: Some(id.into()),
            started_ms: 10,
            last_state: "Unknown".into(),
            last_state_ms: 10,
            terminal_addr: addr.map(str::to_string),
            terminal_created_ms: Some(created),
            ..Default::default()
        }
    }

    fn run_row<'a>(doc: &'a Value, id: &str) -> &'a Value {
        doc["runs"]
            .as_array()
            .unwrap()
            .iter()
            .find(|r| r["run_id"] == id)
            .unwrap_or_else(|| panic!("no row {id} in {doc}"))
    }

    #[test]
    fn an_external_agent_gets_one_run_with_its_id_from_argv() {
        let repo = scratch_repo("rec-ext");
        let store = Store::open_in_memory().unwrap();
        let src = FixedSource(vec![live(
            "t1",
            Some("working"),
            repo.to_str().unwrap(),
            100,
        )]);
        let doc = snapshot(&store, &src, &opts(1000)).unwrap();
        let again = snapshot(&store, &src, &opts(2000)).unwrap();
        assert_eq!(doc["runs"].as_array().unwrap().len(), 1);
        assert_eq!(
            again["runs"].as_array().unwrap().len(),
            1,
            "a second snapshot must not duplicate the run"
        );
        let r = &again["runs"][0];
        assert_eq!(
            (
                r["dcc_state"].as_str(),
                r["launch_mode"].as_str(),
                r["session_id"].as_str(),
                r["session_id_source"].as_str()
            ),
            (Some("Running"), Some("external"), Some(SID), Some("argv"))
        );
        assert_eq!(
            (
                r["git"]["branch"].as_str(),
                r["git"]["worktree_kind"].as_str(),
                r["space"].as_str()
            ),
            (Some("main"), Some("main"), Some("sp"))
        );
        assert_eq!(
            r["state_since_ms"], 1000,
            "the state did not change in the second call"
        );
    }

    #[test]
    fn finished_runs_become_completed_failed_or_stopped() {
        let store = Store::open_in_memory().unwrap();
        for (id, addr) in [("ok", "t1"), ("bad", "t2"), ("killed", "t3")] {
            store.insert_run(&dcc_run(id, Some(addr), 100)).unwrap();
        }
        let src = FixedSource(vec![
            finished("t1", Some(0), "exited", None, 100),
            finished("t2", Some(3), "exited", None, 100),
            finished("t3", None, "killed", Some(1), 100),
        ]);
        let doc = snapshot(&store, &src, &opts(1000)).unwrap();
        for (id, state, why) in [
            ("ok", "Completed", "exited_ok"),
            ("bad", "Failed", "exit_nonzero"),
            ("killed", "Stopped", "killed"),
        ] {
            let r = run_row(&doc, id);
            assert_eq!(
                (r["dcc_state"].as_str(), r["end_reason"].as_str()),
                (Some(state), Some(why)),
                "{id}"
            );
            assert_eq!(r["ended_ms"], 900);
        }
        assert_eq!(run_row(&doc, "bad")["exit_code"], 3);
        assert!(store.open_runs().unwrap().is_empty());
    }

    #[test]
    fn a_run_whose_terminal_vanished_is_stopped_as_lost() {
        let store = Store::open_in_memory().unwrap();
        store.insert_run(&dcc_run("gone", Some("t9"), 100)).unwrap();
        let doc = snapshot(&store, &FixedSource(vec![]), &opts(1000)).unwrap();
        assert_eq!(
            (
                run_row(&doc, "gone")["dcc_state"].as_str(),
                run_row(&doc, "gone")["end_reason"].as_str()
            ),
            (Some("Stopped"), Some("lost"))
        );
    }

    #[test]
    fn a_reused_terminal_address_is_a_new_run() {
        let repo = scratch_repo("rec-reuse");
        let store = Store::open_in_memory().unwrap();
        store.insert_run(&dcc_run("old", Some("t1"), 100)).unwrap();
        snapshot(
            &store,
            &FixedSource(vec![finished("t1", Some(0), "exited", None, 100)]),
            &opts(1000),
        )
        .unwrap();
        // the same address comes back with a different creation time
        let doc = snapshot(
            &store,
            &FixedSource(vec![live("t1", Some("idle"), repo.to_str().unwrap(), 5000)]),
            &opts(6000),
        )
        .unwrap();
        assert_eq!(doc["runs"].as_array().unwrap().len(), 2);
        assert_eq!(run_row(&doc, "old")["dcc_state"], "Completed");
        assert!(
            doc["runs"]
                .as_array()
                .unwrap()
                .iter()
                .any(|r| r["run_id"] == "ext-t1-5000" && r["dcc_state"] == "Waiting")
        );
    }

    #[test]
    fn a_dcc_run_without_a_terminal_is_attached_through_its_label_and_a_stale_one_is_lost() {
        let repo = scratch_repo("rec-label");
        let store = Store::open_in_memory().unwrap();
        store.insert_run(&dcc_run(SID, None, 100)).unwrap();
        store.insert_session(SID, SID, "argv", 10).unwrap();
        let mut t = live("t7", Some("working"), repo.to_str().unwrap(), 100);
        t.label_run = Some(SID.into());
        let doc = snapshot(&store, &FixedSource(vec![t]), &opts(1000)).unwrap();
        let r = run_row(&doc, SID);
        assert_eq!(
            (
                r["terminal"].as_str(),
                r["dcc_state"].as_str(),
                r["launch_mode"].as_str()
            ),
            (Some("t7"), Some("Running"), Some("dcc"))
        );
        // a launch that never produced a terminal is given up after a minute
        store.insert_run(&dcc_run("never", None, 0)).unwrap();
        let doc = snapshot(&store, &FixedSource(vec![]), &opts(200_000)).unwrap();
        assert_eq!(run_row(&doc, "never")["end_reason"], "lost");
    }

    #[test]
    fn clear_adds_a_session_to_the_same_run() {
        let repo = scratch_repo("rec-clear");
        let store = Store::open_in_memory().unwrap();
        store.insert_run(&dcc_run(SID, Some("t1"), 100)).unwrap();
        store.insert_session(SID, SID, "argv", 10).unwrap();
        let hook =
            |raw: &str, ts| hooks::record(&store, SID, &hooks::parse(raw).unwrap(), ts).unwrap();
        hook(
            &format!(r#"{{"hook_event_name":"SessionEnd","session_id":"{SID}","reason":"clear"}}"#),
            50,
        );
        hook(
            r#"{"hook_event_name":"SessionStart","session_id":"11111111-2222","source":"clear"}"#,
            51,
        );
        let src = FixedSource(vec![live("t1", Some("idle"), repo.to_str().unwrap(), 100)]);
        let doc = snapshot(&store, &src, &opts(1000)).unwrap();
        let r = run_row(&doc, SID);
        assert_eq!(
            (
                r["session_count"].as_i64(),
                r["session_id"].as_str(),
                r["session_id_source"].as_str()
            ),
            (Some(2), Some("11111111-2222"), Some("hook"))
        );
        assert_eq!(r["last_event"], "SessionStart");
    }

    #[test]
    fn two_agents_in_one_folder_raise_a_warning() {
        let repo = scratch_repo("rec-shared");
        let other = scratch_repo("rec-own");
        let store = Store::open_in_memory().unwrap();
        let (a, b) = (repo.to_str().unwrap(), other.to_str().unwrap());
        let mut t2 = live("t2", Some("working"), a, 200);
        t2.argv.clear();
        let t3 = {
            let mut t = live("t3", Some("working"), b, 300);
            t.argv.clear();
            t
        };
        let mut t1 = live("t1", Some("idle"), a, 100);
        t1.argv.clear();
        let doc = snapshot(&store, &FixedSource(vec![t1, t2, t3]), &opts(1000)).unwrap();
        assert_eq!(doc["warnings"].as_array().unwrap().len(), 1);
        assert!(
            doc["warnings"][0]
                .as_str()
                .unwrap()
                .contains("changed files cannot be told apart")
        );
        assert_eq!(run_row(&doc, "ext-t1-100")["shared_folder"], true);
        assert_eq!(run_row(&doc, "ext-t3-300")["shared_folder"], false);
    }

    #[test]
    fn an_unreliable_channel_makes_live_runs_unknown() {
        let repo = scratch_repo("rec-unreliable");
        let store = Store::open_in_memory().unwrap();
        store.set_state("status_reliable", "false").unwrap();
        let doc = snapshot(
            &store,
            &FixedSource(vec![live(
                "t1",
                Some("working"),
                repo.to_str().unwrap(),
                100,
            )]),
            &opts(1000),
        )
        .unwrap();
        assert_eq!(doc["status_reliable"], false);
        assert_eq!(doc["runs"][0]["dcc_state"], "Unknown");
        assert!(doc["warnings"][0].as_str().unwrap().contains("contract"));
    }

    #[test]
    fn an_agent_that_left_a_shell_behind_is_completed() {
        let repo = scratch_repo("rec-exit");
        let store = Store::open_in_memory().unwrap();
        let cwd = repo.to_str().unwrap();
        snapshot(
            &store,
            &FixedSource(vec![live("t1", Some("idle"), cwd, 100)]),
            &opts(1000),
        )
        .unwrap();
        let mut shell = live("t1", None, cwd, 100);
        shell.program = "zsh".into();
        shell.argv = vec!["/bin/zsh".into()];
        let doc = snapshot(&store, &FixedSource(vec![shell]), &opts(2000)).unwrap();
        let r = &doc["runs"][0];
        assert_eq!(
            (r["dcc_state"].as_str(), r["end_reason"].as_str()),
            (Some("Completed"), Some("agent_exited"))
        );
        assert!(store.open_runs().unwrap().is_empty());
    }

    #[test]
    fn the_status_going_missing_while_claude_runs_is_unknown_not_completed() {
        let repo = scratch_repo("rec-trust");
        let store = Store::open_in_memory().unwrap();
        // a claude process with no status yet (the folder trust question looks like this)
        let mut t = live("t1", None, repo.to_str().unwrap(), 100);
        t.status = None;
        let doc = snapshot(&store, &FixedSource(vec![t]), &opts(1000)).unwrap();
        assert_eq!(
            (
                doc["runs"][0]["dcc_state"].as_str(),
                doc["runs"][0]["note_code"].as_str()
            ),
            (Some("Unknown"), Some("no_status_claude"))
        );
    }

    #[test]
    fn a_read_only_snapshot_changes_nothing() {
        let repo = scratch_repo("rec-ro");
        let store = Store::open_in_memory().unwrap();
        let src = FixedSource(vec![live(
            "t1",
            Some("working"),
            repo.to_str().unwrap(),
            100,
        )]);
        let mut o = opts(1000);
        o.write = false;
        let doc = snapshot(&store, &src, &o).unwrap();
        assert_eq!(
            doc["runs"].as_array().unwrap().len(),
            1,
            "the document still shows the run"
        );
        assert!(
            store.open_runs().unwrap().is_empty(),
            "but nothing was written"
        );
    }

    #[test]
    fn branch_and_worktree_follow_the_folder() {
        let repo = scratch_repo("rec-wt");
        let wt = repo.parent().unwrap().join(format!(
            "{}-wt",
            repo.file_name().unwrap().to_string_lossy()
        ));
        crate::git::worktree_add(repo.to_str().unwrap(), "dcc/one", &wt).unwrap();
        std::fs::write(wt.join("a.txt"), "x").unwrap();
        sh(&wt, &["add", "a.txt"]);
        let store = Store::open_in_memory().unwrap();
        let mut t = live("t1", Some("working"), wt.to_str().unwrap(), 100);
        t.argv.clear();
        let doc = snapshot(&store, &FixedSource(vec![t]), &opts(1000)).unwrap();
        let g = &doc["runs"][0]["git"];
        assert_eq!(
            (
                g["branch"].as_str(),
                g["worktree_kind"].as_str(),
                g["changed_files"].as_i64(),
                g["staged"].as_i64()
            ),
            (Some("dcc/one"), Some("linked"), Some(1), Some(1))
        );
    }
}
