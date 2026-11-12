//! Runs the real `dcc` binary the way Claude Code does: hook and status line commands with the
//! event on stdin and `DCC_RUN_ID` in the environment.

use std::io::Write;
use std::path::Path;
use std::process::{Command, Output, Stdio};
use std::time::Instant;

use dcc::agent::db::Run;
use dcc::store::Store;
use dcc::util::scratch_dir;

const RUN: &str = "abcdef12-3456-4789-abcd-ef0123456789";

fn seed(data: &Path) {
    let store = Store::open(data).unwrap();
    store
        .insert_run(&Run {
            id: RUN.into(),
            agent_kind: "claude".into(),
            launch_mode: "dcc".into(),
            launch_session_id: Some(RUN.into()),
            started_ms: 1,
            last_state: "Unknown".into(),
            last_state_ms: 1,
            ..Default::default()
        })
        .unwrap();
    store.insert_session(RUN, RUN, "argv", 1).unwrap();
}

fn run_dcc(data: &Path, args: &[&str], stdin: &str, run_id: Option<&str>) -> Output {
    let mut c = Command::new(env!("CARGO_BIN_EXE_dcc"));
    c.arg("--data-dir")
        .arg(data)
        .args(args)
        .env_remove("DCC_RUN_ID")
        .env_remove("DCC_REQUIRE_ISOLATION");
    if let Some(r) = run_id {
        c.env("DCC_RUN_ID", r);
    }
    let mut child = c
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(stdin.as_bytes())
        .unwrap();
    child.wait_with_output().unwrap()
}

const EVENT: &str = r#"{"session_id":"abcdef12-3456-4789-abcd-ef0123456789","transcript_path":"/home/u/.claude/projects/x/abc.jsonl","cwd":"/work/SECRET-PROJECT-NAME",
    "hook_event_name":"PreToolUse","tool_name":"Write","tool_input":{"file_path":"/work/SECRET-FILE.txt","content":"CONTENT-THAT-MUST-NOT-LEAK"},
    "prompt":"PROMPT-THAT-MUST-NOT-LEAK","last_assistant_message":"ANSWER-THAT-MUST-NOT-LEAK"}"#;

fn everything_stored(data: &Path) -> String {
    let store = Store::open(data).unwrap();
    let mut out = String::new();
    for table in ["agent_runs", "agent_sessions", "agent_events"] {
        let mut stmt = store
            .conn()
            .prepare(&format!("SELECT * FROM {table}"))
            .unwrap();
        let n = stmt.column_count();
        let mut rows = stmt.query([]).unwrap();
        while let Some(r) = rows.next().unwrap() {
            for i in 0..n {
                match r.get_ref(i).unwrap() {
                    rusqlite::types::ValueRef::Text(t) | rusqlite::types::ValueRef::Blob(t) => {
                        out.push_str(&String::from_utf8_lossy(t))
                    }
                    other => out.push_str(&format!("{other:?}")),
                }
                out.push(' ');
            }
        }
    }
    out
}

#[test]
fn a_hook_records_names_and_ids_and_nothing_else() {
    let data = scratch_dir("hook-content");
    seed(&data);
    let o = run_dcc(&data, &["agent", "hook-log"], EVENT, Some(RUN));
    assert_eq!(o.status.code(), Some(0));
    assert!(
        o.stdout.is_empty() && o.stderr.is_empty(),
        "a hook says nothing"
    );
    let dump = everything_stored(&data);
    assert!(dump.contains("PreToolUse") && dump.contains("Write"));
    for leaked in [
        "SECRET-PROJECT-NAME",
        "SECRET-FILE",
        "CONTENT-THAT-MUST-NOT-LEAK",
        "PROMPT-THAT-MUST-NOT-LEAK",
        "ANSWER-THAT-MUST-NOT-LEAK",
        "abc.jsonl",
        "/work/",
    ] {
        assert!(!dump.contains(leaked), "{leaked} reached the database");
    }
    // the raw database file must not hold them either (WAL file included)
    for entry in std::fs::read_dir(&data).unwrap().flatten() {
        let bytes = std::fs::read(entry.path()).unwrap_or_default();
        assert!(
            !String::from_utf8_lossy(&bytes).contains("CONTENT-THAT-MUST-NOT-LEAK"),
            "{}",
            entry.path().display()
        );
    }
}

#[test]
fn a_hook_always_exits_zero_whatever_goes_wrong() {
    let data = scratch_dir("hook-fail");
    seed(&data);
    for (stdin, run) in [
        ("not json at all", Some(RUN)),
        ("", Some(RUN)),
        (EVENT, None),
        (EVENT, Some("../../evil")),
        (EVENT, Some("deadbeef-0000-0000-0000-000000000000")),
    ] {
        let o = run_dcc(&data, &["agent", "hook-log"], stdin, run);
        assert_eq!(o.status.code(), Some(0), "stdin {stdin:?} run {run:?}");
        assert!(o.stdout.is_empty());
    }
    // errors are noted by kind only
    let log = std::fs::read_to_string(data.join("hook-errors.log")).unwrap();
    assert!(!log.contains("PROMPT") && !log.contains("/work"), "{log}");
    assert!(log.lines().count() >= 3);
    // and with no data folder at all
    let o = Command::new(env!("CARGO_BIN_EXE_dcc"))
        .args(["agent", "hook-log"])
        .env_remove("DCC_DATA_DIR")
        .env("DCC_RUN_ID", RUN)
        .stdin(Stdio::null())
        .output()
        .unwrap();
    assert_eq!(o.status.code(), Some(0));
    // and with a bad command line (an old settings file after a dcc update). Exit code 2 would
    // block the tool call, so the hook and status line commands exit 0 even then.
    for sub in ["hook-log", "statusline-log"] {
        let o = Command::new(env!("CARGO_BIN_EXE_dcc"))
            .args(["agent", sub, "--no-such-flag"])
            .stdin(Stdio::null())
            .output()
            .unwrap();
        assert_eq!(o.status.code(), Some(0), "{sub}");
    }
    // any other command still reports a usage error
    let o = Command::new(env!("CARGO_BIN_EXE_dcc"))
        .args(["agent", "snapshot", "--no-such-flag"])
        .stdin(Stdio::null())
        .output()
        .unwrap();
    assert_eq!(o.status.code(), Some(2));
}

#[test]
fn a_hook_is_fast() {
    let data = scratch_dir("hook-speed");
    seed(&data);
    run_dcc(&data, &["agent", "hook-log"], EVENT, Some(RUN)); // warm up
    let start = Instant::now();
    for _ in 0..10 {
        run_dcc(&data, &["agent", "hook-log"], EVENT, Some(RUN));
    }
    let each = start.elapsed().as_millis() / 10;
    assert!(each < 400, "a hook took {each} ms on average");
    eprintln!("hook-log average {each} ms");
}

#[test]
fn the_status_line_draws_the_reported_values_and_keeps_keys_not_values() {
    let data = scratch_dir("statusline");
    let json = r#"{"model":{"id":"VALUE-MODEL","display_name":"x"},"context_window":{"used_percentage":8},"rate_limits":{"five_hour":{"used_percentage":1}}}"#;
    let o = run_dcc(&data, &["agent", "statusline-log"], json, Some(RUN));
    assert_eq!(
        (
            o.status.code(),
            strip(String::from_utf8_lossy(&o.stdout).trim())
        ),
        (Some(0), "x | ctx 8% | 5h 1% | 7d ?".to_string())
    );
    let text = std::fs::read_to_string(data.join("agents").join(RUN).join("statusline-keys.json"))
        .unwrap();
    assert!(
        text.contains("context_window.used_percentage")
            && text.contains("rate_limits.five_hour.used_percentage")
    );
    assert!(!text.contains("VALUE-MODEL"));
    // garbage prints the word, a missing run id still draws the line (nothing is stored), both exit 0
    for (stdin, run, want) in [
        ("garbage", Some(RUN), "dcc"),
        (json, None, "x | ctx 8% | 5h 1% | 7d ?"),
    ] {
        let o = run_dcc(&data, &["agent", "statusline-log"], stdin, run);
        assert_eq!(
            (
                o.status.code(),
                strip(String::from_utf8_lossy(&o.stdout).trim())
            ),
            (Some(0), want.to_string())
        );
    }
}

#[cfg(unix)]
#[test]
fn exec_cleans_the_environment_and_sets_the_run_id() {
    let data = scratch_dir("exec-env");
    let o = Command::new(env!("CARGO_BIN_EXE_dcc"))
        .arg("--data-dir")
        .arg(&data)
        .args(["agent", "exec", "--run-id", RUN, "--", "/usr/bin/env"])
        .env("CLAUDECODE", "1")
        .env("CLAUDE_CODE_SESSION_ID", "x")
        .env("CLAUDE_CODE_MESSAGING_TOKEN", "TOKEN-VALUE")
        .env("AI_AGENT", "claude")
        .env("CLAUDE_CONFIG_DIR", "/keep/me")
        .output()
        .unwrap();
    let text = String::from_utf8_lossy(&o.stdout).to_string();
    assert!(text.contains(&format!("DCC_RUN_ID={RUN}")));
    for gone in [
        "CLAUDECODE=",
        "CLAUDE_CODE_SESSION_ID=",
        "CLAUDE_CODE_MESSAGING_TOKEN=",
        "AI_AGENT=",
        "TOKEN-VALUE",
    ] {
        assert!(!text.contains(gone), "{gone} was not removed");
    }
    assert!(
        text.contains("CLAUDE_CONFIG_DIR=/keep/me"),
        "the user's own setting stays"
    );
}

#[test]
fn snapshot_and_contract_refuse_to_run_without_isolation_when_it_is_required() {
    let data = scratch_dir("iso-agent");
    for args in [&["agent", "snapshot"][..], &["diag", "contract"][..]] {
        let o = Command::new(env!("CARGO_BIN_EXE_dcc"))
            .arg("--data-dir")
            .arg(&data)
            .args(args)
            .env("DCC_REQUIRE_ISOLATION", "1")
            .env_remove("HERDR_APP_HOME")
            .env_remove("HC_SESSION")
            .output()
            .unwrap();
        assert_eq!(o.status.code(), Some(1), "{args:?}");
        assert!(String::from_utf8_lossy(&o.stderr).contains("isolation"));
    }
}

#[test]
fn git_worktree_commands_need_the_confirmation_to_remove() {
    let repo = dcc::util::scratch_repo("cli-wt");
    let wt = repo.parent().unwrap().join(format!(
        "{}-wt",
        repo.file_name().unwrap().to_string_lossy()
    ));
    let run = |args: &[&str]| {
        Command::new(env!("CARGO_BIN_EXE_dcc"))
            .args(args)
            .output()
            .unwrap()
    };
    let add = run(&[
        "git",
        "worktree",
        "add",
        repo.to_str().unwrap(),
        "--branch",
        "dcc/x",
        wt.to_str().unwrap(),
    ]);
    assert!(
        add.status.success(),
        "{}",
        String::from_utf8_lossy(&add.stderr)
    );
    let denied = run(&[
        "git",
        "worktree",
        "remove",
        repo.to_str().unwrap(),
        wt.to_str().unwrap(),
    ]);
    assert_eq!(denied.status.code(), Some(2));
    assert!(wt.exists());
    let done = run(&[
        "git",
        "worktree",
        "remove",
        repo.to_str().unwrap(),
        wt.to_str().unwrap(),
        "--confirm",
    ]);
    assert!(done.status.success());
    assert!(!wt.exists());
}

/// The status line output without its colour codes.
fn strip(s: &str) -> String {
    let mut out = String::new();
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        if c == '\x1b' {
            for n in chars.by_ref() {
                if n == 'm' {
                    break;
                }
            }
        } else {
            out.push(c);
        }
    }
    out
}
