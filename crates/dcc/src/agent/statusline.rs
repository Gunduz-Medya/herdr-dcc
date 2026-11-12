//! The status line command receives a JSON document with model, context and quota information.
//! DCC keeps only the key names and the type of each value (a dotted path and a type name), never a
//! value. That is enough to see which fields a Claude Code version offers. Reading the numbers is
//! Phase 6 and needs the usage tables first.

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use serde_json::{Value, json};

use crate::error::Result;
use crate::util::{create_private_dir, now_ms};

const MAX_DEPTH: usize = 8;

const RESET: &str = "\x1b[0m";
const DIM: &str = "\x1b[90m";
const BLUE: &str = "\x1b[34m";
const WHITE: &str = "\x1b[97m";
const GREEN: &str = "\x1b[32m";
const YELLOW: &str = "\x1b[33m";
const RED: &str = "\x1b[31m";

/// A percentage as the agent reported it, written the way the number is (12.5 stays 12.5), or
/// `?` when the field is missing or null. Nothing is rounded or computed.
fn shown(v: Option<&Value>) -> String {
    match v.and_then(Value::as_f64) {
        Some(x) if x.is_finite() && x >= 0.0 => format!("{x}%"),
        _ => "?".to_string(),
    }
}

/// Only a colour for the number the agent reported: green below 50, yellow below 80, red from
/// 80 on, grey when there is no number. The thresholds are a display choice, nothing is derived.
fn level_colour(v: Option<&Value>) -> &'static str {
    match v.and_then(Value::as_f64) {
        Some(x) if x.is_finite() && x < 50.0 => GREEN,
        Some(x) if x.is_finite() && x < 80.0 => YELLOW,
        Some(x) if x.is_finite() => RED,
        _ => DIM,
    }
}

/// The text the status line shows in the agent's own bar. DCC replaces the user's status line
/// command for the sessions it starts (that is how it receives the numbers), so it draws the same
/// facts itself: folder, model, context and the two quota windows, each exactly as reported.
/// A document that cannot be read gives `dcc`.
pub fn display_line(raw: &str) -> String {
    render(raw, true)
}

/// The same line without colour codes.
pub fn plain_line(raw: &str) -> String {
    render(raw, false)
}

fn render(raw: &str, colour: bool) -> String {
    let Ok(doc) = serde_json::from_str::<Value>(raw) else {
        return "dcc".to_string();
    };
    let text = |path: &[&str]| -> Option<String> {
        let mut v = &doc;
        for key in path {
            v = v.get(key)?;
        }
        v.as_str().map(str::to_string)
    };
    let dir = text(&["workspace", "current_dir"])
        .or_else(|| text(&["cwd"]))
        .map(|d| {
            let home = std::env::var("HOME").unwrap_or_default();
            match d.strip_prefix(&home) {
                Some(rest) if !home.is_empty() => format!("~{rest}"),
                _ => d,
            }
        });
    let get = |a: &str, b: &str| {
        doc.get(a)
            .and_then(|v| v.get(b))
            .and_then(|v| v.get("used_percentage"))
    };
    let ctx = doc
        .get("context_window")
        .and_then(|v| v.get("used_percentage"));
    let paint = |code: &str, t: String| {
        if colour {
            format!("{code}{t}{RESET}")
        } else {
            t
        }
    };
    let mut parts: Vec<String> = Vec::new();
    if let Some(d) = dir {
        parts.push(paint(BLUE, d));
    }
    if let Some(m) = text(&["model", "display_name"]) {
        parts.push(paint(WHITE, m));
    }
    for (label, v) in [
        ("ctx", ctx),
        ("5h", get("rate_limits", "five_hour")),
        ("7d", get("rate_limits", "seven_day")),
    ] {
        let value = paint(level_colour(v), shown(v));
        parts.push(format!("{} {value}", paint(DIM, label.to_string())));
    }
    parts.join(&paint(DIM, " | ".to_string()))
}

fn type_name(v: &Value) -> &'static str {
    match v {
        Value::Null => "null",
        Value::Bool(_) => "bool",
        Value::Number(_) => "number",
        Value::String(_) => "string",
        Value::Array(_) => "array",
        Value::Object(_) => "object",
    }
}

/// A map key that could be data (a file name, an id, a model name with odd characters) becomes
/// `<dyn>`, so the paths never carry anything but field names.
fn safe_segment(key: &str) -> String {
    let plain = !key.is_empty()
        && key.len() <= 40
        && key
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '$')
        && !key.starts_with("toolu_");
    if plain {
        key.to_string()
    } else {
        "<dyn>".to_string()
    }
}

/// Dotted key paths with the type of the value. Arrays add `[]` and describe up to three elements.
pub fn flatten_types(value: &Value) -> BTreeMap<String, String> {
    fn walk(v: &Value, path: &str, depth: usize, out: &mut BTreeMap<String, Vec<&'static str>>) {
        if depth > MAX_DEPTH {
            return;
        }
        match v {
            Value::Object(m) => {
                for (k, child) in m {
                    let p = if path.is_empty() {
                        safe_segment(k)
                    } else {
                        format!("{path}.{}", safe_segment(k))
                    };
                    let types = out.entry(p.clone()).or_default();
                    if !types.contains(&type_name(child)) {
                        types.push(type_name(child));
                    }
                    walk(child, &p, depth + 1, out);
                }
            }
            Value::Array(a) => {
                let p = format!("{path}[]");
                for child in a.iter().take(3) {
                    let types = out.entry(p.clone()).or_default();
                    if !types.contains(&type_name(child)) {
                        types.push(type_name(child));
                    }
                    walk(child, &p, depth + 1, out);
                }
            }
            _ => {}
        }
    }
    let mut acc = BTreeMap::new();
    walk(value, "", 0, &mut acc);
    acc.into_iter()
        .map(|(k, mut v)| {
            v.sort_unstable();
            (k, v.join("|"))
        })
        .collect()
}

/// Writes the latest key paths of a run to `<data>/agents/<run>/statusline-keys.json`.
pub fn record(data_dir: &Path, run_id: &str, raw: &str) -> Result<()> {
    if !super::hooks::valid_session_id(run_id) {
        return Ok(());
    }
    let doc = match serde_json::from_str::<Value>(raw) {
        Ok(v) => json!({ "ts_ms": now_ms(), "paths": flatten_types(&v) }),
        Err(_) => json!({ "ts_ms": now_ms(), "error": "stdin was not JSON" }),
    };
    let dir = data_dir.join("agents").join(run_id);
    create_private_dir(&dir)?;
    fs::write(dir.join("statusline-keys.json"), doc.to_string())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::util::scratch_dir;

    #[test]
    fn paths_and_types_but_no_values() {
        let v: Value = serde_json::from_str(r#"{"a":{"b":"hello-value","c":[1,2],"d":null},"e":true,"f":{"toolu_01ABC":1,"my file.txt":2},"g":[1,"x"]}"#).unwrap();
        let m = flatten_types(&v);
        assert_eq!(
            (
                m["a.b"].as_str(),
                m["a.c"].as_str(),
                m["a.c[]"].as_str(),
                m["e"].as_str()
            ),
            ("string", "array", "number", "bool")
        );
        assert_eq!(m["f.<dyn>"], "number");
        assert_eq!(m["g[]"], "number|string");
        let dump = serde_json::to_string(&m).unwrap();
        for leaked in ["hello-value", "toolu_01ABC", "my file"] {
            assert!(!dump.contains(leaked), "{leaked}");
        }
    }

    #[test]
    fn the_display_line_shows_folder_model_context_and_quota_as_reported() {
        let doc = r#"{"workspace":{"current_dir":"/tmp/project"},"model":{"display_name":"Sonnet 5.5"},
            "context_window":{"used_percentage":46},"rate_limits":{"five_hour":{"used_percentage":82.5},"seven_day":{"used_percentage":null}}}"#;
        assert_eq!(
            plain_line(doc),
            "/tmp/project | Sonnet 5.5 | ctx 46% | 5h 82.5% | 7d ?"
        );
        // the coloured line is the same text with colour codes around the parts
        let coloured = display_line(doc);
        assert_eq!(strip_ansi(&coloured), plain_line(doc));
        assert!(coloured.contains("\x1b[32m46%"), "46 is below 50, green");
        assert!(
            coloured.contains("\x1b[31m82.5%"),
            "82.5 is from 80 on, red"
        );
        assert!(coloured.contains("\x1b[90m?"), "no number is grey");
    }

    fn strip_ansi(s: &str) -> String {
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

    #[test]
    fn colour_follows_the_reported_number_only() {
        for (n, code) in [
            ("0", GREEN),
            ("49.9", GREEN),
            ("50", YELLOW),
            ("79", YELLOW),
            ("80", RED),
            ("100", RED),
        ] {
            let doc = format!(r#"{{"context_window":{{"used_percentage":{n}}}}}"#);
            assert!(display_line(&doc).contains(&format!("{code}{n}%")), "{n}");
        }
    }

    #[test]
    fn missing_fields_show_a_question_mark_and_bad_input_shows_dcc() {
        assert_eq!(plain_line("{}"), "ctx ? | 5h ? | 7d ?");
        assert_eq!(display_line("not json"), "dcc");
        assert_eq!(display_line(""), "dcc");
        // a non number is not shown as a number
        assert!(plain_line(r#"{"context_window":{"used_percentage":"46"}}"#).contains("ctx ?"));
    }

    #[test]
    fn record_writes_a_keys_file_per_run_and_ignores_bad_ids() {
        let d = scratch_dir("statusline");
        record(
            &d,
            "abcdef12-3456",
            r#"{"model":{"id":"m"},"context_window":{"used_percentage":8}}"#,
        )
        .unwrap();
        let text = fs::read_to_string(d.join("agents/abcdef12-3456/statusline-keys.json")).unwrap();
        assert!(text.contains("context_window.used_percentage") && !text.contains("\"m\""));
        record(&d, "../evil", "{}").unwrap();
        assert!(!d.join("agents/../evil").exists());
        record(&d, "abcdef12-3456", "not json").unwrap();
        assert!(
            fs::read_to_string(d.join("agents/abcdef12-3456/statusline-keys.json"))
                .unwrap()
                .contains("not JSON")
        );
    }
}
