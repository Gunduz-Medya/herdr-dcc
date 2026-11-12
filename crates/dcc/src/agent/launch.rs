//! `dcc agent launch`: starts a Claude Code agent the DCC way.
//!
//! 1. optionally a worktree of its own (`git worktree add -b dcc/<id> <path>`), so two agents
//!    never share a folder (risk 22)
//! 2. a run id, which is also the first session id (`claude --session-id <id>`)
//! 3. a settings file in the data folder that points the hooks and the status line at dcc
//!    (`claude --settings <file>`). Nothing is written to `~/.claude`.
//! 4. a tab, then `hc run` with the labels `herdr.space`, `herdr.tab` and `dcc.run`. The agent
//!    runs as `dcc agent exec`, which cleans the environment, adds `DCC_RUN_ID` and replaces itself
//!    with `claude`. Started this way the terminal keeps its exit code, so a finished run can be
//!    told apart as Completed, Stopped or Failed.
//! 5. the shell the new tab started with is closed, so the tab holds the agent only.

use std::path::{Path, PathBuf};

use serde_json::{Value, json};

use super::adapter::{AgentAdapter, ClaudeAdapter, LaunchContext, LaunchRequest, find_program};
use super::db::Run;
use super::state::DccState;
use crate::error::{Error, Result};
use crate::git;
use crate::herdr::api::Herdr;
use crate::herdr::tabs::{TabSpec, run_in_new_tab};
use crate::store::Store;
use crate::util::{create_private_dir, new_uuid};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorktreeMode {
    /// A new worktree and branch for this agent.
    Auto,
    /// The repository folder itself.
    Off,
}

#[derive(Debug, Clone)]
pub struct LaunchParams {
    pub repo: PathBuf,
    pub worktree: WorktreeMode,
    pub branch: Option<String>,
    pub model: Option<String>,
    pub permission_mode: Option<String>,
    pub extra_args: Vec<String>,
    /// Space to put the agent in, made when it does not exist.
    pub space: String,
    /// The agent program. Found on PATH when not given.
    pub program: Option<PathBuf>,
    pub dcc_exe: PathBuf,
    pub data_dir: PathBuf,
    pub herdr_session: Option<String>,
}

fn worktree_path(repo_root: &str, short: &str) -> PathBuf {
    let root = Path::new(repo_root);
    let name = root
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "repo".into());
    root.parent()
        .unwrap_or(root)
        .join(format!("{name}.worktrees"))
        .join(short)
}

pub fn launch(store: &Store, h: &dyn Herdr, p: &LaunchParams, now_ms: i64) -> Result<Value> {
    let repo = p.repo.to_string_lossy().to_string();
    // A folder that is not a git repository can host an agent in place (worktree off), the panel
    // then shows no branch. A separate worktree needs git.
    let info = git::info(&repo);
    if info.is_none() && p.worktree == WorktreeMode::Auto {
        return Err(Error::usage(format!(
            "{repo} is not a git repository, a separate worktree needs one (start the agent without it)"
        )));
    }
    let run_id = new_uuid();
    let short = run_id[..8].to_string();
    let program = match &p.program {
        Some(path) => path.clone(),
        None => find_program("claude")
            .ok_or_else(|| Error::not_found("claude was not found on PATH"))?,
    };

    let (cwd, worktree_path_used, branch) = match p.worktree {
        WorktreeMode::Off => (
            repo.clone(),
            None,
            info.as_ref().and_then(|i| i.branch.clone()),
        ),
        WorktreeMode::Auto => {
            let info = info
                .as_ref()
                .ok_or_else(|| Error::usage("not a git repository"))?;
            let branch = p.branch.clone().unwrap_or_else(|| format!("dcc/{short}"));
            let path = worktree_path(&info.repo_root, &short);
            git::worktree_add(&info.repo_root, &branch, &path)?;
            (
                path.to_string_lossy().to_string(),
                Some(path.to_string_lossy().to_string()),
                Some(branch),
            )
        }
    };

    let run_dir = p.data_dir.join("agents").join(&run_id);
    create_private_dir(&run_dir)?;
    let settings_path = run_dir.join("settings.json");
    let adapter = ClaudeAdapter;
    let ctx = LaunchContext {
        program_path: &program.to_string_lossy(),
        run_id: &run_id,
        settings_path: &settings_path,
        dcc_exe: &p.dcc_exe,
        data_dir: &p.data_dir,
    };
    let plan = adapter.launch_plan(
        &ctx,
        &LaunchRequest {
            model: p.model.clone(),
            permission_mode: p.permission_mode.clone(),
            extra_args: p.extra_args.clone(),
        },
    )?;
    if let Some(settings) = &plan.settings {
        std::fs::write(&settings_path, serde_json::to_string_pretty(settings)?)?;
    }

    let mut run = Run {
        id: run_id.clone(),
        agent_kind: adapter.kind().to_string(),
        launch_mode: "dcc".into(),
        launch_session_id: Some(run_id.clone()),
        started_ms: now_ms,
        last_state: DccState::Unknown.as_str().into(),
        last_state_ms: now_ms,
        cwd: Some(cwd.clone()),
        repo_root: info.as_ref().map(|i| i.repo_root.clone()),
        branch,
        worktree_kind: info.as_ref().map(|i| {
            if worktree_path_used.is_some() {
                "linked".to_string()
            } else {
                i.worktree_kind().to_string()
            }
        }),
        worktree_path: worktree_path_used.clone(),
        herdr_session: p.herdr_session.clone(),
        space_name: Some(p.space.clone()),
        tab_name: Some(format!("agent {short}")),
        ..Default::default()
    };
    store.insert_run(&run)?;
    store.insert_session(&run_id, &run_id, "argv", now_ms)?;

    // a tab of its own, in the named space
    let tab_name = format!("agent {short}");
    let mut argv: Vec<String> = vec![
        p.dcc_exe.to_string_lossy().into(),
        "--data-dir".into(),
        p.data_dir.to_string_lossy().into(),
        "agent".into(),
        "exec".into(),
        "--run-id".into(),
        run_id.clone(),
        "--".into(),
        plan.program.clone(),
    ];
    argv.extend(plan.args.iter().cloned());
    let labels = vec![("dcc.run".to_string(), run_id.clone())];
    let run_name = format!("dcc-{short}");
    let placed = run_in_new_tab(
        h,
        &TabSpec {
            space: &p.space,
            tab_name: &tab_name,
            cwd: &cwd,
            run_name: &run_name,
            extra_labels: &labels,
            argv: &argv,
            what: "agent",
        },
    )?;
    run.terminal_addr = Some(placed.terminal.clone());
    store.update_run(&run)?;
    let (terminal, warnings) = (placed.terminal, placed.warnings);
    Ok(json!({
        "run_id": run_id, "session_id": run_id, "terminal": terminal, "space": p.space, "tab": tab_name, "cwd": cwd,
        "worktree": worktree_path_used, "branch": run.branch, "settings": settings_path.to_string_lossy(), "warnings": warnings,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::herdr::fake::FakeHerdr;
    use crate::util::{scratch_dir, scratch_repo};

    fn params(repo: &Path, data: &Path, worktree: WorktreeMode) -> LaunchParams {
        LaunchParams {
            repo: repo.to_path_buf(),
            worktree,
            branch: None,
            model: Some("haiku".into()),
            permission_mode: Some("default".into()),
            extra_args: vec![],
            space: "agents".into(),
            program: Some(PathBuf::from("/usr/local/bin/claude")),
            dcc_exe: PathBuf::from("/x/dcc"),
            data_dir: data.to_path_buf(),
            herdr_session: Some("t".into()),
        }
    }

    #[test]
    fn a_folder_without_git_hosts_an_agent_in_place_but_not_a_worktree() {
        // the scratch folders of the tests sit inside this repository, so use the system temp folder
        let dir = std::env::temp_dir().join(format!("dcc-nogit-{}", new_uuid()));
        std::fs::create_dir_all(&dir).unwrap();
        let data = scratch_dir("launch-nogit-data");
        let store = Store::open_in_memory().unwrap();
        let h = FakeHerdr::new();
        let r = launch(&store, &h, &params(&dir, &data, WorktreeMode::Off), 1000).unwrap();
        let run = store.run(r["run_id"].as_str().unwrap()).unwrap().unwrap();
        assert_eq!(
            (run.branch, run.repo_root, run.worktree_kind),
            (None, None, None)
        );
        assert_eq!(run.cwd.as_deref(), Some(dir.to_string_lossy().as_ref()));
        assert_eq!(h.launched().len(), 1);
        let e = launch(&store, &h, &params(&dir, &data, WorktreeMode::Auto), 1000).unwrap_err();
        assert_eq!(e.kind, "usage");
        assert!(e.message.contains("separate worktree needs one"));
        assert_eq!(h.launched().len(), 1, "a refused launch starts nothing");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_launch_makes_a_worktree_a_settings_file_a_labeled_run_and_the_records() {
        let repo = scratch_repo("launch-wt");
        let data = scratch_dir("launch-data");
        let store = Store::open_in_memory().unwrap();
        let h = FakeHerdr::new();
        let r = launch(&store, &h, &params(&repo, &data, WorktreeMode::Auto), 1000).unwrap();
        let run_id = r["run_id"].as_str().unwrap();

        // worktree and branch of its own
        let wt = r["worktree"].as_str().unwrap();
        assert!(Path::new(wt).join("README.md").exists());
        let g = git::info(wt).unwrap();
        assert_eq!(
            (g.branch.as_deref(), g.worktree_kind()),
            (Some(format!("dcc/{}", &run_id[..8])).as_deref(), "linked")
        );

        // settings file outside any claude folder, with hooks that call dcc
        let settings: Value = serde_json::from_str(
            &std::fs::read_to_string(data.join("agents").join(run_id).join("settings.json"))
                .unwrap(),
        )
        .unwrap();
        assert!(
            settings["hooks"]["SessionEnd"][0]["hooks"][0]["command"]
                .as_str()
                .unwrap()
                .ends_with("agent hook-log")
        );

        // the agent runs through `dcc agent exec`, labeled, with the run id as its session id
        let launched = h.launched();
        assert_eq!(launched.len(), 1);
        let (name, argv, labels) = &launched[0];
        assert_eq!(name, &format!("dcc-{}", &run_id[..8]));
        assert_eq!(&argv[3..7], ["agent", "exec", "--run-id", run_id]);
        assert!(argv.windows(2).any(|w| w == ["--session-id", run_id]));
        assert!(
            argv.windows(2)
                .any(|w| w[0] == "--settings" && w[1].ends_with("settings.json"))
        );
        let label = |k: &str| {
            labels
                .iter()
                .find(|(key, _)| key == k)
                .map(|(_, v)| v.clone())
        };
        assert_eq!(label("dcc.run").as_deref(), Some(run_id));
        assert!(label("herdr.space").is_some() && label("herdr.tab").is_some());

        // the records
        let run = store.run(run_id).unwrap().unwrap();
        assert_eq!(
            (
                run.launch_mode.as_str(),
                run.terminal_addr.as_deref(),
                run.worktree_kind.as_deref()
            ),
            ("dcc", r["terminal"].as_str(), Some("linked"))
        );
        let sessions = store.sessions_of(run_id).unwrap();
        assert_eq!(
            (sessions.len(), sessions[0].id_source.as_str()),
            (1, "argv")
        );

        // the tab holds the agent only, the starting shell was closed
        let spaces = h.spaces().unwrap();
        assert_eq!(spaces[0].tabs[0].terminals.len(), 1);
        assert_eq!(h.live_terminals().len(), 1);
    }

    #[test]
    fn two_launches_get_two_worktrees_and_a_second_tab_in_the_same_space() {
        let repo = scratch_repo("launch-two");
        let data = scratch_dir("launch-two-data");
        let store = Store::open_in_memory().unwrap();
        let h = FakeHerdr::new();
        let a = launch(&store, &h, &params(&repo, &data, WorktreeMode::Auto), 1000).unwrap();
        let b = launch(&store, &h, &params(&repo, &data, WorktreeMode::Auto), 1001).unwrap();
        assert_ne!(a["worktree"], b["worktree"]);
        let spaces = h.spaces().unwrap();
        assert_eq!((spaces.len(), spaces[0].tabs.len()), (1, 2));
        assert_eq!(git::worktree_list(repo.to_str().unwrap()).unwrap().len(), 3);
    }

    #[test]
    fn without_a_worktree_the_agent_runs_in_the_repository_folder() {
        let repo = scratch_repo("launch-off");
        let data = scratch_dir("launch-off-data");
        let store = Store::open_in_memory().unwrap();
        let h = FakeHerdr::new();
        let r = launch(&store, &h, &params(&repo, &data, WorktreeMode::Off), 1000).unwrap();
        assert_eq!(r["worktree"], Value::Null);
        assert_eq!(r["cwd"].as_str().unwrap(), repo.to_str().unwrap());
        assert_eq!(git::worktree_list(repo.to_str().unwrap()).unwrap().len(), 1);
    }

    #[test]
    fn a_folder_that_is_not_a_repository_is_refused_before_anything_is_made() {
        let data = scratch_dir("launch-bad-data");
        let store = Store::open_in_memory().unwrap();
        let h = FakeHerdr::new();
        let p = params(Path::new("/definitely/not/here"), &data, WorktreeMode::Auto);
        assert_eq!(launch(&store, &h, &p, 1).unwrap_err().kind, "usage");
        assert!(h.spaces().unwrap().is_empty() && store.open_runs().unwrap().is_empty());
    }

    #[test]
    fn a_bad_branch_name_is_refused() {
        let repo = scratch_repo("launch-branch");
        let data = scratch_dir("launch-branch-data");
        let store = Store::open_in_memory().unwrap();
        let mut p = params(&repo, &data, WorktreeMode::Auto);
        p.branch = Some("-evil".into());
        assert_eq!(
            launch(&store, &FakeHerdr::new(), &p, 1).unwrap_err().kind,
            "usage"
        );
        assert!(store.open_runs().unwrap().is_empty());
    }
}
