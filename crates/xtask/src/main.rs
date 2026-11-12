//! Development tasks. Run with the isolated environment loaded (`DCC_REQUIRE_ISOLATION=1`,
//! `HERDR_APP_HOME`, `HC_SESSION`, XDG folders). The tasks refuse to run without it.
//!
//!   cargo run -p xtask -- env             print the environment lines (for a POSIX shell)
//!   cargo run -p xtask -- dev-up          start the isolated Herdr window and record its pids
//!   cargo run -p xtask -- dev-down        end its terminals, quit it, stop the core, verify by pid
//!   cargo run -p xtask -- leak-scan DIR   scan for a value read from stdin (never printed)
//!   cargo run -p xtask -- secret-leak     store a fake secret through the hidden prompt and scan
//!
//! The pty trick in `secret-leak` uses the BSD `script` command, so it is macOS only for now.

use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode, Stdio};
use std::time::{Duration, Instant};

use dcc::herdr::api::{CliHerdr, Herdr};
use dcc::herdr::{core, isolation};
use dcc::util::{now_ms, process_alive};
use serde_json::{Value, json};

type R<T> = Result<T, String>;

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .canonicalize()
        .expect("repo root")
}

fn need_isolation() -> R<()> {
    isolation::require().map_err(|e| e.message)?;
    if !isolation::required() {
        return Err("DCC_REQUIRE_ISOLATION=1 is not set. Load the dev environment first (cargo run -p xtask -- env).".into());
    }
    Ok(())
}

fn dev_dir() -> PathBuf {
    root().join(".dev")
}

fn pids_file() -> PathBuf {
    dev_dir().join("dev-pids.json")
}

fn env_lines() -> String {
    let d = dev_dir();
    let p = |s: &str| d.join(s).to_string_lossy().to_string();
    [
        format!("export HERDR_APP_HOME={}", p("scratch-home")),
        "export HC_SESSION=dcc-dev".to_string(),
        format!("export XDG_CONFIG_HOME={}", p("scratch-xdg/config")),
        format!("export XDG_DATA_HOME={}", p("scratch-xdg/data")),
        format!("export XDG_STATE_HOME={}", p("scratch-xdg/state")),
        format!("export XDG_CACHE_HOME={}", p("scratch-xdg/cache")),
        format!("export DCC_DATA_DIR={}", p("scratch-data")),
        "export DCC_REQUIRE_ISOLATION=1".to_string(),
    ]
    .join("\n")
}

fn dev_up() -> R<Value> {
    need_isolation()?;
    let state = isolation::state_home().ok_or("no state folder")?;
    let session = isolation::current().session.ok_or("no session")?;
    isolation::check_socket_len(&state, &session).map_err(|e| e.message)?;
    let log = fs::File::create(dev_dir().join("scratch-app.log")).map_err(|e| e.to_string())?;
    let child = Command::new("herdr-app")
        .stdin(Stdio::null())
        .stdout(log.try_clone().map_err(|e| e.to_string())?)
        .stderr(log)
        .spawn()
        .map_err(|e| format!("cannot start herdr-app: {e}"))?;
    let app = child.id();
    drop(child);
    let start = Instant::now();
    loop {
        let ok = Command::new("herdr-app")
            .arg("status")
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false);
        if ok {
            break;
        }
        if start.elapsed() > Duration::from_secs(20) {
            return Err("the window did not answer `herdr-app status` in 20 s".into());
        }
        std::thread::sleep(Duration::from_millis(300));
    }
    // The app started its own core, so asking for its pids does not start another one.
    let core_pids = core::pids().map_err(|e| e.message)?;
    let doc = json!({ "app": app, "core": core_pids, "started_ms": now_ms() });
    fs::write(pids_file(), doc.to_string()).map_err(|e| e.to_string())?;
    Ok(doc)
}

fn dev_down() -> R<Value> {
    need_isolation()?;
    let doc: Value = serde_json::from_slice(
        &fs::read(pids_file()).map_err(|e| format!("no {}: {e}", pids_file().display()))?,
    )
    .map_err(|e| e.to_string())?;
    let mut pids: Vec<u32> = doc["core"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|p| p.as_u64().map(|p| p as u32))
        .collect();
    pids.push(doc["app"].as_u64().ok_or("no app pid")? as u32);
    let herdr = CliHerdr::new().map_err(|e| e.message)?;
    let mut killed = Vec::new();
    for t in herdr.terminals().map_err(|e| e.message)? {
        if herdr.kill(&t.address).is_ok() {
            killed.push(t.address);
        }
    }
    let _ = Command::new("herdr-app").args(["do", "app.quit"]).output();
    std::thread::sleep(Duration::from_secs(2));
    let stopped = core::stop().map_err(|e| e.message)?;
    let start = Instant::now();
    let alive = loop {
        let alive = core::alive_of(&pids).map_err(|e| e.message)?;
        if alive.is_empty() || start.elapsed() > Duration::from_secs(10) {
            break alive;
        }
        std::thread::sleep(Duration::from_millis(300));
    };
    if alive.is_empty() {
        Ok(
            json!({ "closed": true, "checked": pids, "terminals_killed": killed, "core_stop": stopped }),
        )
    } else {
        Err(format!("still running after the shutdown: {alive:?}"))
    }
}

const B64: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

fn base64(data: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    for chunk in data.chunks(3) {
        let n = (u32::from(chunk[0]) << 16)
            | (u32::from(*chunk.get(1).unwrap_or(&0)) << 8)
            | u32::from(*chunk.get(2).unwrap_or(&0));
        out.push(B64[(n >> 18) as usize & 63]);
        out.push(B64[(n >> 12) as usize & 63]);
        out.push(if chunk.len() > 1 {
            B64[(n >> 6) as usize & 63]
        } else {
            b'='
        });
        out.push(if chunk.len() > 2 {
            B64[n as usize & 63]
        } else {
            b'='
        });
    }
    out
}

fn contains(hay: &[u8], needle: &[u8]) -> bool {
    !needle.is_empty() && hay.windows(needle.len()).any(|w| w == needle)
}

/// The forms searched for. Base64 of a value in a longer text can start at three offsets, so the
/// value is also encoded behind one and two filler bytes and the filler part is cut off.
fn patterns(value: &[u8]) -> (Vec<u8>, Vec<Vec<u8>>) {
    let mut forms = vec![base64(value)];
    for filler in 1..=2usize {
        let mut padded = vec![b'x'; filler];
        padded.extend_from_slice(value);
        let enc = base64(&padded);
        let skip = (filler * 4).div_ceil(3).min(enc.len());
        let trimmed: Vec<u8> = enc[skip..].iter().copied().filter(|b| *b != b'=').collect();
        let cut = trimmed.len().saturating_sub(trimmed.len() % 4 + 4);
        if cut > 4 {
            forms.push(trimmed[..cut].to_vec());
        }
    }
    (value.to_vec(), forms)
}

#[derive(Default)]
struct Hits {
    plain: Vec<String>,
    base64: Vec<String>,
    scanned: usize,
}

fn scan_dir(dir: &Path, value: &[u8], hits: &mut Hits) {
    let (plain, forms) = patterns(value);
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().to_string();
        let Ok(kind) = entry.file_type() else {
            continue;
        };
        if kind.is_dir() {
            if name != "target" && name != "rust" && name != ".git" {
                scan_dir(&path, value, hits);
            }
        } else if kind.is_file() {
            let Ok(meta) = entry.metadata() else { continue };
            if meta.len() > 64 * 1024 * 1024 {
                continue;
            }
            let Ok(bytes) = fs::read(&path) else { continue };
            hits.scanned += 1;
            if contains(&bytes, &plain) {
                hits.plain.push(path.to_string_lossy().to_string());
            }
            if forms.iter().any(|f| contains(&bytes, f)) {
                hits.base64.push(path.to_string_lossy().to_string());
            }
        }
    }
}

fn argv_hits(value: &[u8]) -> usize {
    let out = Command::new("ps")
        .args(["-axww", "-o", "args="])
        .output()
        .map(|o| o.stdout)
        .unwrap_or_default();
    out.split(|b| *b == b'\n')
        .filter(|line| contains(line, value))
        .count()
}

fn screen_hits(value: &[u8]) -> R<usize> {
    let herdr = CliHerdr::new().map_err(|e| e.message)?;
    let mut n = 0;
    for t in herdr.terminals().map_err(|e| e.message)? {
        let out = Command::new("hc")
            .args(["read", "--lines", "200", &t.address])
            .output()
            .map_err(|e| e.to_string())?;
        let (plain, forms) = patterns(value);
        if contains(&out.stdout, &plain) || forms.iter().any(|f| contains(&out.stdout, f)) {
            n += 1;
        }
    }
    Ok(n)
}

fn leak_scan(dir: &Path, screens: bool) -> R<Value> {
    let mut value = String::new();
    std::io::stdin()
        .read_to_string(&mut value)
        .map_err(|e| e.to_string())?;
    let value = value.trim_end_matches(['\n', '\r']).as_bytes().to_vec();
    if value.len() < 8 {
        return Err(
            "the value on stdin must be at least 8 characters, a shorter one would match by chance"
                .into(),
        );
    }
    let mut hits = Hits::default();
    scan_dir(dir, &value, &mut hits);
    let argv = argv_hits(&value);
    let screen = if screens {
        Some(screen_hits(&value)?)
    } else {
        None
    };
    let clean =
        hits.plain.is_empty() && hits.base64.is_empty() && argv == 0 && screen.unwrap_or(0) == 0;
    let doc = json!({ "clean": clean, "files_scanned": hits.scanned, "plain_hit_files": hits.plain, "base64_hit_files": hits.base64, "argv_hits": argv, "screen_hits": screen });
    if clean { Ok(doc) } else { Err(doc.to_string()) }
}

/// A different fake value on every run, never printed.
fn fake_value() -> String {
    format!("dccfake-{:x}-{:x}-Zq9", now_ms(), std::process::id())
}

fn dcc_bin() -> PathBuf {
    root().join("target").join("debug").join("dcc")
}

fn secret_leak() -> R<Value> {
    need_isolation()?;
    let name = "xtask-leak-test";
    let value = fake_value();
    let bin = dcc_bin();
    if !bin.exists() {
        return Err(format!(
            "{} does not exist, run cargo build first",
            bin.display()
        ));
    }
    // 1. store it through the hidden prompt. `script` gives dcc a pty, the value only travels on its stdin.
    let mut child = Command::new("script")
        .args(["-q", "/dev/null"])
        .arg(&bin)
        .args(["secret", "set", name])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("cannot run script: {e}"))?;
    {
        let stdin = child.stdin.as_mut().ok_or("no stdin")?;
        std::thread::sleep(Duration::from_millis(800));
        stdin
            .write_all(format!("{value}\n").as_bytes())
            .map_err(|e| e.to_string())?;
    }
    let out = child.wait_with_output().map_err(|e| e.to_string())?;
    let said = String::from_utf8_lossy(&out.stdout).to_string();
    let echoed = said.contains(&value);
    let stored = said.contains("\"stored\":true");
    let exists = |expect: bool| -> R<bool> {
        let o = Command::new(&bin)
            .args(["secret", "exists", name])
            .output()
            .map_err(|e| e.to_string())?;
        let v: Value = serde_json::from_slice(&o.stdout).map_err(|e| e.to_string())?;
        Ok(v["exists"].as_bool() == Some(expect))
    };
    let exists_after_set = exists(true)?;

    // 2. scan the repo (and its .dev folder), the processes and the screens
    let mut hits = Hits::default();
    scan_dir(&root(), value.as_bytes(), &mut hits);
    let argv = argv_hits(value.as_bytes());
    let screens = screen_hits(value.as_bytes())?;

    // 3. positive control, the same scan must find a file that does hold the value
    let control = dev_dir().join("leak-control.txt");
    fs::write(&control, format!("control {value}\n")).map_err(|e| e.to_string())?;
    let mut control_hits = Hits::default();
    scan_dir(&dev_dir(), value.as_bytes(), &mut control_hits);
    fs::write(&control, "").map_err(|e| e.to_string())?; // emptied, not removed
    let control_found = control_hits
        .plain
        .iter()
        .any(|p| p.ends_with("leak-control.txt"));

    // 4. delete the secret again
    let del = Command::new(&bin)
        .args(["secret", "delete", name])
        .output()
        .map_err(|e| e.to_string())?;
    let gone = exists(false)?;

    let clean =
        !echoed && hits.plain.is_empty() && hits.base64.is_empty() && argv == 0 && screens == 0;
    let doc = json!({
        "clean": clean, "stored_through_prompt": stored, "exists_after_set": exists_after_set, "deleted": del.status.success() && gone,
        "prompt_output_had_the_value": echoed, "files_scanned": hits.scanned, "plain_hit_files": hits.plain, "base64_hit_files": hits.base64,
        "argv_hits": argv, "screen_hits": screens, "positive_control_found": control_found,
    });
    if clean && stored && exists_after_set && control_found && gone {
        Ok(doc)
    } else {
        Err(doc.to_string())
    }
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let result = match args.first().map(String::as_str) {
        Some("env") => Ok(json!({ "lines": env_lines() })).map(|v| {
            println!("{}", v["lines"].as_str().unwrap_or_default());
            Value::Null
        }),
        Some("dev-up") => dev_up(),
        Some("dev-down") => dev_down(),
        Some("leak-scan") => match args.get(1) {
            Some(dir) => need_isolation()
                .and_then(|_| leak_scan(Path::new(dir), args.iter().any(|a| a == "--screens"))),
            None => Err("usage: leak-scan DIR [--screens], the value comes on stdin".into()),
        },
        Some("secret-leak") => secret_leak(),
        _ => Err("tasks are env, dev-up, dev-down, leak-scan, secret-leak".into()),
    };
    let _ = process_alive;
    match result {
        Ok(Value::Null) => ExitCode::SUCCESS,
        Ok(v) => {
            println!("{v}");
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("{e}");
            ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base64_matches_known_vectors() {
        assert_eq!(base64(b""), b"");
        assert_eq!(base64(b"f"), b"Zg==");
        assert_eq!(base64(b"fo"), b"Zm8=");
        assert_eq!(base64(b"foo"), b"Zm9v");
        assert_eq!(base64(b"foobar"), b"Zm9vYmFy");
    }

    #[test]
    fn scan_finds_plain_and_encoded_values_at_every_offset() {
        let value = b"a-secret-value-12345";
        for prefix in ["", "k", "kk"] {
            let mut text = prefix.as_bytes().to_vec();
            text.extend(base64(&[prefix.as_bytes(), value].concat()));
            let (_, forms) = patterns(value);
            assert!(
                forms.iter().any(|f| contains(&text, f)),
                "offset {}",
                prefix.len()
            );
        }
        let (plain, forms) = patterns(value);
        assert!(contains(b"x a-secret-value-12345 y", &plain));
        assert!(!contains(b"nothing here", &plain));
        assert!(!forms.iter().any(|f| contains(b"nothing here", f)));
    }

    #[test]
    fn fake_values_differ_and_are_long_enough() {
        let a = fake_value();
        assert!(a.len() >= 16);
    }
}
