//! Detached jobs for callers that cannot wait (a Herdr mod, whose `exec` is stopped after 30 s).
//! Same idea as the Skillshare mod (START and ALIVE in its model.js) in plain Rust, so it needs no
//! `sh`, `setsid`, `mktemp` or `kill -0`.
//!
//! - `start -- <dcc args>` makes `<data>/jobs/<id>/`, starts a detached `dcc job run` in its own
//!   process group and without `HC_SCOPE` (Herdr hangs up programs that carry it), waits up to 5 s
//!   for the `pid` file, prints the id and returns.
//! - The runner writes `out`, `err`, then `code` (atomically) and touches `hb` every second.
//! - `status` is a short read: running, done, dead or unknown.
//! - `clean` removes one job folder, `sweep` removes old finished or dead ones.
//!
//! Job folders are 0700 on Unix. The Windows branch is written but has never been compiled or run.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use serde_json::{Value, json};

use crate::error::{Error, Result};
use crate::util::{create_private_dir, now_ms};

const HEARTBEAT_STALE_MS: i64 = 5000;
const START_WAIT_MS: u64 = 5000;

fn jobs_root(data_dir: &Path) -> PathBuf {
    data_dir.join("jobs")
}

/// Ids are `<ms>-<pid>`, digits and one dash. Anything else could point outside the jobs folder.
fn job_dir(data_dir: &Path, id: &str) -> Result<PathBuf> {
    if id.is_empty() || !id.chars().all(|c| c.is_ascii_digit() || c == '-') {
        return Err(Error::usage("invalid job id"));
    }
    Ok(jobs_root(data_dir).join(id))
}

fn read_trim(path: &Path) -> Option<String> {
    fs::read_to_string(path).ok().map(|s| s.trim().to_string())
}

fn started_ms(id: &str) -> i64 {
    id.split('-')
        .next()
        .and_then(|s| s.parse().ok())
        .unwrap_or(0)
}

pub fn start(data_dir: &Path, args: &[String]) -> Result<Value> {
    if args.is_empty() {
        return Err(Error::usage("job start needs a dcc command after --"));
    }
    let id = format!("{}-{}", now_ms(), std::process::id());
    let dir = job_dir(data_dir, &id)?;
    create_private_dir(&jobs_root(data_dir))?;
    create_private_dir(&dir)?;
    let exe = std::env::current_exe()?;

    let mut cmd = Command::new(exe);
    cmd.arg("--data-dir")
        .arg(data_dir)
        .arg("job")
        .arg("run")
        .arg(&dir)
        .arg("--")
        .args(args)
        .env_remove("HC_SCOPE")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        cmd.process_group(0);
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const DETACHED_PROCESS: u32 = 0x0000_0008;
        const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;
        cmd.creation_flags(DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP);
    }
    let child = cmd
        .spawn()
        .map_err(|e| Error::external(format!("cannot start job runner: {e}")))?;
    drop(child); // never waited on, it outlives this process

    let mut waited = 0;
    while !dir.join("pid").exists() && waited < START_WAIT_MS {
        std::thread::sleep(Duration::from_millis(50));
        waited += 50;
    }
    if !dir.join("pid").exists() {
        return Err(Error::external("job runner did not report a pid in time"));
    }
    Ok(json!({ "id": id, "dir": dir.to_string_lossy() }))
}

/// Runs inside the detached process. Executes `dcc <args>`, keeps a heartbeat, writes `code` last.
pub fn run(dir: &Path, data_dir: &Path, args: &[String]) -> i32 {
    let _ = fs::write(dir.join("pid"), std::process::id().to_string());

    let stop = Arc::new(AtomicBool::new(false));
    let hb_dir = dir.to_path_buf();
    let hb_stop = stop.clone();
    let heartbeat = std::thread::spawn(move || {
        while !hb_stop.load(Ordering::Relaxed) {
            let _ = fs::write(hb_dir.join("hb"), now_ms().to_string());
            std::thread::sleep(Duration::from_millis(1000));
        }
    });

    let code = match std::env::current_exe().and_then(|exe| {
        let out = fs::File::create(dir.join("out"))?;
        let err = fs::File::create(dir.join("err"))?;
        Command::new(exe)
            .arg("--data-dir")
            .arg(data_dir)
            .args(args)
            .env_remove("HC_SCOPE")
            .stdin(Stdio::null())
            .stdout(out)
            .stderr(err)
            .status()
    }) {
        Ok(s) => s.code().unwrap_or(1),
        Err(e) => {
            let _ = fs::write(dir.join("err"), format!("cannot run job: {e}"));
            127
        }
    };

    stop.store(true, Ordering::Relaxed);
    let _ = heartbeat.join();
    let tmp = dir.join("code.tmp");
    let _ = fs::write(&tmp, code.to_string());
    let _ = fs::rename(&tmp, dir.join("code"));
    code
}

pub fn status(data_dir: &Path, id: &str) -> Result<Value> {
    let dir = job_dir(data_dir, id)?;
    if !dir.exists() {
        return Ok(json!({ "id": id, "state": "unknown" }));
    }
    if let Some(code) = read_trim(&dir.join("code")).filter(|c| !c.is_empty()) {
        return Ok(json!({
            "id": id,
            "state": "done",
            "code": code.parse::<i64>().unwrap_or(1),
            "stdout": fs::read_to_string(dir.join("out")).unwrap_or_default(),
            "stderr": fs::read_to_string(dir.join("err")).unwrap_or_default(),
        }));
    }
    // No code yet. Running if the heartbeat is fresh. Right after start there may be no heartbeat
    // yet, so fall back to the age of the job.
    let started = started_ms(id);
    let last = read_trim(&dir.join("hb"))
        .and_then(|s| s.parse::<i64>().ok())
        .unwrap_or(started);
    if now_ms() - last <= HEARTBEAT_STALE_MS {
        Ok(json!({ "id": id, "state": "running", "elapsed_ms": now_ms() - started }))
    } else {
        Ok(json!({ "id": id, "state": "dead", "stderr": "job stopped before it finished" }))
    }
}

pub fn clean(data_dir: &Path, id: &str) -> Result<Value> {
    let dir = job_dir(data_dir, id)?;
    if dir.exists() {
        fs::remove_dir_all(&dir)?;
    }
    Ok(json!({ "id": id, "cleaned": true }))
}

/// Removes job folders older than `older_than_secs` that are finished (a `code` file) or dead
/// (stale heartbeat). A running job and a young one are never touched. Age comes from the id.
pub fn sweep(data_dir: &Path, older_than_secs: i64) -> Result<Value> {
    let mut removed = Vec::new();
    let mut kept = 0;
    let Ok(entries) = fs::read_dir(jobs_root(data_dir)) else {
        return Ok(json!({ "removed": removed, "kept": kept }));
    };
    for entry in entries.flatten() {
        let id = entry.file_name().to_string_lossy().to_string();
        let Ok(dir) = job_dir(data_dir, &id) else {
            continue;
        };
        let started = started_ms(&id);
        let finished = dir.join("code").exists();
        let last = read_trim(&dir.join("hb"))
            .and_then(|s| s.parse::<i64>().ok())
            .unwrap_or(started);
        let heartbeat_over = now_ms() - last > HEARTBEAT_STALE_MS;
        if now_ms() - started > older_than_secs * 1000
            && (finished || heartbeat_over)
            && fs::remove_dir_all(&dir).is_ok()
        {
            removed.push(id);
        } else {
            kept += 1;
        }
    }
    removed.sort();
    Ok(json!({ "removed": removed, "kept": kept }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::util::scratch_dir;

    fn make(data: &Path, id: &str, code: Option<&str>, hb: Option<i64>) {
        let d = jobs_root(data).join(id);
        fs::create_dir_all(&d).unwrap();
        if let Some(c) = code {
            fs::write(d.join("code"), c).unwrap();
        }
        if let Some(h) = hb {
            fs::write(d.join("hb"), h.to_string()).unwrap();
        }
    }

    #[test]
    fn ids_cannot_escape_the_jobs_folder() {
        let d = scratch_dir("jobs-id");
        for bad in ["", "../x", "1-2/../3", "abc", "1 2"] {
            assert_eq!(status(&d, bad).unwrap_err().kind, "usage", "{bad}");
        }
    }

    #[test]
    fn status_states() {
        let d = scratch_dir("jobs-status");
        let now = now_ms();
        assert_eq!(status(&d, "1-1").unwrap()["state"], "unknown");
        make(&d, &format!("{now}-1"), Some("0"), None);
        assert_eq!(status(&d, &format!("{now}-1")).unwrap()["state"], "done");
        make(&d, &format!("{now}-2"), None, Some(now));
        assert_eq!(status(&d, &format!("{now}-2")).unwrap()["state"], "running");
        make(&d, &format!("{now}-3"), None, Some(now - 60_000));
        assert_eq!(status(&d, &format!("{now}-3")).unwrap()["state"], "dead");
        // right after start, no heartbeat yet but the job is young
        make(&d, &format!("{now}-4"), None, None);
        assert_eq!(status(&d, &format!("{now}-4")).unwrap()["state"], "running");
    }

    #[test]
    fn sweep_removes_old_finished_and_dead_only() {
        let d = scratch_dir("jobs-sweep");
        let now = now_ms();
        let old = now - 2 * 3600 * 1000;
        make(&d, &format!("{old}-1"), Some("0"), None); // old, finished
        make(&d, &format!("{old}-2"), None, Some(old)); // old, dead
        make(&d, &format!("{old}-3"), None, Some(now)); // old but alive
        make(&d, &format!("{now}-4"), Some("0"), None); // young, finished
        let r = sweep(&d, 3600).unwrap();
        assert_eq!(
            r["removed"],
            json!([format!("{old}-1"), format!("{old}-2")])
        );
        assert_eq!(r["kept"], 2);
        assert!(jobs_root(&d).join(format!("{old}-3")).exists());
    }

    #[cfg(unix)]
    #[test]
    fn job_folders_are_private() {
        use std::os::unix::fs::PermissionsExt;
        let d = scratch_dir("jobs-perm");
        create_private_dir(&jobs_root(&d)).unwrap();
        let mode = fs::metadata(jobs_root(&d)).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o700);
    }
}
