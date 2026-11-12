//! Usage through the real binary: the status line command stores what it is given, `usage show`
//! reports it in two separate parts, and nothing is ever derived.

use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};

use dcc::agent::db::Run;
use dcc::store::Store;
use dcc::util::scratch_dir;
use serde_json::Value;

fn statusline(data: &Path, run_id: &str, doc: &str) -> std::process::Output {
    let mut c = Command::new(env!("CARGO_BIN_EXE_dcc"))
        .args(["--data-dir"])
        .arg(data)
        .args(["agent", "statusline-log"])
        .env("DCC_RUN_ID", run_id)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    c.stdin.take().unwrap().write_all(doc.as_bytes()).unwrap();
    c.wait_with_output().unwrap()
}

fn show(data: &Path) -> Value {
    let o = Command::new(env!("CARGO_BIN_EXE_dcc"))
        .arg("--data-dir")
        .arg(data)
        .args(["usage", "show"])
        .output()
        .unwrap();
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    serde_json::from_slice(&o.stdout).unwrap()
}

fn run(id: &str, mode: &str) -> Run {
    Run {
        id: id.into(),
        agent_kind: "claude".into(),
        launch_mode: mode.into(),
        started_ms: 1,
        last_state: "Unknown".into(),
        last_state_ms: 1,
        ..Default::default()
    }
}

#[test]
fn the_status_line_command_stores_reported_values_and_the_panel_data_keeps_the_parts_apart() {
    let data = scratch_dir("usage-cli");
    {
        let s = Store::open(&data).unwrap();
        s.insert_run(&run("abcdef12-aaaa", "dcc")).unwrap();
        s.insert_run(&run("abcdef12-bbbb", "external")).unwrap();
    }
    let doc = r#"{"model":{"id":"m"},"context_window":{"used_percentage":null},"rate_limits":{"five_hour":{"used_percentage":12.5,"resets_at":1900000000}}}"#;
    let o = statusline(&data, "abcdef12-aaaa", doc);
    assert!(o.status.success());
    assert_eq!(
        strip(String::from_utf8_lossy(&o.stdout).trim()),
        "ctx ? | 5h 12.5% | 7d ?"
    );

    let v = show(&data);
    let ctx = v["context"].as_array().unwrap();
    let mine = ctx.iter().find(|c| c["run_id"] == "abcdef12-aaaa").unwrap();
    assert_eq!(mine["status"], "unknown", "a null percentage stays unknown");
    assert!(mine["used_percentage"].is_null());
    let other = ctx.iter().find(|c| c["run_id"] == "abcdef12-bbbb").unwrap();
    assert_eq!(
        other["status"], "unavailable",
        "a run DCC did not start has no source"
    );
    let five = &v["quota"]["windows"][0];
    assert_eq!(
        (
            five["window"].as_str(),
            five["used_percentage"].as_f64(),
            five["resets_at_ms"].as_i64()
        ),
        (Some("five_hour"), Some(12.5), Some(1_900_000_000_000))
    );
    assert_eq!(v["quota"]["windows"][1]["status"], "unavailable");

    // the sample of a context that became known later replaces "unknown"
    let doc2 = r#"{"context_window":{"used_percentage":31}}"#;
    assert!(statusline(&data, "abcdef12-aaaa", doc2).status.success());
    let v = show(&data);
    let mine = v["context"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["run_id"] == "abcdef12-aaaa")
        .unwrap()
        .clone();
    assert_eq!(
        (mine["status"].as_str(), mine["used_percentage"].as_f64()),
        (Some("known"), Some(31.0))
    );
}

#[test]
fn the_status_line_command_never_fails_and_stores_no_text_from_the_document() {
    let data = scratch_dir("usage-cli-safe");
    Store::open(&data)
        .unwrap()
        .insert_run(&run("abcdef12-cccc", "dcc"))
        .unwrap();
    for doc in [
        "garbage",
        "",
        "{\"context_window\":{\"used_percentage\":\"x\"}}",
        "{\"model\":{\"display_name\":\"PRIVATE-NAME\"}}",
    ] {
        assert!(statusline(&data, "abcdef12-cccc", doc).status.success());
    }
    assert!(statusline(&data, "../evil", "{}").status.success());
    let s = Store::open(&data).unwrap();
    let n: i64 = s
        .conn()
        .query_row("SELECT COUNT(*) FROM usage_samples", [], |r| r.get(0))
        .unwrap();
    assert_eq!(n, 0);
    // a quota history over an empty store is an empty list, not an error
    let o = Command::new(env!("CARGO_BIN_EXE_dcc"))
        .arg("--data-dir")
        .arg(&data)
        .args(["usage", "history", "--kind", "quota"])
        .output()
        .unwrap();
    assert!(o.status.success());
    let h: Value = serde_json::from_slice(&o.stdout).unwrap();
    assert!(h["samples"].as_array().unwrap().is_empty());
    let bad = Command::new(env!("CARGO_BIN_EXE_dcc"))
        .arg("--data-dir")
        .arg(&data)
        .args(["usage", "history", "--kind", "cost"])
        .output()
        .unwrap();
    assert!(!bad.status.success());
}

/// Code review as a test. The part of the usage module that runs may read the reported
/// percentage and the reset time, and nothing else from the status line document. Token counts,
/// the window size and the cost are never looked at, so no number can be computed from them.
#[test]
fn the_usage_code_reads_only_the_reported_percentage_and_reset_time() {
    let src = std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("src/usage.rs"))
        .unwrap();
    let code = src.split("#[cfg(test)]").next().unwrap();
    for forbidden in [
        "current_usage",
        "input_tokens",
        "output_tokens",
        "context_window_size",
        "total_cost",
        "remaining_percentage",
        "cache_read",
    ] {
        assert!(
            !code.contains(forbidden),
            "usage.rs must not read {forbidden}"
        );
    }
    for used in ["used_percentage", "resets_at"] {
        assert!(code.contains(used));
    }
    // no arithmetic is done on a percentage
    for op in [
        "used_percentage *",
        "used_percentage /",
        "used_percentage +",
        "used_percentage -",
        "100.0",
        "as f64 /",
        "as f64 *",
    ] {
        assert!(
            !code.contains(op),
            "usage.rs must not compute with a percentage ({op})"
        );
    }
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
