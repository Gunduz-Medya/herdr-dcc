//! Context and account quota as the agent's own status line reports them. Nothing here computes,
//! estimates or infers a number. A value is stored exactly as it arrived (percentage as a number
//! or null, reset time converted from the documented epoch seconds to milliseconds), a missing
//! field stores nothing, and a null stays unknown. Only DCC started Claude Code runs have a
//! source. Every other agent is "unavailable".

use rusqlite::params;
use serde_json::{Value, json};

use crate::agent::hooks::valid_session_id;
use crate::error::Result;
use crate::store::Store;
use crate::util::now_ms;

pub const SOURCE: &str = "statusline";
/// Quota windows Claude Code documents, as they are named in `rate_limits`.
pub const QUOTA_WINDOWS: &[&str] = &["five_hour", "seven_day", "spend_limit"];
const KEEP_MS: i64 = 30 * 24 * 3600 * 1000;
/// An unchanged value is written again only after this long, so the history has a point per minute.
const REPEAT_MS: i64 = 60_000;

/// One observation taken from a status line document.
#[derive(Debug, Clone, PartialEq)]
pub struct Observation {
    pub kind: &'static str,
    pub window: &'static str,
    /// `None` when the field was present and null. The agent has not reported a number yet.
    pub used_percentage: Option<f64>,
    pub resets_at_ms: Option<i64>,
}

fn percentage(v: &Value) -> Option<Option<f64>> {
    match v {
        Value::Null => Some(None),
        Value::Number(n) => {
            let x = n.as_f64()?;
            // not a plausible percentage (the spend limit may pass 100, nothing passes 10000)
            (x.is_finite() && (0.0..=10_000.0).contains(&x)).then_some(Some(x))
        }
        _ => None,
    }
}

/// What the document reports. Fields that are absent or of an unexpected type give nothing.
pub fn observations(doc: &Value) -> Vec<Observation> {
    let mut out = Vec::new();
    if let Some(p) = doc
        .get("context_window")
        .and_then(|c| c.get("used_percentage"))
        && let Some(used) = percentage(p)
    {
        out.push(Observation {
            kind: "context",
            window: "session",
            used_percentage: used,
            resets_at_ms: None,
        });
    }
    if let Some(limits) = doc.get("rate_limits").filter(|l| l.is_object()) {
        for window in QUOTA_WINDOWS {
            let Some(w) = limits.get(*window).filter(|w| w.is_object()) else {
                continue;
            };
            let Some(used) = w.get("used_percentage").and_then(percentage) else {
                continue;
            };
            let resets = w
                .get("resets_at")
                .and_then(Value::as_i64)
                .and_then(|s| s.checked_mul(1000));
            out.push(Observation {
                kind: "quota",
                window,
                used_percentage: used,
                resets_at_ms: resets,
            });
        }
    }
    out
}

impl Store {
    fn session_exists(&self, id: &str) -> Result<bool> {
        Ok(self
            .conn()
            .query_row(
                "SELECT 1 FROM agent_sessions WHERE id = ?1",
                params![id],
                |_| Ok(()),
            )
            .is_ok())
    }

    fn run_exists(&self, id: &str) -> Result<bool> {
        Ok(self
            .conn()
            .query_row(
                "SELECT 1 FROM agent_runs WHERE id = ?1",
                params![id],
                |_| Ok(()),
            )
            .is_ok())
    }

    /// Stores what the status line document reports for a run. Returns how many rows were written.
    pub fn record_usage(&self, run_id: &str, raw: &str, now: i64) -> Result<usize> {
        let Ok(doc) = serde_json::from_str::<Value>(raw) else {
            return Ok(0);
        };
        let obs = observations(&doc);
        if obs.is_empty() || !self.run_exists(run_id)? {
            return Ok(0);
        }
        let session = doc
            .get("session_id")
            .and_then(Value::as_str)
            .filter(|s| valid_session_id(s))
            .filter(|s| self.session_exists(s).unwrap_or(false))
            .map(str::to_string);
        let mut written = 0;
        for o in &obs {
            let last: Option<(i64, Option<f64>, Option<i64>)> = self
                .conn()
                .query_row(
                    "SELECT ts_ms, used_percentage, resets_at_ms FROM usage_samples
                     WHERE run_id = ?1 AND kind = ?2 AND window = ?3 ORDER BY id DESC LIMIT 1",
                    params![run_id, o.kind, o.window],
                    |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
                )
                .ok();
            let same = last.is_some_and(|(ts, used, resets)| {
                used == o.used_percentage && resets == o.resets_at_ms && now - ts < REPEAT_MS
            });
            if same {
                continue;
            }
            self.conn().execute(
                "INSERT INTO usage_samples (ts_ms, run_id, session_id, kind, window, used_percentage, resets_at_ms, source)
                 VALUES (?1,?2,?3,?4,?5,?6,?7,?8)",
                params![now, run_id, session, o.kind, o.window, o.used_percentage, o.resets_at_ms, SOURCE],
            )?;
            written += 1;
        }
        self.conn().execute(
            "DELETE FROM usage_samples WHERE ts_ms < ?1",
            params![now - KEEP_MS],
        )?;
        Ok(written)
    }
}

fn sample_json(ts: i64, used: Option<f64>, resets: Option<i64>) -> Value {
    json!({ "sampled_ms": ts, "used_percentage": used, "resets_at_ms": resets,
            "status": if used.is_some() { "known" } else { "unknown" } })
}

/// The newest sample of a window across all runs (the quota belongs to the account), as it was
/// stored. When the reset time of the sample has passed, that is said, the value stays as sampled.
fn latest_quota(store: &Store, window: &str, now: i64) -> Result<Value> {
    let row: Option<(i64, Option<f64>, Option<i64>, String)> = store
        .conn()
        .query_row(
            "SELECT ts_ms, used_percentage, resets_at_ms, run_id FROM usage_samples
             WHERE kind = 'quota' AND window = ?1 ORDER BY ts_ms DESC, id DESC LIMIT 1",
            params![window],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
        )
        .ok();
    Ok(match row {
        None => json!({ "window": window, "status": "unavailable" }),
        Some((ts, used, resets, run)) => {
            let mut v = sample_json(ts, used, resets);
            v["window"] = json!(window);
            v["run_id"] = json!(run);
            v["reset_passed"] = json!(resets.is_some_and(|r| r <= now));
            v
        }
    })
}

/// Context per open run and the quota windows. Two separate parts. A run DCC did not start, or an
/// agent that is not Claude Code, has no source and says so.
pub fn show(store: &Store, now: i64) -> Result<Value> {
    let mut contexts = Vec::new();
    for run in store.open_runs()? {
        let has_source = run.launch_mode == "dcc" && run.agent_kind == "claude";
        let row: Option<(i64, Option<f64>)> = if has_source {
            store
                .conn()
                .query_row(
                    "SELECT ts_ms, used_percentage FROM usage_samples
                     WHERE run_id = ?1 AND kind = 'context' ORDER BY id DESC LIMIT 1",
                    params![run.id],
                    |r| Ok((r.get(0)?, r.get(1)?)),
                )
                .ok()
        } else {
            None
        };
        let mut v = match row {
            Some((ts, used)) => sample_json(ts, used, None),
            None => json!({ "status": "unavailable" }),
        };
        v["run_id"] = json!(run.id);
        v["agent"] = json!(run.agent_kind);
        v["reason"] = json!(match (has_source, v["status"].as_str()) {
            (false, _) => "no official source for this agent or it was not started by DCC",
            (true, Some("unavailable")) => "no status line report received yet",
            (true, Some("unknown")) => "the agent has not reported a number yet",
            _ => "",
        });
        contexts.push(v);
    }
    let mut windows = Vec::new();
    for w in QUOTA_WINDOWS {
        windows.push(latest_quota(store, w, now)?);
    }
    Ok(json!({
        "generated_ms": now,
        "context": contexts,
        "quota": { "windows": windows, "source": SOURCE },
    }))
}

/// Stored samples of one kind, oldest first, for the history view.
pub fn history(
    store: &Store,
    kind: &str,
    window: Option<&str>,
    since_ms: i64,
    limit: i64,
) -> Result<Value> {
    let mut st = store.conn().prepare(
        "SELECT ts_ms, run_id, window, used_percentage, resets_at_ms FROM (
             SELECT * FROM usage_samples
             WHERE kind = ?1 AND ts_ms >= ?2 AND (?3 IS NULL OR window = ?3)
             ORDER BY ts_ms DESC, id DESC LIMIT ?4
         ) ORDER BY ts_ms, id",
    )?;
    let rows = st.query_map(params![kind, since_ms, window, limit], |r| {
        Ok(json!({
            "sampled_ms": r.get::<_, i64>(0)?, "run_id": r.get::<_, String>(1)?,
            "window": r.get::<_, String>(2)?, "used_percentage": r.get::<_, Option<f64>>(3)?,
            "resets_at_ms": r.get::<_, Option<i64>>(4)?,
        }))
    })?;
    let samples: Vec<Value> = rows.collect::<rusqlite::Result<_>>()?;
    Ok(
        json!({ "kind": kind, "since_ms": since_ms, "samples": samples,
               "note": "only samples taken while DCC was running" }),
    )
}

pub fn default_since(now: i64) -> i64 {
    now - 24 * 3600 * 1000
}

pub fn now() -> i64 {
    now_ms()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::db::tests::run as new_run;

    fn store_with_run(mode: &str, kind: &str) -> (Store, String) {
        let s = Store::open_in_memory().unwrap();
        let mut r = new_run("abcdef12-0000");
        r.launch_mode = mode.into();
        r.agent_kind = kind.into();
        s.insert_run(&r).unwrap();
        (s, r.id)
    }

    const DOC: &str = r#"{"session_id":"abcdef12-0000","context_window":{"used_percentage":8.5,"context_window_size":200000,"current_usage":{"input_tokens":17000}},
        "rate_limits":{"five_hour":{"used_percentage":23.5,"resets_at":1738425600},"seven_day":{"used_percentage":null,"resets_at":1738857600}}}"#;

    #[test]
    fn values_are_taken_as_reported_and_the_reset_is_seconds_to_milliseconds() {
        let o = observations(&serde_json::from_str(DOC).unwrap());
        assert_eq!(o.len(), 3);
        assert_eq!(
            (o[0].kind, o[0].window, o[0].used_percentage),
            ("context", "session", Some(8.5))
        );
        assert_eq!(
            (o[1].window, o[1].used_percentage, o[1].resets_at_ms),
            ("five_hour", Some(23.5), Some(1_738_425_600_000))
        );
        assert_eq!((o[2].window, o[2].used_percentage), ("seven_day", None));
    }

    #[test]
    fn nothing_is_computed_from_other_fields() {
        // tokens and sizes are there, the percentage is not: no row, no derived number
        let doc = r#"{"context_window":{"context_window_size":200000,"current_usage":{"input_tokens":50000,"cache_read_input_tokens":1}},
                      "cost":{"total_cost_usd":1.5},"rate_limits":{"five_hour":{"resets_at":1738425600}}}"#;
        assert!(observations(&serde_json::from_str(doc).unwrap()).is_empty());
        let (s, run) = store_with_run("dcc", "claude");
        assert_eq!(s.record_usage(&run, doc, 1000).unwrap(), 0);
        assert!(
            s.conn()
                .query_row("SELECT COUNT(*) FROM usage_samples", [], |r| r
                    .get::<_, i64>(0))
                .unwrap()
                == 0
        );
    }

    #[test]
    fn bad_values_and_types_store_nothing() {
        for doc in [
            r#"{"context_window":{"used_percentage":"12"}}"#,
            r#"{"context_window":{"used_percentage":-4}}"#,
            r#"{"context_window":{"used_percentage":1e9}}"#,
            r#"{"rate_limits":"none"}"#,
            r#"{"rate_limits":{"five_hour":5}}"#,
            r#"{"rate_limits":{"mystery_window":{"used_percentage":5}}}"#,
            "not json",
        ] {
            let (s, run) = store_with_run("dcc", "claude");
            assert_eq!(s.record_usage(&run, doc, 1000).unwrap(), 0, "{doc}");
        }
    }

    #[test]
    fn show_keeps_context_and_quota_apart_and_a_null_stays_unknown() {
        let (s, run) = store_with_run("dcc", "claude");
        assert_eq!(s.record_usage(&run, DOC, 1_738_400_000_000).unwrap(), 3);
        let v = show(&s, 1_738_400_001_000).unwrap();
        assert_eq!(v["context"][0]["used_percentage"], 8.5);
        assert_eq!(v["context"][0]["status"], "known");
        let w = &v["quota"]["windows"];
        assert_eq!(
            (
                w[0]["window"].as_str(),
                w[0]["used_percentage"].as_f64(),
                w[0]["status"].as_str()
            ),
            (Some("five_hour"), Some(23.5), Some("known"))
        );
        assert_eq!(
            (
                w[1]["window"].as_str(),
                w[1]["used_percentage"].is_null(),
                w[1]["status"].as_str()
            ),
            (Some("seven_day"), true, Some("unknown"))
        );
        assert_eq!(
            w[2]["status"], "unavailable",
            "a window never reported is unavailable"
        );
        assert!(v["context"].is_array() && v["quota"].is_object());
    }

    #[test]
    fn an_agent_without_a_source_is_unavailable() {
        let (s, run) = store_with_run("external", "claude");
        // even if something wrote a sample for it, an external run has no official source here
        s.conn().execute("INSERT INTO usage_samples (ts_ms, run_id, kind, window, used_percentage, source) VALUES (1,?1,'context','session',50,'statusline')", params![run]).unwrap();
        let v = show(&s, 10).unwrap();
        assert_eq!(v["context"][0]["status"], "unavailable");
        assert!(
            v["context"][0]["reason"]
                .as_str()
                .unwrap()
                .contains("not started by DCC")
        );
        let (s2, _) = store_with_run("dcc", "codex");
        assert_eq!(
            show(&s2, 10).unwrap()["context"][0]["status"],
            "unavailable"
        );
        let (s3, _) = store_with_run("dcc", "claude");
        let v3 = show(&s3, 10).unwrap();
        assert_eq!(v3["context"][0]["status"], "unavailable");
        assert!(
            v3["context"][0]["reason"]
                .as_str()
                .unwrap()
                .contains("no status line report")
        );
    }

    #[test]
    fn a_reset_that_has_passed_is_said_and_the_value_is_not_changed() {
        let (s, run) = store_with_run("dcc", "claude");
        s.record_usage(&run, DOC, 1_738_400_000_000).unwrap();
        let v = show(&s, 1_738_500_000_000).unwrap();
        let five = &v["quota"]["windows"][0];
        assert_eq!(five["reset_passed"], true);
        assert_eq!(five["used_percentage"], 23.5);
    }

    #[test]
    fn repeats_are_thinned_and_changes_are_kept() {
        let (s, run) = store_with_run("dcc", "claude");
        assert_eq!(s.record_usage(&run, DOC, 1_000_000).unwrap(), 3);
        assert_eq!(
            s.record_usage(&run, DOC, 1_010_000).unwrap(),
            0,
            "same values within a minute"
        );
        assert_eq!(
            s.record_usage(&run, DOC, 1_070_000).unwrap(),
            3,
            "a minute later"
        );
        let changed = DOC.replace("8.5", "9.0");
        assert_eq!(
            s.record_usage(&run, &changed, 1_071_000).unwrap(),
            1,
            "only the context value changed"
        );
        let h = history(&s, "context", None, 0, 100).unwrap();
        let used: Vec<f64> = h["samples"]
            .as_array()
            .unwrap()
            .iter()
            .map(|x| x["used_percentage"].as_f64().unwrap())
            .collect();
        assert_eq!(used, vec![8.5, 8.5, 9.0]);
    }

    #[test]
    fn old_samples_are_removed_and_a_missing_run_is_ignored() {
        let (s, run) = store_with_run("dcc", "claude");
        s.record_usage(&run, DOC, 1_000).unwrap();
        s.record_usage(&run, &DOC.replace("8.5", "9.5"), 1_000 + KEEP_MS + 5)
            .unwrap();
        let n: i64 = s
            .conn()
            .query_row(
                "SELECT COUNT(*) FROM usage_samples WHERE ts_ms = 1000",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(n, 0);
        assert_eq!(s.record_usage("00000000-0000", DOC, 5).unwrap(), 0);
    }

    #[test]
    fn the_session_is_linked_only_when_it_is_known() {
        let (s, run) = store_with_run("dcc", "claude");
        s.record_usage(&run, DOC, 1).unwrap();
        let sid: Option<String> = s
            .conn()
            .query_row("SELECT session_id FROM usage_samples LIMIT 1", [], |r| {
                r.get(0)
            })
            .unwrap();
        assert_eq!(
            sid, None,
            "the session abcdef12-0000 has no row in this store"
        );
        s.conn().execute("INSERT INTO agent_sessions (id, run_id, started_ms, id_source) VALUES ('abcdef12-0000', ?1, 1, 'argv')", params![run]).unwrap();
        s.record_usage(&run, &DOC.replace("8.5", "9.1"), 100)
            .unwrap();
        let sid: Option<String> = s
            .conn()
            .query_row(
                "SELECT session_id FROM usage_samples ORDER BY id DESC LIMIT 1",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(sid.as_deref(), Some("abcdef12-0000"));
    }
}
