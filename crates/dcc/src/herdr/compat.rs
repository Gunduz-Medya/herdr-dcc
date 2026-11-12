//! Which Herdr versions DCC accepts. The agent status channel (OSC 7501) is not documented, so a
//! version outside the list gets a warning and the contract test. The list lives in
//! `compat.toml`, a tiny subset of TOML (`[[supported]]` blocks with `app` and `core`).

use std::process::Command;

use crate::error::{Error, Result};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pair {
    pub app: String,
    pub core: String,
}

type Partial = Option<(Option<String>, Option<String>)>;

fn flush(current: &mut Partial, out: &mut Vec<Pair>) {
    if let Some((Some(app), Some(core))) = current.take() {
        out.push(Pair { app, core });
    }
}

pub fn parse(text: &str) -> Vec<Pair> {
    let mut pairs = Vec::new();
    let mut current: Partial = None;
    for line in text.lines().map(str::trim) {
        if line == "[[supported]]" {
            flush(&mut current, &mut pairs);
            current = Some((None, None));
        } else if let Some((key, value)) = line.split_once('=') {
            let value = value.trim().trim_matches('"').to_string();
            if let Some(cur) = current.as_mut() {
                match key.trim() {
                    "app" => cur.0 = Some(value),
                    "core" => cur.1 = Some(value),
                    _ => {}
                }
            }
        }
    }
    flush(&mut current, &mut pairs);
    pairs
}

pub fn is_supported(pairs: &[Pair], app: &str, core: &str) -> bool {
    pairs.iter().any(|p| p.app == app && p.core == core)
}

/// The last word of `<program> --version`, for example `0.1.15` from `herdr-app 0.1.15`.
pub fn program_version(program: &str) -> Result<String> {
    let out = Command::new(program)
        .arg("--version")
        .output()
        .map_err(|e| Error::external(format!("cannot run {program}: {e}")))?;
    let text = String::from_utf8_lossy(&out.stdout).to_string();
    text.split_whitespace()
        .last()
        .map(str::to_string)
        .ok_or_else(|| Error::external(format!("{program} --version printed nothing")))
}

#[cfg(test)]
mod tests {
    use super::*;

    const TEXT: &str = "# accepted pairs\n[[supported]]\napp = \"0.1.15\"\ncore = \"0.1.20\"\n\n[[supported]]\napp = \"0.1.16\"\ncore = \"0.1.20\"\n[[supported]]\napp = \"broken\"\n";

    #[test]
    fn parses_blocks_and_skips_incomplete_ones() {
        let p = parse(TEXT);
        assert_eq!(p.len(), 2);
        assert!(is_supported(&p, "0.1.15", "0.1.20"));
        assert!(!is_supported(&p, "0.1.15", "0.1.21"));
        assert!(!is_supported(&p, "broken", "0.1.20"));
    }
}
