//! Core life cycle helpers. `hc server status` is NOT a liveness probe: on Unix it starts a core
//! when none runs (found in the spike). `pids` is for a core that is expected to be running.
//! Shutdown is verified by process id, never by asking the core.

use std::process::Command;

use serde_json::{Value, json};

use crate::error::{Error, Result};
use crate::util::process_alive;

/// Core and holder pids of the running core. Starts one if none runs, so call it only on purpose.
pub fn pids() -> Result<Vec<u32>> {
    let out = Command::new("hc")
        .args(["server", "status"])
        .output()
        .map_err(|e| Error::external(format!("cannot run hc: {e}")))?;
    if !out.status.success() {
        return Err(Error::external("hc server status failed"));
    }
    let v: Value = serde_json::from_slice(&out.stdout)?;
    Ok(pids_from_status(&v))
}

pub fn pids_from_status(v: &Value) -> Vec<u32> {
    let mut pids: Vec<u32> = Vec::new();
    if let Some(p) = v.get("pid").and_then(Value::as_u64) {
        pids.push(p as u32);
    }
    for h in v
        .get("holders")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        if let Some(p) = h.get("pid").and_then(Value::as_u64) {
            pids.push(p as u32);
        }
    }
    pids.sort_unstable();
    pids.dedup();
    pids
}

pub fn stop() -> Result<Value> {
    let out = Command::new("hc")
        .args(["server", "stop"])
        .output()
        .map_err(|e| Error::external(format!("cannot run hc: {e}")))?;
    if !out.status.success() {
        return Err(Error::external(format!(
            "hc server stop: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        )));
    }
    Ok(serde_json::from_slice(&out.stdout).unwrap_or(json!({"stopped": true})))
}

/// Which of these processes still run. Empty means all are gone.
pub fn alive_of(pids: &[u32]) -> Result<Vec<u32>> {
    let mut alive = Vec::new();
    for p in pids {
        if process_alive(*p)? {
            alive.push(*p);
        }
    }
    Ok(alive)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pids_from_a_status_document() {
        let v = json!({"pid": 10, "holder": {"pid": 12}, "holders": [{"pid": 12}, {"pid": 13}]});
        assert_eq!(pids_from_status(&v), vec![10, 12, 13]);
        assert!(pids_from_status(&json!({})).is_empty());
    }

    #[test]
    fn a_dead_pid_is_reported_gone() {
        // pid 0x7fff_fff0 is far above any pid a test machine hands out
        assert!(alive_of(&[0x7fff_fff0]).unwrap().is_empty());
        assert_eq!(alive_of(&[std::process::id()]).unwrap().len(), 1);
    }
}
