//! DCC mods are folders copied into `<app home>/mods/<name>/` (the desktop app has no plugin
//! install). `herdr-app check <folder>` loads a mod once and reports what is wrong with it.
//! Files are overwritten, never removed.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use serde_json::{Value, json};

use crate::error::{Error, Result};

fn copy_dir(src: &Path, dst: &Path) -> Result<usize> {
    fs::create_dir_all(dst)?;
    let mut count = 0;
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let to = dst.join(entry.file_name());
        let kind = entry.file_type()?;
        if kind.is_dir() {
            count += copy_dir(&entry.path(), &to)?;
        } else if kind.is_file() {
            fs::copy(entry.path(), &to)?;
            count += 1;
        }
    }
    Ok(count)
}

/// Names of the mod folders under `src` (folders that hold a `mod.json`).
pub fn mod_names(src: &Path) -> Result<Vec<String>> {
    let mut names = Vec::new();
    for entry in fs::read_dir(src)
        .map_err(|e| Error::not_found(format!("cannot read {}: {e}", src.display())))?
    {
        let entry = entry?;
        if entry.file_type()?.is_dir() && entry.path().join("mod.json").exists() {
            names.push(entry.file_name().to_string_lossy().to_string());
        }
    }
    names.sort();
    Ok(names)
}

pub fn mod_path(app_home: &Path, name: &str) -> PathBuf {
    app_home.join("mods").join(name)
}

/// Copies every mod of `src` into the app home. With `config`, also writes it as `config.json` in
/// each copy (the mods find the dcc binary and the data folder there, since `exec` has no
/// environment). With `check`, runs `herdr-app check` on each copy.
pub fn ensure(src: &Path, app_home: &Path, check: bool, config: Option<&Value>) -> Result<Value> {
    let mut report = Vec::new();
    for name in mod_names(src)? {
        let dst = mod_path(app_home, &name);
        let files = copy_dir(&src.join(&name), &dst)?;
        if let Some(cfg) = config {
            fs::write(dst.join("config.json"), serde_json::to_string_pretty(cfg)?)?;
        }
        let mut entry = json!({ "mod": name, "files": files });
        if check {
            let out = Command::new("herdr-app")
                .arg("check")
                .arg(&dst)
                .output()
                .map_err(|e| Error::external(format!("cannot run herdr-app: {e}")))?;
            let text = format!(
                "{}{}",
                String::from_utf8_lossy(&out.stdout),
                String::from_utf8_lossy(&out.stderr)
            );
            entry["check_passed"] = json!(out.status.success() && text.contains("check passed"));
            entry["check"] = json!(text.trim().lines().last().unwrap_or_default());
        }
        report.push(entry);
    }
    Ok(json!({ "mods": report }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::util::scratch_dir;

    #[test]
    fn copies_only_folders_with_a_mod_json() {
        let d = scratch_dir("mods");
        let src = d.join("src");
        fs::create_dir_all(src.join("a/sub")).unwrap();
        fs::write(src.join("a/mod.json"), "{}").unwrap();
        fs::write(src.join("a/main.js"), "x").unwrap();
        fs::write(src.join("a/sub/b.txt"), "y").unwrap();
        fs::create_dir_all(src.join("notamod")).unwrap();
        fs::write(src.join("notamod/readme.md"), "z").unwrap();
        let home = d.join("home");
        let r = ensure(&src, &home, false, Some(&json!({"dcc": "/x/dcc"}))).unwrap();
        assert_eq!(r["mods"].as_array().unwrap().len(), 1);
        assert_eq!(r["mods"][0]["files"], 3);
        assert!(mod_path(&home, "a").join("config.json").exists());
        assert!(mod_path(&home, "a").join("sub/b.txt").exists());
        assert!(!mod_path(&home, "notamod").exists());
    }
}
