//! Calls to Skillshare, the tool that syncs skills between AI tools. DCC has no sync engine of its
//! own. Reading (status, list, diff, dry run) is free. Writing (`apply`) happens only for a
//! preview the user has seen (its hash), after Skillshare's own backup of the targets succeeded,
//! and an action that removes something needs its own flag.

use std::path::PathBuf;
use std::process::Command;

use serde_json::{Value, json};

use crate::error::{Error, Result};
use crate::store::Store;
use crate::util::{fnv1a_hex, now_ms};

pub struct Output {
    pub code: i32,
    pub stdout: String,
    pub stderr: String,
}

pub trait Runner {
    fn run(&self, args: &[&str]) -> Result<Output>;
}

/// Runs the installed `skillshare` with the given home. The home (and the config folder under
/// it) is set explicitly, so a test or a dev run can point it at a scratch folder.
pub struct CliRunner {
    program: PathBuf,
    home: PathBuf,
}

impl CliRunner {
    pub fn new(home: PathBuf) -> Result<Self> {
        let program = crate::agent::adapter::find_program("skillshare").ok_or_else(|| {
            Error::not_found(
                "skillshare is not installed (DCC installs nothing), see https://skillshare.runkids.cc",
            )
        })?;
        Ok(Self { program, home })
    }
}

impl Runner for CliRunner {
    fn run(&self, args: &[&str]) -> Result<Output> {
        let out = Command::new(&self.program)
            .args(args)
            .env("HOME", &self.home)
            .env("XDG_CONFIG_HOME", self.home.join(".config"))
            .env("NO_COLOR", "1")
            .stdin(std::process::Stdio::null())
            .output()
            .map_err(|e| Error::external(format!("cannot run skillshare: {e}")))?;
        Ok(Output {
            code: out.status.code().unwrap_or(-1),
            stdout: String::from_utf8_lossy(&out.stdout).to_string(),
            stderr: String::from_utf8_lossy(&out.stderr).to_string(),
        })
    }
}

fn json_of(o: &Output, what: &str) -> Result<Value> {
    if o.code != 0 {
        return Err(Error::external(format!(
            "skillshare {what} failed ({}): {}",
            o.code,
            o.stderr.trim().lines().next().unwrap_or("no message")
        )));
    }
    serde_json::from_str(&o.stdout)
        .map_err(|_| Error::external(format!("skillshare {what} did not answer with JSON")))
}

pub fn status(r: &dyn Runner) -> Result<Value> {
    json_of(&r.run(&["status", "--json"])?, "status")
}

pub fn list(r: &dyn Runner) -> Result<Value> {
    json_of(&r.run(&["list", "--json", "--no-tui"])?, "list")
}

/// What a sync would do, and a hash of exactly that.
pub fn preview(r: &dyn Runner) -> Result<Value> {
    let diff = json_of(&r.run(&["diff", "--json", "--no-tui"])?, "diff")?;
    let dry = json_of(
        &r.run(&["sync", "--dry-run", "--json", "--quiet"])?,
        "sync --dry-run",
    )?;
    let mut adds = 0u64;
    let mut updates = 0u64;
    let mut removals: Vec<String> = Vec::new();
    for t in diff["targets"].as_array().cloned().unwrap_or_default() {
        let target = t["name"].as_str().unwrap_or("?").to_string();
        for i in t["items"].as_array().cloned().unwrap_or_default() {
            let name = i["name"].as_str().unwrap_or("?");
            match i["action"].as_str() {
                Some("add") => adds += 1,
                Some("update") => updates += 1,
                // `is_sync` false means a local only item that sync leaves alone
                Some("remove") if i["is_sync"].as_bool() == Some(true) => {
                    removals.push(format!("{target}/{name}"));
                }
                _ => {}
            }
        }
    }
    let pruned = dry["pruned"].as_u64().unwrap_or(0);
    // what the hash covers. Times and token estimates are left out so the same state hashes the same.
    let basis = json!({ "diff": diff["targets"], "details": dry["details"], "pruned": pruned });
    let hash = fnv1a_hex(basis.to_string().as_bytes());
    Ok(json!({
        "hash": hash,
        "writes": false,
        "summary": { "add": adds, "update": updates, "remove": removals.len().max(pruned as usize) },
        "removals": removals,
        "diff": diff,
        "dry_run": { "targets": dry["targets"], "linked": dry["linked"], "updated": dry["updated"], "pruned": pruned },
    }))
}

/// Backs up the targets, then syncs. Refuses a preview that changed, and a sync that removes
/// something unless `allow_delete` is set. Nothing is changed before the backup succeeded.
pub fn apply(
    r: &dyn Runner,
    store: &Store,
    preview_hash: &str,
    allow_delete: bool,
) -> Result<Value> {
    let now = preview(r)?;
    if now["hash"].as_str() != Some(preview_hash) {
        return Err(Error::conflict(
            "what a sync would do has changed since the preview, nothing was written. Preview again.",
        ));
    }
    let removes = now["summary"]["remove"].as_u64().unwrap_or(0);
    if removes > 0 && !allow_delete {
        return Err(Error::conflict(format!(
            "the sync would remove {removes} item(s), nothing was written. Approve the removal separately with --allow-delete."
        )));
    }
    let backup = r.run(&["backup", "--all"])?;
    if backup.code != 0 {
        return Err(Error::external(format!(
            "the backup failed, nothing was synced: {}",
            backup.stderr.trim().lines().next().unwrap_or("no message")
        )));
    }
    let sync = json_of(&r.run(&["sync", "--json", "--quiet"])?, "sync")?;
    store.set_state("config.last_sync_ms", &now_ms().to_string())?;
    Ok(json!({
        "applied": true,
        "backup": { "ok": true, "restore_with": "skillshare restore" },
        "removals_approved": removes > 0,
        "sync": { "linked": sync["linked"], "updated": sync["updated"], "pruned": sync["pruned"] },
    }))
}

#[cfg(test)]
pub mod tests {
    use super::*;
    use std::cell::RefCell;

    /// Answers from a list, records what was asked.
    pub struct Fake {
        pub calls: RefCell<Vec<String>>,
        pub diff: RefCell<Value>,
        pub backup_code: i32,
    }

    impl Fake {
        pub fn new(diff: Value) -> Self {
            Self {
                calls: RefCell::new(vec![]),
                diff: RefCell::new(diff),
                backup_code: 0,
            }
        }
        pub fn asked(&self) -> Vec<String> {
            self.calls.borrow().clone()
        }
    }

    impl Runner for Fake {
        fn run(&self, args: &[&str]) -> Result<Output> {
            self.calls.borrow_mut().push(args.join(" "));
            let ok = |v: Value| {
                Ok(Output {
                    code: 0,
                    stdout: v.to_string(),
                    stderr: String::new(),
                })
            };
            match args[0] {
                "diff" => ok(self.diff.borrow().clone()),
                "sync" if args.contains(&"--dry-run") => ok(
                    json!({"linked":1,"updated":0,"pruned":0,"details":[{"name":"claude","linked":1}],"targets":1}),
                ),
                "sync" => ok(json!({"linked":1,"updated":0,"pruned":0})),
                "backup" => Ok(Output {
                    code: self.backup_code,
                    stdout: String::new(),
                    stderr: "disk full".into(),
                }),
                "status" => ok(json!({"version":"x"})),
                _ => Err(Error::usage("unexpected")),
            }
        }
    }

    pub fn diff_with(items: Value) -> Value {
        json!({"targets":[{"name":"claude","items":items}],"duration":"1s"})
    }

    #[test]
    fn a_preview_counts_changes_and_never_writes() {
        let f = Fake::new(diff_with(json!([
            {"action":"add","name":"a","is_sync":true},
            {"action":"update","name":"b","is_sync":true},
            {"action":"remove","name":"local","is_sync":false},
        ])));
        let p = preview(&f).unwrap();
        assert_eq!(
            (
                p["summary"]["add"].as_u64(),
                p["summary"]["update"].as_u64(),
                p["summary"]["remove"].as_u64()
            ),
            (Some(1), Some(1), Some(0))
        );
        assert_eq!(p["writes"], false);
        assert!(
            f.asked().iter().all(
                |c| c.starts_with("diff") || (c.starts_with("sync") && c.contains("--dry-run"))
            )
        );
        assert_eq!(
            p["hash"],
            preview(&f).unwrap()["hash"],
            "the same state gives the same hash"
        );
    }

    #[test]
    fn apply_needs_the_hash_of_the_preview_that_was_shown() {
        let store = Store::open_in_memory().unwrap();
        let f = Fake::new(diff_with(
            json!([{"action":"add","name":"a","is_sync":true}]),
        ));
        let hash = preview(&f).unwrap()["hash"].as_str().unwrap().to_string();
        let wrong = apply(&f, &store, "0000", false).unwrap_err();
        assert_eq!(wrong.kind, "conflict");
        assert!(
            !f.asked().iter().any(|c| c.starts_with("backup")),
            "nothing may run before the hash matched"
        );
        // the state moves on, the old hash is refused
        *f.diff.borrow_mut() = diff_with(
            json!([{"action":"add","name":"a","is_sync":true},{"action":"add","name":"b","is_sync":true}]),
        );
        assert_eq!(
            apply(&f, &store, &hash, false).unwrap_err().kind,
            "conflict"
        );
    }

    #[test]
    fn apply_backs_up_first_then_syncs_and_records_the_time() {
        let store = Store::open_in_memory().unwrap();
        let f = Fake::new(diff_with(
            json!([{"action":"add","name":"a","is_sync":true}]),
        ));
        let hash = preview(&f).unwrap()["hash"].as_str().unwrap().to_string();
        f.calls.borrow_mut().clear();
        let r = apply(&f, &store, &hash, false).unwrap();
        assert_eq!(r["applied"], true);
        let calls = f.asked();
        let pos = |p: &str| {
            calls
                .iter()
                .position(|c| c.starts_with(p) && !c.contains("--dry-run"))
                .unwrap()
        };
        assert!(pos("backup") < pos("sync"), "{calls:?}");
        assert!(store.state("config.last_sync_ms").unwrap().is_some());
    }

    #[test]
    fn a_failed_backup_stops_everything() {
        let store = Store::open_in_memory().unwrap();
        let mut f = Fake::new(diff_with(
            json!([{"action":"add","name":"a","is_sync":true}]),
        ));
        f.backup_code = 1;
        let hash = preview(&f).unwrap()["hash"].as_str().unwrap().to_string();
        f.calls.borrow_mut().clear();
        let e = apply(&f, &store, &hash, false).unwrap_err();
        assert!(e.message.contains("nothing was synced"));
        assert!(!f.asked().iter().any(|c| c == "sync --json --quiet"));
        assert!(store.state("config.last_sync_ms").unwrap().is_none());
    }

    #[test]
    fn a_removal_needs_its_own_approval() {
        let store = Store::open_in_memory().unwrap();
        let f = Fake::new(diff_with(
            json!([{"action":"remove","name":"gone","is_sync":true}]),
        ));
        let p = preview(&f).unwrap();
        assert_eq!(p["summary"]["remove"], 1);
        assert_eq!(p["removals"][0], "claude/gone");
        let hash = p["hash"].as_str().unwrap().to_string();
        f.calls.borrow_mut().clear();
        let e = apply(&f, &store, &hash, false).unwrap_err();
        assert!(e.message.contains("--allow-delete"));
        assert!(
            !f.asked().iter().any(|c| c.starts_with("backup")),
            "refused before any backup or write"
        );
        let ok = apply(&f, &store, &hash, true).unwrap();
        assert_eq!(ok["removals_approved"], true);
    }
}
