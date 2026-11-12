//! `dcc git` through the real binary, on scratch repositories.

use std::path::Path;
use std::process::{Command, Output};

use dcc::util::{scratch_dir, scratch_repo};
use serde_json::Value;

fn dcc() -> Command {
    let mut c = Command::new(env!("CARGO_BIN_EXE_dcc"));
    c.env_remove("DCC_REQUIRE_ISOLATION");
    c
}

fn ok(o: &Output) -> Value {
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    serde_json::from_slice(&o.stdout).unwrap()
}

#[test]
fn worktrees_are_added_listed_and_removed_only_with_confirmation() {
    let repo = scratch_repo("git-cli-wt");
    let wt = scratch_dir("git-cli-wt-dir").join("feature");
    let r = repo.to_str().unwrap();
    let add = dcc()
        .args(["git", "worktree", "add", r, "--branch", "feature/one"])
        .arg(&wt)
        .output()
        .unwrap();
    assert_eq!(ok(&add)["branch"], "feature/one");

    let list = ok(&dcc().args(["git", "worktree", "list", r]).output().unwrap());
    let paths: Vec<String> = list["worktrees"]
        .as_array()
        .unwrap()
        .iter()
        .map(|w| w["path"].as_str().unwrap().to_string())
        .collect();
    assert_eq!(paths.len(), 2);

    // the linked worktree shows the repository's branch facts
    let info = ok(&dcc().args(["git", "info"]).arg(&wt).output().unwrap());
    assert_eq!(
        (info["branch"].as_str(), info["worktree_kind"].as_str()),
        (Some("feature/one"), Some("linked"))
    );

    // removal needs --confirm, without it nothing happens
    let no = dcc()
        .args(["git", "worktree", "remove", r])
        .arg(&wt)
        .output()
        .unwrap();
    assert!(!no.status.success());
    assert!(Path::new(&wt).exists());
    let yes = dcc()
        .args(["git", "worktree", "remove", r, "--confirm"])
        .arg(&wt)
        .output()
        .unwrap();
    ok(&yes);
    assert!(!wt.exists());

    // a bad branch name is refused before git runs
    let bad = dcc()
        .args(["git", "worktree", "add", r, "--branch", "-bad"])
        .arg(wt.with_file_name("x"))
        .output()
        .unwrap();
    assert!(!bad.status.success());
}

#[test]
fn tools_report_paths_and_install_hints() {
    let t = ok(&dcc().args(["git", "tools"]).output().unwrap());
    let names: Vec<&str> = t["tools"]
        .as_array()
        .unwrap()
        .iter()
        .map(|x| x["name"].as_str().unwrap())
        .collect();
    assert_eq!(names, ["nvim", "lazygit"]);
    assert!(t["tools"][0]["install"].as_str().unwrap().len() > 5);
}
