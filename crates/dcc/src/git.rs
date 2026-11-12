//! Read only git facts and worktree handling. Every call runs `git --no-optional-locks -C <dir>`
//! with a short timeout, so it never takes the index lock away from an agent and never hangs.
//! Worktree removal is its own function and the caller must have asked for it explicitly.

use std::fs;
use std::io::Read;
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use serde_json::{Value, json};

use crate::error::{Error, Result};

const GIT_TIMEOUT_MS: u64 = 1500;

/// Runs one git command. None on failure or timeout.
pub fn git(cwd: &str, args: &[&str]) -> Option<String> {
    git_with_timeout(cwd, args, Duration::from_millis(GIT_TIMEOUT_MS))
}

pub fn git_with_timeout(cwd: &str, args: &[&str], timeout: Duration) -> Option<String> {
    // `core.fsmonitor` in a repository's own config is a command that `git status` runs. DCC reads
    // folders of agents and terminals, some of them cloned from places nobody vetted, so that hook
    // is switched off for every read only call, and the filter drivers are emptied for `status`
    // (see `filter_overrides`). `--no-optional-locks` keeps the index untouched.
    let mut child = Command::new("git")
        .args(["-c", "core.fsmonitor=false"])
        .arg("--no-optional-locks")
        .env("GIT_TERMINAL_PROMPT", "0")
        .arg("-C")
        .arg(cwd)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    let mut out = child.stdout.take()?;
    let reader = std::thread::spawn(move || {
        let mut s = String::new();
        let _ = out.read_to_string(&mut s);
        s
    });
    let start = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                let text = reader.join().ok()?;
                return if status.success() {
                    Some(text.trim_end().to_string())
                } else {
                    None
                };
            }
            Ok(None) if start.elapsed() > timeout => {
                let _ = child.kill();
                let _ = child.wait();
                return None;
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(4)),
            Err(_) => return None,
        }
    }
}

/// Names of the filter drivers (`filter.<name>.clean` and friends) defined in the configuration
/// that applies to this folder. Reading configuration runs nothing.
fn filter_driver_names(cwd: &str) -> Vec<String> {
    let text = git(cwd, &["config", "--get-regexp", r"^filter\."]).unwrap_or_default();
    let mut names: Vec<String> = text
        .lines()
        .filter_map(|line| line.split_whitespace().next())
        .filter_map(|key| {
            key.strip_prefix("filter.")?
                .rsplit_once('.')
                .map(|(name, _)| name.to_string())
        })
        .collect();
    names.sort();
    names.dedup();
    names
}

/// `-c` options that empty every filter driver, so `git status` cannot run a command that a
/// repository (through `.gitattributes` and its config) asks for while it compares file contents.
fn filter_overrides(cwd: &str) -> Vec<String> {
    let mut out = Vec::new();
    for name in filter_driver_names(cwd) {
        for key in ["clean", "smudge", "process"] {
            out.push("-c".to_string());
            out.push(format!("filter.{name}.{key}="));
        }
        out.push("-c".to_string());
        out.push(format!("filter.{name}.required=false"));
    }
    out
}

fn same_path(a: &str, b: &str) -> bool {
    let norm = |p: &str| {
        fs::canonicalize(p)
            .map(|c| c.to_string_lossy().to_string())
            .unwrap_or_else(|_| p.trim_end_matches(['/', '\\']).to_string())
    };
    norm(a) == norm(b)
}

/// Counts from `git status --porcelain` text: (changed lines, staged, unstaged, untracked).
pub fn count_porcelain(text: &str) -> (u64, u64, u64, u64) {
    let (mut changed, mut staged, mut unstaged, mut untracked) = (0, 0, 0, 0);
    for line in text.lines() {
        let b = line.as_bytes();
        if b.len() < 3 {
            continue;
        }
        changed += 1;
        if line.starts_with("??") {
            untracked += 1;
            continue;
        }
        if !matches!(b[0], b' ' | b'?' | b'!') {
            staged += 1;
        }
        if !matches!(b[1], b' ' | b'?' | b'!') {
            unstaged += 1;
        }
    }
    (changed, staged, unstaged, untracked)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitInfo {
    pub repo_root: String,
    /// The branch, or the short commit when HEAD is detached.
    pub branch: Option<String>,
    pub detached: bool,
    pub linked_worktree: bool,
    pub changed: Option<u64>,
    pub staged: Option<u64>,
    pub unstaged: Option<u64>,
    pub untracked: Option<u64>,
}

impl GitInfo {
    pub fn worktree_kind(&self) -> &'static str {
        if self.linked_worktree {
            "linked"
        } else {
            "main"
        }
    }

    pub fn to_json(&self) -> Value {
        json!({
            "ok": true, "repo_root": self.repo_root, "branch": self.branch, "detached": self.detached,
            "worktree_kind": self.worktree_kind(), "changed_files": self.changed, "staged": self.staged,
            "unstaged": self.unstaged, "untracked": self.untracked,
        })
    }
}

/// None when the folder is gone or not inside a git repository (or git timed out).
pub fn info(cwd: &str) -> Option<GitInfo> {
    if !Path::new(cwd).is_dir() {
        return None;
    }
    let top = git(
        cwd,
        &[
            "rev-parse",
            "--show-toplevel",
            "--path-format=absolute",
            "--git-dir",
            "--git-common-dir",
        ],
    )?;
    let mut it = top.lines();
    let (root, gitdir, common) = (
        it.next()?.to_string(),
        it.next().unwrap_or(""),
        it.next().unwrap_or(""),
    );
    let linked = !gitdir.is_empty() && !common.is_empty() && !same_path(gitdir, common);
    let (branch, detached) = match git(cwd, &["symbolic-ref", "--short", "-q", "HEAD"]) {
        Some(b) if !b.is_empty() => (Some(b), false),
        _ => (
            git(cwd, &["rev-parse", "--short", "HEAD"]).filter(|s| !s.is_empty()),
            true,
        ),
    };
    // status compares file contents, which can run the clean filters of the repository
    let overrides = filter_overrides(cwd);
    let mut status_args: Vec<&str> = overrides.iter().map(String::as_str).collect();
    status_args.extend(["status", "--porcelain"]);
    let counts = git(cwd, &status_args).map(|t| count_porcelain(&t));
    Some(GitInfo {
        repo_root: root,
        branch,
        detached,
        linked_worktree: linked,
        changed: counts.map(|c| c.0),
        staged: counts.map(|c| c.1),
        unstaged: counts.map(|c| c.2),
        untracked: counts.map(|c| c.3),
    })
}

// worktrees

fn run_git(cwd: &str, args: &[&str]) -> Result<String> {
    let out = Command::new("git")
        .arg("-C")
        .arg(cwd)
        .args(args)
        .stdin(Stdio::null())
        .output()
        .map_err(|e| Error::external(format!("cannot run git: {e}")))?;
    if !out.status.success() {
        return Err(Error::external(format!(
            "git {}: {}",
            args.first().unwrap_or(&""),
            String::from_utf8_lossy(&out.stderr).trim()
        )));
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim_end().to_string())
}

/// A branch name made of safe characters only.
pub fn valid_branch(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 100
        && !name.starts_with(['-', '/', '.'])
        && !name.ends_with(['/', '.'])
        && !name.contains("..")
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '/' | '.'))
}

/// `git worktree add -b <branch> <path>` from the repository at `repo`. Refuses an existing path.
pub fn worktree_add(repo: &str, branch: &str, path: &Path) -> Result<()> {
    if !valid_branch(branch) {
        return Err(Error::usage(format!("invalid branch name {branch:?}")));
    }
    if path.exists() {
        return Err(Error::conflict(format!(
            "{} already exists",
            path.display()
        )));
    }
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    run_git(
        repo,
        &["worktree", "add", "-b", branch, &path.to_string_lossy()],
    )
    .map(|_| ())
}

/// Worktrees of the repository as (path, branch or None when detached), main worktree first.
pub fn worktree_list(repo: &str) -> Result<Vec<(String, Option<String>)>> {
    let text = run_git(repo, &["worktree", "list", "--porcelain"])?;
    let mut out: Vec<(String, Option<String>)> = Vec::new();
    for line in text.lines() {
        if let Some(p) = line.strip_prefix("worktree ") {
            out.push((p.to_string(), None));
        } else if let (Some(b), Some(last)) =
            (line.strip_prefix("branch refs/heads/"), out.last_mut())
        {
            last.1 = Some(b.to_string());
        }
    }
    Ok(out)
}

/// Removes a linked worktree. The caller must have been told to by a person (`confirmed`). The
/// main worktree is never removed, and git refuses a worktree with changes unless forced, which
/// this function never does.
pub fn worktree_remove(repo: &str, path: &str, confirmed: bool) -> Result<()> {
    if !confirmed {
        return Err(Error::usage(
            "removing a worktree needs an explicit confirmation",
        ));
    }
    let list = worktree_list(repo)?;
    let Some((main, _)) = list.first() else {
        return Err(Error::not_found("no worktrees"));
    };
    if same_path(main, path) {
        return Err(Error::usage("the main worktree is never removed"));
    }
    if !list.iter().any(|(p, _)| same_path(p, path)) {
        return Err(Error::not_found(format!(
            "{path} is not a worktree of this repository"
        )));
    }
    run_git(repo, &["worktree", "remove", path]).map(|_| ())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::util::{scratch_repo, sh};

    #[test]
    fn main_worktree_clean_and_dirty() {
        let d = scratch_repo("git-main");
        let i = info(d.to_str().unwrap()).unwrap();
        assert_eq!(
            (i.branch.as_deref(), i.worktree_kind(), i.changed),
            (Some("main"), "main", Some(0))
        );
        fs::write(d.join("README.md"), "changed\n").unwrap();
        fs::write(d.join("new.txt"), "n\n").unwrap();
        fs::write(d.join("staged.txt"), "s\n").unwrap();
        sh(&d, &["add", "staged.txt"]);
        let i = info(d.to_str().unwrap()).unwrap();
        assert_eq!(
            (i.changed, i.staged, i.unstaged, i.untracked),
            (Some(3), Some(1), Some(1), Some(1))
        );
    }

    #[test]
    fn linked_worktree_detached_head_and_removal() {
        let d = scratch_repo("git-wt");
        let wt = d
            .parent()
            .unwrap()
            .join(format!("{}-wt", d.file_name().unwrap().to_string_lossy()));
        worktree_add(d.to_str().unwrap(), "dcc/abc", &wt).unwrap();
        let i = info(wt.to_str().unwrap()).unwrap();
        assert_eq!(
            (i.branch.as_deref(), i.worktree_kind()),
            (Some("dcc/abc"), "linked")
        );
        assert_eq!(worktree_list(d.to_str().unwrap()).unwrap().len(), 2);
        assert_eq!(
            worktree_add(d.to_str().unwrap(), "x", &wt)
                .unwrap_err()
                .kind,
            "conflict"
        );
        assert_eq!(
            worktree_add(d.to_str().unwrap(), "-bad", &wt.join("n"))
                .unwrap_err()
                .kind,
            "usage"
        );
        // removal needs the explicit flag and never touches the main worktree
        assert_eq!(
            worktree_remove(d.to_str().unwrap(), wt.to_str().unwrap(), false)
                .unwrap_err()
                .kind,
            "usage"
        );
        assert_eq!(
            worktree_remove(d.to_str().unwrap(), d.to_str().unwrap(), true)
                .unwrap_err()
                .kind,
            "usage"
        );
        worktree_remove(d.to_str().unwrap(), wt.to_str().unwrap(), true).unwrap();
        assert_eq!(worktree_list(d.to_str().unwrap()).unwrap().len(), 1);
        sh(&d, &["checkout", "-q", "--detach"]);
        let i = info(d.to_str().unwrap()).unwrap();
        assert!(i.detached && i.branch.as_deref().is_some_and(|b| b.len() >= 4));
    }

    #[test]
    fn a_dirty_worktree_is_not_removed() {
        let d = scratch_repo("git-dirty");
        let wt = d
            .parent()
            .unwrap()
            .join(format!("{}-wt", d.file_name().unwrap().to_string_lossy()));
        worktree_add(d.to_str().unwrap(), "dcc/dirty", &wt).unwrap();
        fs::write(wt.join("tracked-change.txt"), "x").unwrap();
        sh(&wt, &["add", "tracked-change.txt"]);
        assert_eq!(
            worktree_remove(d.to_str().unwrap(), wt.to_str().unwrap(), true)
                .unwrap_err()
                .kind,
            "external"
        );
        assert!(wt.exists());
    }

    #[cfg(unix)]
    #[test]
    fn a_repository_cannot_run_its_own_fsmonitor_command_through_dcc() {
        use std::os::unix::fs::PermissionsExt;
        let d = scratch_repo("git-fsmonitor");
        let marker = d.join("..").join(format!(
            "{}-marker",
            d.file_name().unwrap().to_string_lossy()
        ));
        let script = d.join("..").join(format!(
            "{}-hook.sh",
            d.file_name().unwrap().to_string_lossy()
        ));
        fs::write(
            &script,
            format!(
                "#!/bin/sh\necho ran > '{}'\nprintf '\\0'\n",
                marker.display()
            ),
        )
        .unwrap();
        fs::set_permissions(&script, fs::Permissions::from_mode(0o755)).unwrap();
        sh(&d, &["config", "core.fsmonitor", script.to_str().unwrap()]);

        // positive control, plain git does run the configured command
        let plain = Command::new("git")
            .arg("-C")
            .arg(&d)
            .args(["status", "--porcelain"])
            .output()
            .unwrap();
        assert!(plain.status.success());
        assert!(
            marker.exists(),
            "the control must show that the config really runs a command"
        );
        fs::remove_file(&marker).unwrap();

        // through dcc it does not
        let i = info(d.to_str().unwrap()).unwrap();
        assert_eq!(i.changed, Some(0));
        assert!(
            !marker.exists(),
            "dcc ran the fsmonitor command of the repository"
        );
    }

    #[cfg(unix)]
    #[test]
    fn a_repository_cannot_run_its_own_clean_filter_through_dcc() {
        use std::os::unix::fs::PermissionsExt;
        let d = scratch_repo("git-filter");
        let name = d.file_name().unwrap().to_string_lossy().to_string();
        let marker = d.join("..").join(format!("{name}-filter-marker"));
        let script = d.join("..").join(format!("{name}-filter.sh"));
        fs::write(
            &script,
            format!("#!/bin/sh\necho ran >> '{}'\ncat\n", marker.display()),
        )
        .unwrap();
        fs::set_permissions(&script, fs::Permissions::from_mode(0o755)).unwrap();
        fs::write(d.join(".gitattributes"), "*.dat filter=evil\n").unwrap();
        fs::write(d.join("a.dat"), "one\n").unwrap();
        sh(&d, &["add", ".gitattributes", "a.dat"]);
        sh(&d, &["commit", "-q", "-m", "files"]);
        sh(
            &d,
            &["config", "filter.evil.clean", script.to_str().unwrap()],
        );
        // same size, different content and time, so git has to look at the file again
        std::thread::sleep(std::time::Duration::from_millis(1100));
        fs::write(d.join("a.dat"), "two\n").unwrap();

        // positive control, plain git runs the filter
        let plain = Command::new("git")
            .arg("-C")
            .arg(&d)
            .args(["status", "--porcelain"])
            .output()
            .unwrap();
        assert!(plain.status.success());
        assert!(
            marker.exists(),
            "the control must show that the config really runs a command"
        );
        fs::remove_file(&marker).unwrap();

        // through dcc it does not, and the change is still counted
        let i = info(d.to_str().unwrap()).unwrap();
        assert!(
            !marker.exists(),
            "dcc ran the clean filter of the repository"
        );
        assert_eq!(i.changed, Some(1));
    }

    #[test]
    fn filter_names_come_from_the_configuration() {
        let d = scratch_repo("git-filter-names");
        sh(&d, &["config", "filter.one.clean", "x"]);
        sh(&d, &["config", "filter.two.dotted.name.smudge", "y"]);
        let names = filter_driver_names(d.to_str().unwrap());
        // a global config (git lfs for example) may add more names
        assert!(names.contains(&"one".to_string()));
        assert!(names.contains(&"two.dotted.name".to_string()));
        let o = filter_overrides(d.to_str().unwrap());
        assert!(
            o.contains(&"filter.one.clean=".to_string())
                && o.contains(&"filter.two.dotted.name.process=".to_string())
        );
    }

    #[test]
    fn a_missing_folder_gives_none() {
        assert!(info("/definitely/not/here").is_none());
    }

    #[test]
    fn porcelain_counts() {
        assert_eq!(
            count_porcelain("M  a\n M b\nMM c\n?? d\nA  e\n"),
            (5, 3, 2, 1)
        );
        assert_eq!(count_porcelain(""), (0, 0, 0, 0));
    }

    #[test]
    fn branch_names() {
        for ok in ["dcc/abc", "feature/x-1", "a_b.c"] {
            assert!(valid_branch(ok), "{ok}");
        }
        for bad in ["", "-x", "/x", "a..b", "a b", "a;b", "x/"] {
            assert!(!valid_branch(bad), "{bad}");
        }
    }
}
