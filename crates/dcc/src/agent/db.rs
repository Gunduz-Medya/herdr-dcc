//! Runs, sessions and events. Only ids, names, states, counts, paths and times are stored, never
//! prompts, answers or tool input.

use rusqlite::{OptionalExtension, Row, params};

use crate::error::Result;
use crate::store::Store;

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Run {
    pub id: String,
    pub agent_kind: String,
    /// `dcc` (started by DCC) or `external`.
    pub launch_mode: String,
    pub launch_session_id: Option<String>,
    pub started_ms: i64,
    pub ended_ms: Option<i64>,
    pub last_state: String,
    pub last_state_ms: i64,
    pub herdr_status: Option<String>,
    pub end_reason: Option<String>,
    pub exit_code: Option<i64>,
    pub cwd: Option<String>,
    pub repo_root: Option<String>,
    pub branch: Option<String>,
    pub worktree_kind: Option<String>,
    pub worktree_path: Option<String>,
    pub changed_files: Option<i64>,
    pub staged: Option<i64>,
    pub unstaged: Option<i64>,
    pub untracked: Option<i64>,
    pub herdr_session: Option<String>,
    pub terminal_addr: Option<String>,
    pub terminal_created_ms: Option<i64>,
    pub pane_id: Option<String>,
    pub space_name: Option<String>,
    pub tab_name: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Session {
    pub id: String,
    pub run_id: String,
    pub id_source: String,
    pub started_ms: i64,
    pub ended_ms: Option<i64>,
    pub end_source: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Event {
    pub run_id: String,
    pub session_id: Option<String>,
    pub ts_ms: i64,
    /// A hook name, or `state_change` (written by the snapshot when the DCC state changes).
    pub kind: String,
    /// The new DCC state, for `state_change` events.
    pub state: Option<String>,
    pub tool_name: Option<String>,
    pub notification_type: Option<String>,
}

const RUN_COLS: &str = "id, agent_kind, launch_mode, launch_session_id, started_ms, ended_ms, last_state, last_state_ms, herdr_status, end_reason, exit_code, cwd, repo_root, branch, worktree_kind, worktree_path, changed_files, staged, unstaged, untracked, herdr_session, terminal_addr, terminal_created_ms, pane_id, space_name, tab_name";

fn run_of(r: &Row) -> rusqlite::Result<Run> {
    Ok(Run {
        id: r.get(0)?,
        agent_kind: r.get(1)?,
        launch_mode: r.get(2)?,
        launch_session_id: r.get(3)?,
        started_ms: r.get(4)?,
        ended_ms: r.get(5)?,
        last_state: r.get(6)?,
        last_state_ms: r.get(7)?,
        herdr_status: r.get(8)?,
        end_reason: r.get(9)?,
        exit_code: r.get(10)?,
        cwd: r.get(11)?,
        repo_root: r.get(12)?,
        branch: r.get(13)?,
        worktree_kind: r.get(14)?,
        worktree_path: r.get(15)?,
        changed_files: r.get(16)?,
        staged: r.get(17)?,
        unstaged: r.get(18)?,
        untracked: r.get(19)?,
        herdr_session: r.get(20)?,
        terminal_addr: r.get(21)?,
        terminal_created_ms: r.get(22)?,
        pane_id: r.get(23)?,
        space_name: r.get(24)?,
        tab_name: r.get(25)?,
    })
}

fn session_of(r: &Row) -> rusqlite::Result<Session> {
    Ok(Session {
        id: r.get(0)?,
        run_id: r.get(1)?,
        id_source: r.get(2)?,
        started_ms: r.get(3)?,
        ended_ms: r.get(4)?,
        end_source: r.get(5)?,
    })
}

impl Store {
    pub fn insert_run(&self, r: &Run) -> Result<()> {
        self.conn().execute(
            &format!("INSERT INTO agent_runs ({RUN_COLS}) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17,?18,?19,?20,?21,?22,?23,?24,?25,?26)"),
            params![
                r.id, r.agent_kind, r.launch_mode, r.launch_session_id, r.started_ms, r.ended_ms, r.last_state, r.last_state_ms, r.herdr_status, r.end_reason, r.exit_code,
                r.cwd, r.repo_root, r.branch, r.worktree_kind, r.worktree_path, r.changed_files, r.staged, r.unstaged, r.untracked, r.herdr_session, r.terminal_addr,
                r.terminal_created_ms, r.pane_id, r.space_name, r.tab_name
            ],
        )?;
        Ok(())
    }

    pub fn update_run(&self, r: &Run) -> Result<()> {
        self.conn().execute(
            "UPDATE agent_runs SET agent_kind=?2, launch_mode=?3, launch_session_id=?4, started_ms=?5, ended_ms=?6, last_state=?7, last_state_ms=?8, herdr_status=?9, end_reason=?10, exit_code=?11,
             cwd=?12, repo_root=?13, branch=?14, worktree_kind=?15, worktree_path=?16, changed_files=?17, staged=?18, unstaged=?19, untracked=?20, herdr_session=?21, terminal_addr=?22,
             terminal_created_ms=?23, pane_id=?24, space_name=?25, tab_name=?26 WHERE id=?1",
            params![
                r.id, r.agent_kind, r.launch_mode, r.launch_session_id, r.started_ms, r.ended_ms, r.last_state, r.last_state_ms, r.herdr_status, r.end_reason, r.exit_code,
                r.cwd, r.repo_root, r.branch, r.worktree_kind, r.worktree_path, r.changed_files, r.staged, r.unstaged, r.untracked, r.herdr_session, r.terminal_addr,
                r.terminal_created_ms, r.pane_id, r.space_name, r.tab_name
            ],
        )?;
        Ok(())
    }

    pub fn run(&self, id: &str) -> Result<Option<Run>> {
        Ok(self
            .conn()
            .query_row(
                &format!("SELECT {RUN_COLS} FROM agent_runs WHERE id = ?1"),
                params![id],
                run_of,
            )
            .optional()?)
    }

    /// Runs that have not ended, oldest first.
    pub fn open_runs(&self) -> Result<Vec<Run>> {
        let mut stmt = self.conn().prepare(&format!(
            "SELECT {RUN_COLS} FROM agent_runs WHERE ended_ms IS NULL ORDER BY started_ms, id"
        ))?;
        let rows = stmt.query_map([], run_of)?;
        Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
    }

    /// The most recently ended runs, newest first.
    pub fn recent_closed_runs(&self, limit: i64) -> Result<Vec<Run>> {
        let mut stmt = self.conn().prepare(&format!("SELECT {RUN_COLS} FROM agent_runs WHERE ended_ms IS NOT NULL ORDER BY ended_ms DESC, id LIMIT ?1"))?;
        let rows = stmt.query_map(params![limit], run_of)?;
        Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
    }

    pub fn insert_session(
        &self,
        run_id: &str,
        id: &str,
        id_source: &str,
        started_ms: i64,
    ) -> Result<bool> {
        let n = self.conn().execute(
            "INSERT OR IGNORE INTO agent_sessions (id, run_id, id_source, started_ms) VALUES (?1, ?2, ?3, ?4)",
            params![id, run_id, id_source, started_ms],
        )?;
        Ok(n > 0)
    }

    /// Ends the open sessions of a run, except one. Returns how many were ended.
    pub fn end_open_sessions(
        &self,
        run_id: &str,
        except_id: Option<&str>,
        ts_ms: i64,
        end_source: &str,
    ) -> Result<usize> {
        Ok(self.conn().execute(
            "UPDATE agent_sessions SET ended_ms = ?3, end_source = ?4 WHERE run_id = ?1 AND ended_ms IS NULL AND id IS NOT ?2",
            params![run_id, except_id, ts_ms, end_source],
        )?)
    }

    pub fn end_session(&self, id: &str, ts_ms: i64, end_source: &str) -> Result<()> {
        self.conn().execute("UPDATE agent_sessions SET ended_ms = ?2, end_source = ?3 WHERE id = ?1 AND ended_ms IS NULL", params![id, ts_ms, end_source])?;
        Ok(())
    }

    pub fn sessions_of(&self, run_id: &str) -> Result<Vec<Session>> {
        let mut stmt = self.conn().prepare("SELECT id, run_id, id_source, started_ms, ended_ms, end_source FROM agent_sessions WHERE run_id = ?1 ORDER BY started_ms, id")?;
        let rows = stmt.query_map(params![run_id], session_of)?;
        Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
    }

    pub fn insert_event(&self, e: &Event) -> Result<()> {
        self.conn().execute(
            "INSERT INTO agent_events (run_id, session_id, ts_ms, kind, state, tool_name, notification_type) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![e.run_id, e.session_id, e.ts_ms, e.kind, e.state, e.tool_name, e.notification_type],
        )?;
        Ok(())
    }

    /// Records a change of the DCC state.
    pub fn insert_state_change(&self, run_id: &str, ts_ms: i64, state: &str) -> Result<()> {
        self.insert_event(&Event {
            run_id: run_id.to_string(),
            session_id: None,
            ts_ms,
            kind: "state_change".into(),
            state: Some(state.to_string()),
            tool_name: None,
            notification_type: None,
        })
    }

    pub fn events_of(&self, run_id: &str) -> Result<Vec<Event>> {
        let mut stmt = self.conn().prepare("SELECT run_id, session_id, ts_ms, kind, state, tool_name, notification_type FROM agent_events WHERE run_id = ?1 ORDER BY ts_ms, id")?;
        let rows = stmt.query_map(params![run_id], |r| {
            Ok(Event {
                run_id: r.get(0)?,
                session_id: r.get(1)?,
                ts_ms: r.get(2)?,
                kind: r.get(3)?,
                state: r.get(4)?,
                tool_name: r.get(5)?,
                notification_type: r.get(6)?,
            })
        })?;
        Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
    }

    /// The last hook event of a run (state changes are not hooks).
    pub fn last_hook(&self, run_id: &str) -> Result<Option<Event>> {
        Ok(self
            .events_of(run_id)?
            .into_iter()
            .rfind(|e| e.kind != "state_change"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    pub fn run(id: &str) -> Run {
        Run {
            id: id.into(),
            agent_kind: "claude".into(),
            launch_mode: "dcc".into(),
            started_ms: 1,
            last_state: "Unknown".into(),
            last_state_ms: 1,
            ..Default::default()
        }
    }

    #[test]
    fn run_round_trip_and_update() {
        let s = Store::open_in_memory().unwrap();
        let mut r = run("a");
        r.cwd = Some("/x".into());
        s.insert_run(&r).unwrap();
        assert_eq!(s.run("a").unwrap().unwrap(), r);
        r.ended_ms = Some(9);
        r.exit_code = Some(0);
        r.last_state = "Completed".into();
        s.update_run(&r).unwrap();
        assert_eq!(s.run("a").unwrap().unwrap(), r);
        assert!(s.run("zzz").unwrap().is_none());
    }

    #[test]
    fn open_and_closed_lists() {
        let s = Store::open_in_memory().unwrap();
        s.insert_run(&run("open")).unwrap();
        let mut c1 = run("c1");
        c1.ended_ms = Some(10);
        let mut c2 = run("c2");
        c2.ended_ms = Some(20);
        s.insert_run(&c1).unwrap();
        s.insert_run(&c2).unwrap();
        assert_eq!(
            s.open_runs()
                .unwrap()
                .iter()
                .map(|r| r.id.as_str())
                .collect::<Vec<_>>(),
            vec!["open"]
        );
        assert_eq!(
            s.recent_closed_runs(5)
                .unwrap()
                .iter()
                .map(|r| r.id.as_str())
                .collect::<Vec<_>>(),
            vec!["c2", "c1"]
        );
        assert_eq!(s.recent_closed_runs(1).unwrap().len(), 1);
    }

    #[test]
    fn sessions_end_in_order_and_ids_are_unique() {
        let s = Store::open_in_memory().unwrap();
        s.insert_run(&run("r")).unwrap();
        assert!(s.insert_session("r", "s1", "argv", 1).unwrap());
        assert!(
            !s.insert_session("r", "s1", "hook", 2).unwrap(),
            "same id again is ignored"
        );
        s.insert_session("r", "s2", "hook", 5).unwrap();
        assert_eq!(s.end_open_sessions("r", Some("s2"), 5, "clear").unwrap(), 1);
        let all = s.sessions_of("r").unwrap();
        assert_eq!(
            (all[0].ended_ms, all[0].end_source.as_deref()),
            (Some(5), Some("clear"))
        );
        assert!(all[1].ended_ms.is_none());
        s.end_session("s2", 9, "session_end").unwrap();
        assert_eq!(s.sessions_of("r").unwrap()[1].ended_ms, Some(9));
    }

    #[test]
    fn events_keep_only_names() {
        let s = Store::open_in_memory().unwrap();
        s.insert_run(&run("r")).unwrap();
        s.insert_event(&Event {
            run_id: "r".into(),
            session_id: Some("s".into()),
            ts_ms: 1,
            kind: "PreToolUse".into(),
            state: None,
            tool_name: Some("Write".into()),
            notification_type: None,
        })
        .unwrap();
        s.insert_event(&Event {
            run_id: "r".into(),
            session_id: None,
            ts_ms: 2,
            kind: "Stop".into(),
            state: None,
            tool_name: None,
            notification_type: None,
        })
        .unwrap();
        assert_eq!(s.last_hook("r").unwrap().unwrap().kind, "Stop");
        s.insert_state_change("r", 3, "Running").unwrap();
        assert_eq!(
            s.last_hook("r").unwrap().unwrap().kind,
            "Stop",
            "a state change is not a hook"
        );
        assert_eq!(
            s.events_of("r").unwrap().last().unwrap().state.as_deref(),
            Some("Running")
        );
        assert_eq!(s.events_of("r").unwrap().len(), 3);
    }
}
