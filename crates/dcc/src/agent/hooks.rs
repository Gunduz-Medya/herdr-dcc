//! What a Claude Code hook may leave behind. A hook gets the whole event on stdin, including the
//! prompt text, the last answer and the tool input. This module reads only whitelisted fields
//! (event name, session id, tool name, notification type, session end reason, start source) and
//! validates each one, so nothing else can reach the database or a log.

use serde_json::Value;

use super::db::Event;
use crate::error::{Error, Result};
use crate::store::Store;

const EVENTS: &[&str] = &[
    "SessionStart",
    "UserPromptSubmit",
    "PreToolUse",
    "Notification",
    "Stop",
    "SessionEnd",
];
const REASONS: &[&str] = &[
    "clear",
    "resume",
    "logout",
    "prompt_input_exit",
    "bypass_permissions_disabled",
    "other",
];
const SOURCES: &[&str] = &["startup", "resume", "clear", "compact", "fork"];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HookFacts {
    /// One of the known events, anything else is `Other`.
    pub event: String,
    pub session_id: Option<String>,
    pub tool_name: Option<String>,
    pub notification_type: Option<String>,
    pub reason: Option<String>,
    pub source: Option<String>,
}

pub fn valid_session_id(s: &str) -> bool {
    (8..=64).contains(&s.len()) && s.chars().all(|c| c.is_ascii_hexdigit() || c == '-')
}

fn clean(s: Option<&str>, max: usize, ok: impl Fn(char) -> bool) -> Option<String> {
    s.filter(|v| !v.is_empty() && v.len() <= max && v.chars().all(ok))
        .map(str::to_string)
}

/// Reads the whitelisted facts out of a hook payload. Everything else in the payload is ignored.
pub fn parse(raw: &str) -> Result<HookFacts> {
    let v: Value = serde_json::from_str(raw).map_err(|_| Error::usage("hook input is not JSON"))?;
    let name = v
        .get("hook_event_name")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let event = if EVENTS.contains(&name) {
        name
    } else {
        "Other"
    }
    .to_string();
    let text = |k: &str| v.get(k).and_then(Value::as_str);
    let tool_name = if event == "PreToolUse" {
        clean(text("tool_name"), 80, |c| {
            c.is_ascii_alphanumeric() || matches!(c, '_' | '.' | ':' | '-')
        })
    } else {
        None
    };
    let notification_type = if event == "Notification" {
        clean(text("notification_type"), 48, |c| {
            c.is_ascii_lowercase() || c == '_'
        })
    } else {
        None
    };
    let reason = if event == "SessionEnd" {
        Some(
            text("reason")
                .filter(|r| REASONS.contains(r))
                .unwrap_or("other")
                .to_string(),
        )
    } else {
        None
    };
    let source = if event == "SessionStart" {
        text("source")
            .filter(|s| SOURCES.contains(s))
            .map(str::to_string)
    } else {
        None
    };
    Ok(HookFacts {
        event,
        session_id: text("session_id")
            .filter(|s| valid_session_id(s))
            .map(str::to_string),
        tool_name,
        notification_type,
        reason,
        source,
    })
}

/// Writes one hook event into the records of a run. A session start also ends the sessions that
/// were still open (a new `/clear` session replaces the old one), a session end closes its session.
pub fn record(store: &Store, run_id: &str, f: &HookFacts, ts_ms: i64) -> Result<()> {
    let run = store
        .run(run_id)?
        .ok_or_else(|| Error::not_found(format!("no run {run_id}")))?;
    match (f.event.as_str(), &f.session_id) {
        ("SessionStart", Some(sid)) => {
            let ended_as = if f.source.as_deref() == Some("clear") {
                "clear"
            } else {
                "replaced"
            };
            store.end_open_sessions(run_id, Some(sid), ts_ms, ended_as)?;
            let id_source = if run.launch_session_id.as_deref() == Some(sid) {
                "argv"
            } else {
                "hook"
            };
            store.insert_session(run_id, sid, id_source, ts_ms)?;
        }
        ("SessionEnd", Some(sid)) => store.end_session(
            sid,
            ts_ms,
            &format!("session_end:{}", f.reason.as_deref().unwrap_or("other")),
        )?,
        _ => {}
    }
    store.insert_event(&Event {
        run_id: run_id.to_string(),
        session_id: f.session_id.clone(),
        ts_ms,
        kind: f.event.clone(),
        state: None,
        tool_name: f.tool_name.clone(),
        notification_type: f.notification_type.clone(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::db::Run;

    const PRE: &str = r#"{"session_id":"abcdef12-3456","transcript_path":"/t/x.jsonl","cwd":"/w","hook_event_name":"PreToolUse","tool_name":"Write",
        "tool_input":{"file_path":"/w/SECRET-NAME.txt","content":"top secret body"},"prompt":"do the secret thing"}"#;

    fn store_with_run() -> Store {
        let s = Store::open_in_memory().unwrap();
        s.insert_run(&Run {
            id: "abcdef12-3456".into(),
            agent_kind: "claude".into(),
            launch_mode: "dcc".into(),
            launch_session_id: Some("abcdef12-3456".into()),
            started_ms: 1,
            last_state: "Unknown".into(),
            last_state_ms: 1,
            ..Default::default()
        })
        .unwrap();
        s.insert_session("abcdef12-3456", "abcdef12-3456", "argv", 1)
            .unwrap();
        s
    }

    #[test]
    fn only_whitelisted_facts_are_read() {
        let f = parse(PRE).unwrap();
        assert_eq!(
            (
                f.event.as_str(),
                f.tool_name.as_deref(),
                f.session_id.as_deref()
            ),
            ("PreToolUse", Some("Write"), Some("abcdef12-3456"))
        );
        let s = Store::open_in_memory().unwrap();
        let dump = format!("{f:?}");
        for leaked in [
            "SECRET-NAME",
            "top secret body",
            "do the secret thing",
            "/w/",
            "x.jsonl",
        ] {
            assert!(!dump.contains(leaked), "{leaked}");
        }
        drop(s);
    }

    #[test]
    fn hostile_values_are_dropped() {
        let f = parse(r#"{"hook_event_name":"PreToolUse","tool_name":"Write; rm -rf /","session_id":"../../etc/passwd"}"#).unwrap();
        assert_eq!((f.tool_name, f.session_id), (None, None));
        let f = parse(r#"{"hook_event_name":"Evil","session_id":"abcdef12"}"#).unwrap();
        assert_eq!(f.event, "Other");
        let f =
            parse(r#"{"hook_event_name":"Notification","notification_type":"Permission Prompt!"}"#)
                .unwrap();
        assert_eq!(f.notification_type, None);
        let f = parse(r#"{"hook_event_name":"SessionEnd","reason":"made up"}"#).unwrap();
        assert_eq!(f.reason.as_deref(), Some("other"));
        assert!(parse("not json").is_err());
    }

    #[test]
    fn clear_makes_a_second_session_under_the_same_run() {
        let s = store_with_run();
        let start = |sid: &str, source: &str, ts| {
            let raw = format!(
                r#"{{"hook_event_name":"SessionStart","session_id":"{sid}","source":"{source}"}}"#
            );
            record(&s, "abcdef12-3456", &parse(&raw).unwrap(), ts).unwrap();
        };
        start("abcdef12-3456", "startup", 10);
        let end = parse(
            r#"{"hook_event_name":"SessionEnd","session_id":"abcdef12-3456","reason":"clear"}"#,
        )
        .unwrap();
        record(&s, "abcdef12-3456", &end, 20).unwrap();
        start("99999999-aaaa", "clear", 21);
        let sessions = s.sessions_of("abcdef12-3456").unwrap();
        assert_eq!(sessions.len(), 2);
        assert_eq!(
            (
                sessions[0].id_source.as_str(),
                sessions[0].end_source.as_deref()
            ),
            ("argv", Some("session_end:clear"))
        );
        assert_eq!(
            (sessions[1].id_source.as_str(), sessions[1].ended_ms),
            ("hook", None)
        );
        assert_eq!(s.events_of("abcdef12-3456").unwrap().len(), 3);
    }

    #[test]
    fn a_new_start_closes_an_old_open_session_even_without_a_session_end() {
        let s = store_with_run();
        let raw =
            r#"{"hook_event_name":"SessionStart","session_id":"bbbbbbbb-1111","source":"resume"}"#;
        record(&s, "abcdef12-3456", &parse(raw).unwrap(), 30).unwrap();
        let sessions = s.sessions_of("abcdef12-3456").unwrap();
        assert_eq!(sessions[0].end_source.as_deref(), Some("replaced"));
    }

    #[test]
    fn an_unknown_run_is_not_found() {
        let s = Store::open_in_memory().unwrap();
        assert_eq!(
            record(&s, "nope", &parse(PRE).unwrap(), 1)
                .unwrap_err()
                .kind,
            "not_found"
        );
    }
}
