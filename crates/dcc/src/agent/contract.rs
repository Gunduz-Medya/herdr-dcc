//! The OSC 7501 contract test. The channel agents use to report their state is not documented
//! (found by experiment, Phase 5), so DCC checks it instead of trusting it. A small fixture agent
//! (`dcc-fixture-agent`) reports idle, working, blocked and done. The test starts it in a terminal
//! of the core, watches the status the core shows and passes when the four states appear in
//! order. The result is stored together with the Herdr versions. When it fails, live agent states
//! turn Unknown (`status_reliable` is false) until a later run passes.

use std::path::Path;
use std::process::Command;
use std::time::{Duration, Instant};

use serde_json::{Value, json};

use crate::error::{Error, Result};
use crate::herdr::api::Herdr;
use crate::store::Store;

pub const EXPECTED: [&str; 4] = ["idle", "working", "blocked", "done"];

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Observed {
    pub state: Option<String>,
    pub kind: Option<String>,
    pub finished: bool,
}

pub trait ContractProbe {
    /// Starts the fixture in a terminal of the core and returns its address.
    fn start(&self, fixture: &Path, mode: &str, step_ms: u64) -> Result<String>;
    fn observe(&self, terminal: &str) -> Result<Observed>;
    fn stop(&self, terminal: &str);
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContractReport {
    pub passed: bool,
    /// The distinct states seen, in order.
    pub seen: Vec<String>,
    /// The `kind` field of the blocked report reached the core.
    pub kind_seen: bool,
}

impl ContractReport {
    pub fn to_json(&self) -> Value {
        json!({ "passed": self.passed, "seen": self.seen, "expected": EXPECTED, "blocked_kind_seen": self.kind_seen })
    }
}

fn is_subsequence(seen: &[String], want: &[&str]) -> bool {
    let mut it = seen.iter();
    want.iter().all(|w| it.any(|s| s == w))
}

pub fn run(
    probe: &dyn ContractProbe,
    fixture: &Path,
    mode: &str,
    step_ms: u64,
    timeout: Duration,
    poll: Duration,
) -> Result<ContractReport> {
    let terminal = probe.start(fixture, mode, step_ms)?;
    let start = Instant::now();
    let mut seen: Vec<String> = Vec::new();
    let mut kind_seen = false;
    loop {
        let o = probe.observe(&terminal)?;
        if let Some(state) = &o.state {
            if seen.last() != Some(state) {
                seen.push(state.clone());
            }
            if state == "blocked" && o.kind.is_some() {
                kind_seen = true;
            }
        }
        if o.finished
            || start.elapsed() > timeout
            || (seen.last().map(String::as_str) == Some("done") && is_subsequence(&seen, &EXPECTED))
        {
            break;
        }
        std::thread::sleep(poll);
    }
    probe.stop(&terminal);
    Ok(ContractReport {
        passed: is_subsequence(&seen, &EXPECTED),
        seen,
        kind_seen,
    })
}

/// Records the result and the reliability flag. Returns the id of the stored run.
pub fn record(
    store: &Store,
    report: &ContractReport,
    herdr_app: &str,
    herdr_core: &str,
) -> Result<i64> {
    store.set_state(
        "status_reliable",
        if report.passed { "true" } else { "false" },
    )?;
    store.record_contract_run(
        herdr_app,
        herdr_core,
        report.passed,
        &report.to_json().to_string(),
    )
}

/// The real probe: `hc run` starts the fixture, `hc get` shows the status the core holds.
pub struct CliProbe<'a> {
    pub herdr: &'a dyn Herdr,
}

impl ContractProbe for CliProbe<'_> {
    fn start(&self, fixture: &Path, mode: &str, step_ms: u64) -> Result<String> {
        let argv = vec![
            fixture.to_string_lossy().to_string(),
            "--mode".into(),
            mode.into(),
            "--step-ms".into(),
            step_ms.to_string(),
        ];
        let cwd = std::env::current_dir()
            .map(|d| d.to_string_lossy().to_string())
            .unwrap_or_else(|_| ".".to_string());
        self.herdr.run_labeled(
            &format!("dcc-contract-{}", crate::util::now_ms() % 100_000),
            &cwd,
            &[],
            &argv,
        )
    }

    fn observe(&self, terminal: &str) -> Result<Observed> {
        let out = Command::new("hc")
            .args(["get", terminal, "--json"])
            .output()
            .map_err(|e| Error::external(format!("cannot run hc: {e}")))?;
        let v: Value = serde_json::from_slice(&out.stdout)?;
        let status = v.get("status");
        let field = |k: &str| {
            status
                .and_then(|s| s.get(k))
                .and_then(Value::as_str)
                .map(str::to_string)
        };
        Ok(Observed {
            state: field("state").or_else(|| status.and_then(Value::as_str).map(str::to_string)),
            kind: field("kind"),
            finished: v.get("state").and_then(Value::as_str) == Some("finished"),
        })
    }

    fn stop(&self, terminal: &str) {
        let _ = self.herdr.kill(terminal);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;

    struct Scripted {
        steps: RefCell<Vec<Observed>>,
        stopped: RefCell<bool>,
    }

    impl Scripted {
        fn new(states: &[(&str, Option<&str>)]) -> Self {
            let mut steps: Vec<Observed> = states
                .iter()
                .map(|(s, k)| Observed {
                    state: Some(s.to_string()),
                    kind: k.map(str::to_string),
                    finished: false,
                })
                .collect();
            steps.reverse();
            Self {
                steps: RefCell::new(steps),
                stopped: RefCell::new(false),
            }
        }
    }

    impl ContractProbe for Scripted {
        fn start(&self, _f: &Path, _m: &str, _s: u64) -> Result<String> {
            Ok("t1".into())
        }
        fn observe(&self, _t: &str) -> Result<Observed> {
            Ok(self.steps.borrow_mut().pop().unwrap_or(Observed {
                finished: true,
                ..Default::default()
            }))
        }
        fn stop(&self, _t: &str) {
            *self.stopped.borrow_mut() = true;
        }
    }

    fn go(p: &Scripted) -> ContractReport {
        run(
            p,
            Path::new("/x"),
            "ok",
            10,
            Duration::from_secs(2),
            Duration::from_millis(1),
        )
        .unwrap()
    }

    #[test]
    fn the_full_sequence_passes_and_the_kind_is_noticed() {
        let p = Scripted::new(&[
            ("idle", None),
            ("idle", None),
            ("working", None),
            ("blocked", Some("permission")),
            ("done", None),
        ]);
        let r = go(&p);
        assert!(r.passed && r.kind_seen);
        assert_eq!(r.seen, vec!["idle", "working", "blocked", "done"]);
        assert!(
            *p.stopped.borrow(),
            "the fixture terminal is always stopped"
        );
    }

    #[test]
    fn a_missing_step_fails() {
        let r = go(&Scripted::new(&[
            ("idle", None),
            ("working", None),
            ("done", None),
        ]));
        assert!(!r.passed);
    }

    #[test]
    fn no_transitions_at_all_fails() {
        let r = go(&Scripted::new(&[]));
        assert!(!r.passed && r.seen.is_empty() && !r.kind_seen);
    }

    #[test]
    fn steps_in_the_wrong_order_fail() {
        assert!(
            !go(&Scripted::new(&[
                ("done", None),
                ("blocked", None),
                ("working", None),
                ("idle", None)
            ]))
            .passed
        );
    }

    #[test]
    fn the_result_decides_the_reliability_flag_and_is_stored() {
        let store = Store::open_in_memory().unwrap();
        let bad = ContractReport {
            passed: false,
            seen: vec![],
            kind_seen: false,
        };
        let id = record(&store, &bad, "0.1.15", "0.1.20").unwrap();
        assert_eq!(
            (id, store.state("status_reliable").unwrap().as_deref()),
            (1, Some("false"))
        );
        let good = ContractReport {
            passed: true,
            seen: EXPECTED.iter().map(|s| s.to_string()).collect(),
            kind_seen: true,
        };
        record(&store, &good, "0.1.15", "0.1.20").unwrap();
        assert_eq!(
            store.state("status_reliable").unwrap().as_deref(),
            Some("true")
        );
    }
}
