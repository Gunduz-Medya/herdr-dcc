//! `dcc config` through the real binary, with a scratch home. The real home is never used.
//! The Skillshare parts run only when Skillshare is installed.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use dcc::util::scratch_dir;
use serde_json::Value;

fn have_skillshare() -> bool {
    Command::new("skillshare").arg("version").output().is_ok()
}

fn dcc(data: &Path, home: &Path) -> Command {
    let mut c = Command::new(env!("CARGO_BIN_EXE_dcc"));
    c.arg("--data-dir")
        .arg(data)
        .env("DCC_AI_HOME", home)
        .env("HOME", home)
        .env_remove("DCC_REQUIRE_ISOLATION");
    c
}

fn ok(o: &Output) -> Value {
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    serde_json::from_slice(&o.stdout).unwrap()
}

fn err_kind(o: &Output) -> String {
    assert!(!o.status.success());
    let v: Value = serde_json::from_slice(&o.stderr).unwrap();
    v["error"]["kind"].as_str().unwrap().to_string()
}

fn put(home: &Path, rel: &str, text: &str) {
    let p = home.join(rel);
    fs::create_dir_all(p.parent().unwrap()).unwrap();
    fs::write(p, text).unwrap();
}

/// Path to content for every file below a folder, to compare before and after.
fn tree(dir: &Path) -> BTreeMap<PathBuf, Vec<u8>> {
    fn walk(d: &Path, out: &mut BTreeMap<PathBuf, Vec<u8>>) {
        for e in fs::read_dir(d).into_iter().flatten().flatten() {
            let p = e.path();
            let m = p.symlink_metadata().unwrap();
            if m.is_dir() {
                walk(&p, out);
            } else {
                out.insert(p.clone(), fs::read(&p).unwrap_or_default());
            }
        }
    }
    let mut out = BTreeMap::new();
    walk(dir, &mut out);
    out
}

fn skillshare(home: &Path, args: &[&str]) -> Output {
    Command::new("skillshare")
        .args(args)
        .env("HOME", home)
        .env("XDG_CONFIG_HOME", home.join(".config"))
        .output()
        .unwrap()
}

fn setup_home(name: &str) -> PathBuf {
    let home = scratch_dir(name);
    put(&home, ".claude/settings.json", "{\"marker\":\"keep-me\"}");
    put(&home, ".claude/CLAUDE.md", "memory");
    put(&home, ".claude/skills/old-skill/SKILL.md", "# old");
    put(&home, ".codex/config.toml", "x");
    let o = skillshare(
        &home,
        &[
            "init",
            "--no-copy",
            "--no-git",
            "--no-skill",
            "--targets",
            "claude",
        ],
    );
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    home
}

fn add_source_skill(home: &Path, name: &str) {
    put(
        home,
        &format!(".config/skillshare/skills/{name}/SKILL.md"),
        &format!("---\nname: {name}\ndescription: test\n---\nbody\n"),
    );
}

#[test]
fn the_inventory_lists_files_and_shows_no_content() {
    let data = scratch_dir("config-inv-data");
    let home = scratch_dir("config-inv-home");
    put(&home, ".claude/settings.json", "SECRET-CONTENT-MARKER");
    put(
        &home,
        ".claude/skills/alpha/SKILL.md",
        "SECRET-CONTENT-MARKER",
    );
    put(&home, ".claude/skills/.credentials.json", "x");
    let before = tree(&home);
    let o = dcc(&data, &home)
        .args(["config", "inventory"])
        .output()
        .unwrap();
    let text = String::from_utf8_lossy(&o.stdout).to_string();
    assert!(!text.contains("SECRET-CONTENT-MARKER") && !text.contains("credentials"));
    let v = ok(&o);
    assert!(
        v["items"]
            .as_array()
            .unwrap()
            .iter()
            .any(|i| i["path"] == "~/.claude/skills/alpha" && i["kind"] == "skill")
    );
    assert_eq!(v["withheld_credential_like"], 1);
    assert_eq!(
        before,
        tree(&home),
        "the inventory must not change anything"
    );
}

#[test]
fn isolation_requires_an_explicit_ai_home() {
    let data = scratch_dir("config-iso-data");
    let o = Command::new(env!("CARGO_BIN_EXE_dcc"))
        .arg("--data-dir")
        .arg(&data)
        .args(["config", "inventory"])
        .env("DCC_REQUIRE_ISOLATION", "1")
        .env_remove("DCC_AI_HOME")
        .output()
        .unwrap();
    assert_eq!(err_kind(&o), "isolation");
}

#[test]
fn a_preview_writes_nothing_and_an_apply_needs_that_preview() {
    if !have_skillshare() {
        eprintln!("skillshare is not installed, skipping");
        return;
    }
    let data = scratch_dir("config-sync-data");
    let home = setup_home("config-sync-home");
    add_source_skill(&home, "new-skill");
    let claude_before = tree(&home.join(".claude"));

    let p = ok(&dcc(&data, &home)
        .args(["config", "sync", "preview"])
        .output()
        .unwrap());
    assert_eq!(p["writes"], false);
    assert_eq!(p["summary"]["add"], 1);
    assert!(
        p["diff"]["targets"][0]["items"]
            .to_string()
            .contains("new-skill")
    );
    assert_eq!(
        claude_before,
        tree(&home.join(".claude")),
        "a preview must leave ~/.claude alone"
    );
    assert!(!home.join(".claude/skills/new-skill").exists());
    let hash = p["hash"].as_str().unwrap().to_string();

    // a wrong hash is refused and still nothing is written
    let bad = dcc(&data, &home)
        .args(["config", "sync", "apply", "--preview-hash", "deadbeef"])
        .output()
        .unwrap();
    assert_eq!(err_kind(&bad), "conflict");
    assert_eq!(claude_before, tree(&home.join(".claude")));

    // the right one backs up, then syncs
    let a = ok(&dcc(&data, &home)
        .args(["config", "sync", "apply", "--preview-hash", &hash])
        .output()
        .unwrap());
    assert_eq!(a["applied"], true);
    assert!(home.join(".claude/skills/new-skill").exists());
    let list = skillshare(&home, &["backup", "--list"]);
    assert!(
        String::from_utf8_lossy(&list.stdout).contains("20"),
        "a backup must exist"
    );
    // the settings file DCC never touches
    assert_eq!(
        fs::read_to_string(home.join(".claude/settings.json")).unwrap(),
        "{\"marker\":\"keep-me\"}"
    );
}

#[test]
fn a_removal_is_refused_without_its_own_approval() {
    if !have_skillshare() {
        eprintln!("skillshare is not installed, skipping");
        return;
    }
    let data = scratch_dir("config-del-data");
    let home = setup_home("config-del-home");
    add_source_skill(&home, "doomed");
    let hash = |d: &Path, h: &Path| {
        ok(&dcc(d, h)
            .args(["config", "sync", "preview"])
            .output()
            .unwrap())
    };
    let p = hash(&data, &home);
    ok(&dcc(&data, &home)
        .args([
            "config",
            "sync",
            "apply",
            "--preview-hash",
            p["hash"].as_str().unwrap(),
        ])
        .output()
        .unwrap());
    assert!(
        home.join(".claude/skills/doomed")
            .symlink_metadata()
            .is_ok()
    );
    // the source loses the skill, a sync would now remove it from the target
    fs::remove_dir_all(home.join(".config/skillshare/skills/doomed")).unwrap();
    let p = hash(&data, &home);
    assert!(p["summary"]["remove"].as_u64().unwrap() >= 1, "{p}");
    let h = p["hash"].as_str().unwrap();
    let refused = dcc(&data, &home)
        .args(["config", "sync", "apply", "--preview-hash", h])
        .output()
        .unwrap();
    assert_eq!(err_kind(&refused), "conflict");
    // the target entry is a link to the source, so look at the link itself
    let present = |h: &Path| h.join(".claude/skills/doomed").symlink_metadata().is_ok();
    assert!(present(&home), "nothing may be removed without approval");
    ok(&dcc(&data, &home)
        .args([
            "config",
            "sync",
            "apply",
            "--preview-hash",
            h,
            "--allow-delete",
        ])
        .output()
        .unwrap());
    assert!(!present(&home));
}
