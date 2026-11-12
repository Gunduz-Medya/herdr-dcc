//! Tests that run the real `dcc` binary. They never talk to Herdr's window or core: the commands
//! used here only touch the data folder, the keychain-free parts and the files of a scratch app home.

use std::fs;
use std::path::Path;
use std::process::{Command, Output};
use std::thread::sleep;
use std::time::{Duration, Instant};

use dcc::util::scratch_dir;
use serde_json::{Value, json};

fn dcc(data: &Path) -> Command {
    let mut c = Command::new(env!("CARGO_BIN_EXE_dcc"));
    c.arg("--data-dir")
        .arg(data)
        .env_remove("DCC_REQUIRE_ISOLATION")
        .env_remove("HC_SCOPE");
    c
}

fn json_of(o: &Output) -> Value {
    serde_json::from_slice(&o.stdout)
        .unwrap_or_else(|_| panic!("not JSON: {}", String::from_utf8_lossy(&o.stdout)))
}

fn err_of(o: &Output) -> Value {
    serde_json::from_slice(&o.stderr)
        .unwrap_or_else(|_| panic!("not JSON: {}", String::from_utf8_lossy(&o.stderr)))
}

fn status(data: &Path, id: &str) -> Value {
    json_of(&dcc(data).args(["job", "status", id]).output().unwrap())
}

fn wait_for(data: &Path, id: &str, state: &str, secs: u64) -> Value {
    let start = Instant::now();
    loop {
        let s = status(data, id);
        if s["state"] == state || start.elapsed() > Duration::from_secs(secs) {
            return s;
        }
        sleep(Duration::from_millis(200));
    }
}

#[test]
fn a_job_runs_detached_and_reports_its_result() {
    let data = scratch_dir("cli-job");
    let start = Instant::now();
    let out = dcc(&data)
        .args(["job", "start", "--", "debug", "sleep", "--seconds", "3"])
        .output()
        .unwrap();
    assert!(out.status.success());
    assert!(
        start.elapsed() < Duration::from_secs(2),
        "start must return at once, it took {:?}",
        start.elapsed()
    );
    let id = json_of(&out)["id"].as_str().unwrap().to_string();
    assert_eq!(status(&data, &id)["state"], "running");
    let done = wait_for(&data, &id, "done", 15);
    assert_eq!(done["state"], "done", "{done}");
    assert_eq!(done["code"], 0);
    assert!(done["stdout"].as_str().unwrap().contains("slept"));
    assert_eq!(
        json_of(&dcc(&data).args(["job", "clean", &id]).output().unwrap())["cleaned"],
        true
    );
    assert_eq!(status(&data, &id)["state"], "unknown");
}

#[test]
fn sweep_leaves_a_running_job_alone() {
    let data = scratch_dir("cli-sweep");
    let id = json_of(
        &dcc(&data)
            .args(["job", "start", "--", "debug", "sleep", "--seconds", "3"])
            .output()
            .unwrap(),
    )["id"]
        .as_str()
        .unwrap()
        .to_string();
    sleep(Duration::from_millis(1500)); // let a heartbeat land
    let r = json_of(
        &dcc(&data)
            .args(["job", "sweep", "--older-than-secs", "0"])
            .output()
            .unwrap(),
    );
    assert_eq!(r["removed"], json!([]));
    assert_eq!(r["kept"], 1);
    assert_eq!(wait_for(&data, &id, "done", 15)["state"], "done");
    let r = json_of(
        &dcc(&data)
            .args(["job", "sweep", "--older-than-secs", "0"])
            .output()
            .unwrap(),
    );
    assert_eq!(r["removed"], json!([id]));
}

#[test]
fn there_is_no_secret_get() {
    let data = scratch_dir("cli-noget");
    let o = dcc(&data)
        .args(["secret", "get", "anything"])
        .output()
        .unwrap();
    assert_eq!(o.status.code(), Some(2));
    let help = dcc(&data).args(["secret", "--help"]).output().unwrap();
    let text = String::from_utf8_lossy(&help.stdout).to_string();
    for word in ["set", "exists", "delete"] {
        assert!(text.contains(word), "{word} missing from the secret help");
    }
    assert!(
        !text.lines().any(|l| l.trim_start().starts_with("get")),
        "a get command is listed:\n{text}"
    );
}

#[test]
fn secret_set_refuses_a_pipe_and_names_are_checked() {
    let data = scratch_dir("cli-set");
    let mut c = dcc(&data);
    let o = c
        .args(["secret", "set", "fine-name"])
        .stdin(std::process::Stdio::piped())
        .output()
        .unwrap();
    assert_eq!(err_of(&o)["error"]["kind"], "usage");
    let o = dcc(&data)
        .args(["secret", "exists", "Bad Name"])
        .output()
        .unwrap();
    assert_eq!(err_of(&o)["error"]["kind"], "usage");
}

#[test]
fn restoring_an_unknown_workspace_is_a_clear_not_found() {
    let data = scratch_dir("cli-restore");
    let o = dcc(&data)
        .args(["workspace", "restore", "ghost"])
        .output()
        .unwrap();
    assert_eq!(o.status.code(), Some(3));
    assert_eq!(err_of(&o)["error"]["kind"], "not_found");
    let list = json_of(&dcc(&data).args(["workspace", "list"]).output().unwrap());
    assert_eq!(list["workspaces"], json!([]));
    let t = json_of(
        &dcc(&data)
            .args(["workspace", "template", "list"])
            .output()
            .unwrap(),
    );
    assert_eq!(t["templates"], json!(["code"]));
}

#[test]
fn isolation_is_enforced_when_required() {
    let data = scratch_dir("cli-iso");
    let o = dcc(&data)
        .args(["herdr", "check-env"])
        .env("DCC_REQUIRE_ISOLATION", "1")
        .env_remove("HERDR_APP_HOME")
        .env_remove("HC_SESSION")
        .output()
        .unwrap();
    assert_eq!(err_of(&o)["error"]["kind"], "isolation");
    let ok = dcc(&data)
        .args(["herdr", "check-env"])
        .env("DCC_REQUIRE_ISOLATION", "1")
        .env("HERDR_APP_HOME", &data)
        .env("HC_SESSION", "t")
        .env("XDG_STATE_HOME", "/s")
        .output()
        .unwrap();
    assert!(
        ok.status.success(),
        "{}",
        String::from_utf8_lossy(&ok.stderr)
    );
    // a session name that makes the core socket path too long is refused up front
    let long = dcc(&data)
        .args(["herdr", "check-env"])
        .env("DCC_REQUIRE_ISOLATION", "1")
        .env("HERDR_APP_HOME", &data)
        .env(
            "HC_SESSION",
            "a-very-long-session-name-for-a-socket-path-test",
        )
        .env("XDG_STATE_HOME", "/s")
        .output()
        .unwrap();
    assert_eq!(err_of(&long)["error"]["kind"], "isolation");
}

#[test]
fn ensure_mods_copies_and_writes_the_config() {
    let data = scratch_dir("cli-mods");
    let home = data.join("home");
    let src = data.join("src/demo");
    fs::create_dir_all(&src).unwrap();
    fs::write(src.join("mod.json"), "{}").unwrap();
    fs::write(src.join("main.js"), "").unwrap();
    let o = dcc(&data)
        .args(["herdr", "ensure-mods", "--no-check", "--src"])
        .arg(data.join("src"))
        .env("HERDR_APP_HOME", &home)
        .output()
        .unwrap();
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    let cfg: Value =
        serde_json::from_str(&fs::read_to_string(home.join("mods/demo/config.json")).unwrap())
            .unwrap();
    assert!(cfg["dcc"].as_str().unwrap().ends_with("dcc"));
    assert!(cfg["data_dir"].as_str().unwrap().contains("cli-mods"));
}

#[test]
fn ensure_layout_builds_then_skips_then_rebuilds() {
    if Command::new("herdr-app").arg("--version").output().is_err() {
        eprintln!("herdr-app is not installed, skipping");
        return;
    }
    let data = scratch_dir("cli-layout");
    let home = data.join("home");
    fs::create_dir_all(home.join("layouts")).unwrap();
    fs::write(home.join("layouts/main.json"), json!({"id":"main","type":"column","children":[{"id":"tabs","type":"block","content":{"mod":"tabs"}},{"id":"panel","type":"column","children":[{"id":"panel-body","type":"stack","children":[{"id":"files","type":"block"}]}]}]}).to_string()).unwrap();
    let recipe = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../layouts/dcc.recipe.json");
    let run = |force: bool| {
        let mut c = dcc(&data);
        c.args(["herdr", "ensure-layout", "--recipe"])
            .arg(&recipe)
            .env("HERDR_APP_HOME", &home);
        if force {
            c.arg("--force");
        }
        json_of(&c.output().unwrap())
    };
    assert_eq!(run(false)["regenerated"], true);
    assert_eq!(run(false)["regenerated"], false);
    let built: Value =
        serde_json::from_str(&fs::read_to_string(home.join("layouts/dcc.json")).unwrap()).unwrap();
    // the DCC panels sit in the side panel stack, after Herdr's own, and the tab bar is DCC's
    let stack = &built["children"][1]["children"][0]["children"];
    assert_eq!(stack[0]["id"], "files");
    assert_eq!(stack[1]["content"]["mod"], "dcc-workspace");
    assert_eq!(stack.as_array().unwrap().len(), 6);
    assert_eq!(built["children"][0]["content"]["mod"], "dcc-tabs");
    assert_eq!(run(true)["reason"], "forced");
}
