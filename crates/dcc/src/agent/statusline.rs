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
